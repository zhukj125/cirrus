import { defineStore } from "pinia";
import { computed, ref } from "vue";

import { Status, StorageKey } from "@/constants";
import { cancelDownloadTransfer, listenDownloadProgress, loadDownloadTasks, photosList, saveDownloadTasks, savePhotoTo, setDownloadSpeedLimit } from "@/lib/api";
import { readStored, writeStored } from "@/lib/common/storage";
import { toast } from "@/lib/common/toast";
import { parseDownloadTasks, serializeDownloadTasks } from "@/lib/downloads/persistence";
import { passedUpperBound, scanRangePage } from "@/lib/photos/range";
import type { DateBounds } from "@/lib/photos/dates";
import type { DownloadProgressEvent, PhotoItem } from "@/types";

export type TaskStatus = "running" | "paused" | "done" | "cancelled";

export interface SnapshotSource {
  kind: "snapshot";
  items: PhotoItem[];
}

export interface RangeSource {
  kind: "range";
  sessionId: string;
  bounds: DateBounds;

  total: number | null;

  startRank: number | null;

  nextRank: number | null;

  cursor: string | null;

  pending: PhotoItem[];

  exhausted: boolean;

  oldestTakenAt: number | null;

  seen: Set<string>;
}

export type DownloadSource = SnapshotSource | RangeSource;

export interface CreateDownloadInput {
  accountKey: string;
  title: string;
  directory: string;
  concurrency?: number;

  subfolder?: boolean;

  photos?: PhotoItem[];

  range?: {
    sessionId: string;
    bounds: DateBounds;
    total: number | null;

    startRank?: number | null;
  };
}

export interface DownloadTask {
  accountKey: string;
  id: string;

  title: string;
  directory: string;
  status: TaskStatus;

  source: DownloadSource;

  nextIndex: number;
  done: number;
  failed: string[];

  activeFiles?: string[];

  lastFile?: string | null;

  speedBytesPerSec?: number;

  usedNames: string[];
  concurrency: number;
  createdAt: number;

  error: string | null;
}

export const CONCURRENCY_OPTIONS = [2, 4, 6, 8] as const;
const DEFAULT_CONCURRENCY = 4;

export function formatSpeed(bytesPerSec: number): string {
  if (!Number.isFinite(bytesPerSec) || bytesPerSec <= 0) return "0 B/s";
  if (bytesPerSec >= 1024 * 1024) return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
  if (bytesPerSec >= 1024) return `${Math.round(bytesPerSec / 1024)} KB/s`;
  return `${Math.round(bytesPerSec)} B/s`;
}

function normalizeSpeedLimit(value: number): number {
  return Number.isFinite(value) && value > 0 ? Math.round(value) : 0;
}

function storedSpeedLimit(): number {
  return normalizeSpeedLimit(Number(readStored(StorageKey.downloadSpeedLimit)));
}

const RANGE_PAGE_SIZE = 60;

export function taskFinished(task: DownloadTask): number {
  return task.done + task.failed.length;
}

export function totalOf(task: DownloadTask): number | null {
  return task.source.kind === "snapshot" ? task.source.items.length : task.source.total;
}

export interface DownloadQueueView {
  active: Array<{ key: string; name: string }>;
  pending: Array<{ key: string; name: string; photoId: string }>;
  pendingTotal: number;

  unfetched: number | null;

  remaining: number | null;
  failed: string[];
}

export function queueView(task: DownloadTask, previewLimit = 200): DownloadQueueView {
  const active = (task.activeFiles ?? []).map((name, index) => ({ key: `${task.id}:active:${index}`, name }));
  const pendingTotal = task.source.kind === "snapshot" ? Math.max(0, task.source.items.length - task.nextIndex) : task.source.pending.length;
  const pendingItems = task.source.kind === "snapshot" ? task.source.items.slice(task.nextIndex, task.nextIndex + previewLimit) : task.source.pending.slice(0, previewLimit);
  const pending = pendingItems.map((item, index) => ({
    key: `${task.id}:pending:${item.id}:${index}`,
    name: item.fileName ?? item.id,
    photoId: item.id,
  }));
  const total = totalOf(task);
  const remaining = total === null ? null : Math.max(0, total - taskFinished(task));
  const unfetched = task.source.kind === "range" && total !== null ? Math.max(0, total - taskFinished(task) - active.length - pendingTotal) : 0;
  return { active, pending, pendingTotal, unfetched, remaining, failed: [...task.failed] };
}

export function percentOf(task: DownloadTask): number | null {
  const total = totalOf(task);
  if (total === null || total === 0) return null;
  return Math.min(100, Math.round((taskFinished(task) / total) * 100));
}

function newId(): string {
  return `dl-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function two(value: number): string {
  return String(value).padStart(2, "0");
}

function taskFolderName(title: string, createdAt: number): string {
  const date = new Date(createdAt);
  const stamp = `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())}_${two(date.getHours())}-${two(date.getMinutes())}-${two(date.getSeconds())}`;
  const safeTitle =
    title
      .replace(/[\\/:*?"<>|]/g, "_")
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, 48) || "Photos";
  return `Cirrus_${stamp}_${safeTitle}`;
}

function fallbackMediaName(item: PhotoItem, sequence: number): string {
  const extension = item.isVideo ? ".mp4" : ".jpeg";
  if (!item.takenAt) return `Cirrus_${String(sequence).padStart(4, "0")}${extension}`;
  const date = new Date(item.takenAt);
  if (Number.isNaN(date.getTime())) return `Cirrus_${String(sequence).padStart(4, "0")}${extension}`;
  const stamp = `${date.getFullYear()}${two(date.getMonth() + 1)}${two(date.getDate())}_${two(date.getHours())}${two(date.getMinutes())}${two(date.getSeconds())}`;
  return `${stamp}_${String(sequence).padStart(4, "0")}${extension}`;
}

function separatorOf(directory: string): string {
  return directory.includes("\\") ? "\\" : "/";
}

const refills = new Map<string, Promise<void>>();

export const useDownloadsStore = defineStore("downloads", () => {
  const tasks = ref<DownloadTask[]>([]);

  const activeAccountKey = ref<string | null>(null);
  const activeSessionId = ref<string | null>(null);

  const tasksByAccount = new Map<string, DownloadTask[]>();

  const loadedAccounts = new Set<string>();

  const persistenceWarning = ref<string | null>(null);

  const persistenceWritable = ref(true);
  const persistenceEnabled = ref(true);

  const downloadSpeed = ref(0);
  const speedLimitBytesPerSec = ref(storedSpeedLimit());
  const activeTransfers = new Map<string, { taskId: string; lastBytes: number; lastAt: number; speedBytesPerSec: number; item: PhotoItem; snapshotIndex: number | null }>();
  let speedTimer: ReturnType<typeof setInterval> | null = null;
  let unlistenProgress: (() => void) | null = null;

  const SAVE_DEBOUNCE_MS = 400;
  const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();
  const dirtyAccounts = new Set<string>();
  const saveFlights = new Map<string, Promise<void>>();
  let persistFailureNotified = false;

  const dialogOpen = ref(false);

  const requestedBounds = ref<DateBounds | null>(null);

  const requestedPhotos = ref<PhotoItem[] | null>(null);

  function openDialog(bounds: DateBounds | null = null): void {
    requestedBounds.value = bounds && (bounds.after !== null || bounds.before !== null) ? { ...bounds } : null;
    requestedPhotos.value = null;
    dialogOpen.value = true;
  }

  function openDialogWithPhotos(photos: PhotoItem[]): void {
    if (photos.length === 0) return;
    requestedBounds.value = null;
    requestedPhotos.value = [...photos];
    dialogOpen.value = true;
  }

  function consumeRequestedBounds(): DateBounds | null {
    const bounds = requestedBounds.value;
    requestedBounds.value = null;
    return bounds ? { ...bounds } : null;
  }

  function consumeRequestedPhotos(): PhotoItem[] | null {
    const photos = requestedPhotos.value;
    requestedPhotos.value = null;
    return photos ? [...photos] : null;
  }

  function closeDialog(): void {
    dialogOpen.value = false;
  }

  function activateAccount(accountKey: string | null, sessionId: string | null = null): void {
    if (activeAccountKey.value) tasksByAccount.set(activeAccountKey.value, tasks.value);
    activeAccountKey.value = accountKey;
    activeSessionId.value = sessionId;
    tasks.value = accountKey ? (tasksByAccount.get(accountKey) ?? []) : [];
    if (accountKey) {
      tasksByAccount.set(accountKey, tasks.value);
      applySession(accountKey, sessionId);
    }
  }

  function applySession(accountKey: string, sessionId: string | null): void {
    if (activeAccountKey.value === accountKey) activeSessionId.value = sessionId;
    if (!sessionId) return;
    const list = tasksByAccount.get(accountKey) ?? (activeAccountKey.value === accountKey ? tasks.value : []);
    for (const task of list) {
      if (task.source.kind === "range" && task.source.sessionId !== sessionId) {
        task.source.sessionId = sessionId;
        schedulePersist(accountKey);
      }
    }
  }

  function rekeyAccount(fromKey: string, toKey: string): void {
    if (!fromKey || !toKey || fromKey === toKey) return;
    const source = tasksByAccount.get(fromKey) ?? (activeAccountKey.value === fromKey ? tasks.value : []);
    for (const task of source) task.accountKey = toKey;
    const existing = tasksByAccount.get(toKey) ?? [];
    const merged = [...source, ...existing];
    tasksByAccount.delete(fromKey);
    tasksByAccount.set(toKey, merged);
    if (loadedAccounts.delete(fromKey)) loadedAccounts.add(toKey);
    if (activeAccountKey.value === fromKey) {
      activeAccountKey.value = toKey;
      tasks.value = merged;
    }
  }

  function removeAccountTasks(accountKey: string): void {
    tasksByAccount.delete(accountKey);
    loadedAccounts.delete(accountKey);
    if (activeAccountKey.value === accountKey) {
      tasks.value = [];
      activeAccountKey.value = null;
      activeSessionId.value = null;
    }
  }

  function restoreAccountTasks(accountKey: string, restored: DownloadTask[]): void {
    loadedAccounts.add(accountKey);
    const normalized = restored.map((task) => ({ ...task, accountKey }));
    tasksByAccount.set(accountKey, normalized);
    if (activeAccountKey.value === accountKey) {
      tasks.value = normalized;
      applySession(accountKey, activeSessionId.value);
    }
  }

  function isAccountLoaded(accountKey: string): boolean {
    return loadedAccounts.has(accountKey);
  }

  async function loadAccountTasks(accountKey: string, sessionId: string | null = null): Promise<void> {
    activateAccount(accountKey, sessionId);
    if (loadedAccounts.has(accountKey)) return;
    try {
      const result = await loadDownloadTasks(accountKey);
      persistenceEnabled.value = result.persistenceEnabled;
      persistenceWritable.value = result.writable;
      if (result.warning) {
        persistenceWarning.value = result.warning;
        toast.info(result.warning);
      }
      const restored = parseDownloadTasks(result.tasks, accountKey).map((task) => (task.status === "running" ? { ...task, status: "paused" as const } : task));
      restoreAccountTasks(accountKey, restored);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : "Could not read download tasks";
      persistenceWarning.value = message;
      toast.error(`${message}. This download can continue.`);
    }
  }

  function findTaskById(taskId: string): DownloadTask | null {
    for (const list of tasksByAccount.values()) {
      const task = list.find((item) => item.id === taskId);
      if (task) return task;
    }
    return tasks.value.find((item) => item.id === taskId) ?? null;
  }

  function syncTaskSpeed(taskId: string): void {
    const task = findTaskById(taskId);
    if (!task) return;
    let total = 0;
    for (const transfer of activeTransfers.values()) {
      if (transfer.taskId === taskId) total += transfer.speedBytesPerSec;
    }
    task.speedBytesPerSec = Math.round(total);
  }

  function syncDownloadSpeed(): void {
    let total = 0;
    for (const transfer of activeTransfers.values()) {
      if (findTaskById(transfer.taskId)?.status === "running") total += transfer.speedBytesPerSec;
    }
    downloadSpeed.value = Math.round(total);
  }

  function ensureSpeedTimer(): void {
    if (speedTimer) return;
    speedTimer = setInterval(() => {
      const now = performance.now();
      const touchedTasks = new Set<string>();
      for (const transfer of activeTransfers.values()) {
        if (now - transfer.lastAt <= 1500) continue;
        transfer.speedBytesPerSec = transfer.speedBytesPerSec < 512 ? 0 : transfer.speedBytesPerSec * 0.6;
        touchedTasks.add(transfer.taskId);
      }
      for (const taskId of touchedTasks) syncTaskSpeed(taskId);
      syncDownloadSpeed();
      if (activeTransfers.size === 0 && downloadSpeed.value === 0) {
        if (speedTimer) clearInterval(speedTimer);
        speedTimer = null;
      }
    }, 500);
  }

  function ingestDownloadProgress(progress: DownloadProgressEvent): void {
    const transfer = activeTransfers.get(progress.transferId);
    if (!transfer) return;
    const task = findTaskById(transfer.taskId);
    if (!task) return;
    const now = performance.now();
    const delta = Math.max(0, progress.downloadedBytes - transfer.lastBytes);
    const elapsed = Math.max(0.001, (now - transfer.lastAt) / 1000);
    if (delta > 0) {
      const instant = delta / elapsed;
      transfer.speedBytesPerSec = transfer.speedBytesPerSec * 0.65 + instant * 0.35;
      transfer.lastAt = now;
      syncTaskSpeed(task.id);
      syncDownloadSpeed();
    }
    transfer.lastBytes = Math.max(transfer.lastBytes, progress.downloadedBytes);
    ensureSpeedTimer();
  }

  function resetSpeedDisplay(): void {
    downloadSpeed.value = 0;
    for (const transfer of activeTransfers.values()) transfer.speedBytesPerSec = 0;
    if (speedTimer) clearInterval(speedTimer);
    speedTimer = null;
  }

  function syncSpeedDisplayWithRunningTransfers(): void {
    const hasRunningTransfer = [...activeTransfers.values()].some((transfer) => findTaskById(transfer.taskId)?.status === "running");
    if (!hasRunningTransfer) resetSpeedDisplay();
    else syncDownloadSpeed();
  }
  function cancelTransfersForTask(taskId: string): void {
    for (const [transferId, transfer] of activeTransfers) {
      if (transfer.taskId === taskId) void cancelDownloadTransfer(transferId).catch(() => undefined);
    }
  }

  function requeueCancelledItem(task: DownloadTask, item: PhotoItem, snapshotIndex: number | null): void {
    if (task.source.kind === "snapshot") {
      if (snapshotIndex !== null) task.nextIndex = Math.min(task.nextIndex, snapshotIndex);
      return;
    }
    if (!task.source.pending.some((pending) => pending.id === item.id)) task.source.pending.unshift(item);
  }
  function beginTransfer(task: DownloadTask, fileName: string, item: PhotoItem, snapshotIndex: number | null): string {
    const transferId = `${task.id}:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 8)}`;
    activeTransfers.set(transferId, { taskId: task.id, lastBytes: 0, lastAt: performance.now(), speedBytesPerSec: 0, item, snapshotIndex });
    task.activeFiles = [...(task.activeFiles ?? []), fileName];
    task.speedBytesPerSec = task.speedBytesPerSec ?? 0;
    ensureSpeedTimer();
    return transferId;
  }

  function endTransfer(transferId: string, fileName: string): void {
    const transfer = activeTransfers.get(transferId);
    activeTransfers.delete(transferId);
    const task = transfer ? findTaskById(transfer.taskId) : null;
    if (task) {
      syncTaskSpeed(task.id);
      syncDownloadSpeed();
      task.lastFile = fileName;
      const index = task.activeFiles?.indexOf(fileName) ?? -1;
      if (index >= 0 && task.activeFiles) task.activeFiles.splice(index, 1);
      if (!task.activeFiles?.length) {
        setTimeout(() => {
          if (!task.activeFiles?.length) task.speedBytesPerSec = 0;
        }, 2000);
      }
    }
  }

  function setSpeedLimit(bytesPerSec: number): void {
    const value = normalizeSpeedLimit(bytesPerSec);
    speedLimitBytesPerSec.value = value;
    writeStored(StorageKey.downloadSpeedLimit, String(value));
    void setDownloadSpeedLimit(value).catch((error) => {
      toast.error(`Could not update the speed limit: ${error instanceof Error ? error.message : "Unknown error"}`);
    });
  }

  async function initDownloadTracking(): Promise<void> {
    await setDownloadSpeedLimit(speedLimitBytesPerSec.value).catch(() => undefined);
    if (unlistenProgress) return;
    try {
      unlistenProgress = await listenDownloadProgress(ingestDownloadProgress);
    } catch {}
  }

  function disposeDownloadTracking(): void {
    unlistenProgress?.();
    unlistenProgress = null;
    if (speedTimer) clearInterval(speedTimer);
    speedTimer = null;
    downloadSpeed.value = 0;
  }
  function taskListFor(accountKey: string): DownloadTask[] {
    return tasksByAccount.get(accountKey) ?? (activeAccountKey.value === accountKey ? tasks.value : []);
  }

  function schedulePersist(accountKey: string | null, immediate = false): void {
    if (!accountKey || !persistenceEnabled.value || !persistenceWritable.value) return;
    dirtyAccounts.add(accountKey);
    const existingTimer = saveTimers.get(accountKey);
    if (existingTimer) clearTimeout(existingTimer);
    saveTimers.delete(accountKey);
    if (immediate) {
      void flushAccount(accountKey);
      return;
    }
    saveTimers.set(
      accountKey,
      setTimeout(() => {
        saveTimers.delete(accountKey);
        void flushAccount(accountKey);
      }, SAVE_DEBOUNCE_MS),
    );
  }

  async function flushAccount(accountKey: string): Promise<void> {
    const timer = saveTimers.get(accountKey);
    if (timer) clearTimeout(timer);
    saveTimers.delete(accountKey);

    const inFlight = saveFlights.get(accountKey);
    if (inFlight) {
      await inFlight;
      if (dirtyAccounts.has(accountKey)) await flushAccount(accountKey);
      return;
    }
    if (!dirtyAccounts.has(accountKey) || !persistenceEnabled.value || !persistenceWritable.value) return;

    dirtyAccounts.delete(accountKey);
    let failed = false;
    const payload = serializeDownloadTasks(taskListFor(accountKey));
    const flight = saveDownloadTasks(accountKey, payload)
      .then(() => {
        persistenceWarning.value = null;
        persistFailureNotified = false;
      })
      .catch((caught: unknown) => {
        failed = true;
        dirtyAccounts.add(accountKey);
        const message = caught instanceof Error ? caught.message : "Could not save download tasks";
        persistenceWarning.value = message;
        if (!persistFailureNotified) {
          persistFailureNotified = true;
          toast.error(`${message}. This download can continue, but it may not resume after restart.`);
        }
      })
      .finally(() => {
        saveFlights.delete(accountKey);
      });
    saveFlights.set(accountKey, flight);
    await flight;
    if (!failed && dirtyAccounts.has(accountKey)) await flushAccount(accountKey);
  }

  async function flushPersistence(): Promise<void> {
    const pending = new Set([...dirtyAccounts, ...saveTimers.keys(), ...saveFlights.keys()]);
    await Promise.all([...pending].map((accountKey) => flushAccount(accountKey)));
  }

  const running = computed(() => tasks.value.filter((task) => task.status === "running"));
  const active = computed(() => tasks.value.filter((task) => task.status === "running" || task.status === "paused"));

  const summary = computed(() => {
    const list = active.value;
    if (list.length === 0) return null;
    let finished = 0;

    let total: number | null = 0;
    for (const task of list) {
      finished += taskFinished(task);
      const taskTotal = totalOf(task);
      total = taskTotal === null || total === null ? null : total + taskTotal;
    }
    return {
      tasks: list.length,
      running: running.value.length,
      finished,
      total,
      percent: total === null || total === 0 ? null : Math.round((finished / total) * 100),
    };
  });

  function uniqueName(task: DownloadTask, name: string): string {
    const used = new Set(task.usedNames);

    const dot = name.lastIndexOf(".");
    const stem = dot > 0 ? name.slice(0, dot) : name;
    const ext = dot > 0 ? name.slice(dot) : "";
    let candidate = name;
    let suffix = 2;
    while (used.has(candidate.toLowerCase())) {
      candidate = `${stem}-${suffix}${ext}`;
      suffix += 1;
    }
    task.usedNames.push(candidate.toLowerCase());
    return candidate;
  }

  async function refillPage(task: DownloadTask, source: RangeSource): Promise<void> {
    const cursorBefore = source.cursor;
    const rankBefore = source.nextRank;
    const usingRank = rankBefore !== null;
    const page = usingRank ? await photosList({ sessionId: source.sessionId, startRank: rankBefore, limit: RANGE_PAGE_SIZE }) : await photosList({ sessionId: source.sessionId, cursor: source.cursor, limit: RANGE_PAGE_SIZE });

    if (task.status !== "running") return;

    if (page.status === Status.error) {
      if (usingRank) {
        source.startRank = null;
        source.nextRank = null;
        return;
      }

      source.exhausted = true;
      task.error = page.message ?? "Could not load the photo list";
      toast.error(task.error);
      return;
    }

    const scan = scanRangePage(page.photos, source.bounds, source.oldestTakenAt);
    source.oldestTakenAt = scan.oldestTakenAt;
    if (usingRank) {
      source.nextRank = page.photos.length > 0 ? rankBefore - page.photos.length : null;

      if (passedUpperBound(page.photos, source.bounds) || page.photos.length === 0 || (source.nextRank ?? -1) < 0) {
        source.exhausted = true;
      }
    } else {
      source.cursor = page.nextCursor;

      if (scan.passedLowerBound || page.nextCursor === null || page.nextCursor === cursorBefore) {
        source.exhausted = true;
      }
    }

    for (const item of scan.matched) {
      if (source.seen.has(item.id)) continue;
      source.seen.add(item.id);
      source.pending.push(item);
    }
    schedulePersist(task.accountKey);
  }

  function refill(task: DownloadTask, source: RangeSource): Promise<void> {
    const inFlight = refills.get(task.id);
    if (inFlight) return inFlight;

    const promise = refillPage(task, source)
      .catch((caught: unknown) => {
        source.exhausted = true;
        task.error = caught instanceof Error ? caught.message : "Could not load the photo list";
        toast.error(task.error);
      })

      .finally(() => {
        refills.delete(task.id);
      });

    refills.set(task.id, promise);
    return promise;
  }

  async function takeFromRange(task: DownloadTask, source: RangeSource): Promise<PhotoItem | null> {
    while (source.pending.length === 0) {
      if (source.exhausted) return null;
      await refill(task, source);

      if (task.status !== "running") return null;
    }
    return source.pending.shift() ?? null;
  }

  async function worker(task: DownloadTask): Promise<void> {
    while (task.status === "running") {
      let item: PhotoItem | null;
      let snapshotIndex: number | null = null;
      if (task.source.kind === "snapshot") {
        const index = task.nextIndex;
        if (index >= task.source.items.length) return;
        task.nextIndex = index + 1;
        snapshotIndex = index;
        item = task.source.items[index] ?? null;
      } else {
        item = await takeFromRange(task, task.source);
      }
      if (item === null) return;

      const url = item.originalUrl || item.displayUrl || item.thumbUrl;
      const rawName = item.fileName || fallbackMediaName(item, task.done + task.failed.length + 1);

      if (!url) {
        task.failed.push(rawName);
        task.lastFile = rawName;
        schedulePersist(task.accountKey);
        continue;
      }

      const transferId = beginTransfer(task, rawName, item, snapshotIndex);
      try {
        await savePhotoTo(url, rawName, `${task.directory}${separatorOf(task.directory)}${uniqueName(task, rawName)}`, transferId);
        task.done += 1;
      } catch {
        const currentStatus = String(task.status);
        if (currentStatus === "paused" || currentStatus === "cancelled") {
          requeueCancelledItem(task, item, snapshotIndex);
        } else {
          task.failed.push(rawName);
        }
      } finally {
        endTransfer(transferId, rawName);
      }
      schedulePersist(task.accountKey);
    }
  }

  function drained(task: DownloadTask): boolean {
    if (task.source.kind === "snapshot") return task.nextIndex >= task.source.items.length;
    return task.source.exhausted && task.source.pending.length === 0;
  }

  async function pump(task: DownloadTask): Promise<void> {
    if (task.status !== "running") return;
    await Promise.all(Array.from({ length: task.concurrency }, () => worker(task)));

    if (task.status !== "running") return;
    task.status = drained(task) ? "done" : "paused";
    schedulePersist(task.accountKey, true);
  }

  function create(input: CreateDownloadInput): DownloadTask | null {
    if (!input.accountKey || input.accountKey !== activeAccountKey.value) {
      toast.error("Sign in to the account that owns this task");
      return null;
    }
    const range = input.range;
    const source: DownloadSource | null =
      range && range.sessionId
        ? {
            kind: "range",
            sessionId: range.sessionId,

            bounds: { ...range.bounds },
            total: range.total,

            startRank: range.startRank ?? null,
            nextRank: range.startRank ?? null,
            cursor: null,
            pending: [],
            exhausted: false,
            oldestTakenAt: null,
            seen: new Set<string>(),
          }
        : input.photos && input.photos.length > 0
          ? { kind: "snapshot", items: [...input.photos] }
          : null;

    if (!source) {
      toast.error("No photos are available to download");
      return null;
    }

    const id = newId();
    const createdAt = Date.now();
    const directory = input.subfolder === true ? `${input.directory}${separatorOf(input.directory)}${taskFolderName(input.title, createdAt)}` : input.directory;
    const task: DownloadTask = {
      accountKey: input.accountKey,
      id,
      title: input.title,
      directory,
      status: "running",
      source,
      nextIndex: 0,
      done: 0,
      failed: [],
      activeFiles: [],
      lastFile: null,
      speedBytesPerSec: 0,
      usedNames: [],
      concurrency: input.concurrency ?? DEFAULT_CONCURRENCY,
      createdAt: Date.now(),
      error: null,
    };
    tasks.value = [task, ...tasks.value];
    tasksByAccount.set(input.accountKey, tasks.value);
    schedulePersist(input.accountKey, true);

    const tracked = tasks.value[0] ?? task;
    toast.success(`Download task created: ${task.title}`);
    void pump(tracked);
    return tracked;
  }

  function pause(id: string): void {
    const task = tasks.value.find((item) => item.id === id);
    if (task?.status === "running") {
      task.status = "paused";
      task.speedBytesPerSec = 0;
      cancelTransfersForTask(task.id);
      syncSpeedDisplayWithRunningTransfers();
      schedulePersist(task.accountKey, true);
    }
  }

  function resume(id: string): void {
    const task = tasks.value.find((item) => item.id === id);
    if (!task || (task.status !== "paused" && task.status !== "cancelled")) return;
    if (!activeSessionId.value) {
      toast.error("This account session is unavailable. Sign in again to continue.");
      return;
    }
    task.status = "running";
    task.error = null;
    schedulePersist(task.accountKey, true);
    void pump(task);
  }

  function cancel(id: string): void {
    const task = tasks.value.find((item) => item.id === id);
    if (task) {
      task.status = "cancelled";
      task.speedBytesPerSec = 0;
      cancelTransfersForTask(task.id);
      syncSpeedDisplayWithRunningTransfers();
      schedulePersist(task.accountKey, true);
    }
  }

  function remove(id: string): void {
    const task = tasks.value.find((item) => item.id === id);
    const accountKey = task?.accountKey ?? activeAccountKey.value;
    if (task) {
      task.status = "cancelled";
      task.speedBytesPerSec = 0;
      cancelTransfersForTask(task.id);
      syncSpeedDisplayWithRunningTransfers();
    }
    tasks.value = tasks.value.filter((item) => item.id !== id);
    if (activeAccountKey.value) tasksByAccount.set(activeAccountKey.value, tasks.value);
    schedulePersist(accountKey, true);
  }

  function clearFinished(): void {
    tasks.value = tasks.value.filter((task) => task.status === "running" || task.status === "paused");
    if (activeAccountKey.value) tasksByAccount.set(activeAccountKey.value, tasks.value);
    schedulePersist(activeAccountKey.value, true);
  }

  return {
    tasks,
    activeAccountKey,
    activeSessionId,
    persistenceWarning,
    persistenceWritable,
    persistenceEnabled,
    downloadSpeed,
    speedLimitBytesPerSec,
    running,
    active,
    summary,
    dialogOpen,
    openDialog,
    openDialogWithPhotos,
    consumeRequestedBounds,
    consumeRequestedPhotos,
    closeDialog,
    activateAccount,
    applySession,
    rekeyAccount,
    removeAccountTasks,
    restoreAccountTasks,
    isAccountLoaded,
    loadAccountTasks,
    schedulePersist,
    flushPersistence,
    initDownloadTracking,
    disposeDownloadTracking,
    setSpeedLimit,
    ingestDownloadProgress,
    create,
    pause,
    resume,
    cancel,
    remove,
    clearFinished,
  };
});
