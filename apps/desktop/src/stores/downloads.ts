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
  pending: PhotoItem[];
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

export interface CompletedFile {
  name: string;
  sizeBytes: number | null;
}

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
  failedItems: PhotoItem[];
  completedFiles?: CompletedFile[];
  pausedItems?: PhotoItem[];
  pausedProgress?: Array<{ photoId: string; downloadedBytes: number; totalBytes: number | null }>;

  activeFiles?: string[];

  activeItems?: PhotoItem[];

  activeProgress?: Array<{ transferId: string; downloadedBytes: number; totalBytes: number | null }>;

  transitionItems?: PhotoItem[];

  transitionOrder?: string[];

  lastFile?: string | null;

  speedBytesPerSec?: number;

  usedNames: string[];
  concurrency: number;
  createdAt: number;

  error: string | null;
}

export const CONCURRENCY_OPTIONS = [2, 4, 6, 8] as const;
const DEFAULT_CONCURRENCY = 4;

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${Math.round(bytes)} B`;
}

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
const RANGE_PREFETCH_THRESHOLD = RANGE_PAGE_SIZE;

export function taskFinished(task: DownloadTask): number {
  return task.done + task.failed.length;
}

export function totalOf(task: DownloadTask): number | null {
  return task.source.kind === "snapshot" ? task.source.items.length : task.source.total;
}

export interface DownloadQueueView {
  active: Array<{ key: string; name: string; photoId: string | null; sizeBytes: number | null; downloadedBytes: number; totalBytes: number | null }>;
  pending: Array<{ key: string; name: string; photoId: string; sizeBytes: number | null }>;
  paused: Array<{ key: string; name: string; photoId: string; sizeBytes: number | null; downloadedBytes: number; totalBytes: number | null }>;
  pendingTotal: number;

  unfetched: number | null;

  remaining: number | null;
  failed: string[];
}

function uniquePhotosById(items: PhotoItem[]): PhotoItem[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    if (seen.has(item.id)) return false;
    seen.add(item.id);
    return true;
  });
}

export function queueView(task: DownloadTask, previewLimit = 200): DownloadQueueView {
  const pausedItems = uniquePhotosById(Array.isArray(task.pausedItems) ? task.pausedItems : []);
  const pausedIds = new Set(pausedItems.map((item) => item.id));
  const transitionItems = uniquePhotosById(Array.isArray(task.transitionItems) ? task.transitionItems : []);
  const transitionIds = new Set(transitionItems.map((item) => item.id));
  const activeIds = new Set<string>();
  const active = (task.activeFiles ?? [])
    .map((name, index) => {
      const progress = task.activeProgress?.[index];
      const photo = task.activeItems?.[index];
      return {
        key: `${task.id}:active:${index}`,
        name,
        photoId: photo?.id ?? null,
        sizeBytes: photo?.sizeBytes ?? progress?.totalBytes ?? null,
        downloadedBytes: progress?.downloadedBytes ?? 0,
        totalBytes: progress?.totalBytes ?? photo?.sizeBytes ?? null,
      };
    })
    .filter((item) => {
      if (item.photoId !== null && (pausedIds.has(item.photoId) || activeIds.has(item.photoId))) return false;
      if (item.photoId !== null) activeIds.add(item.photoId);
      return true;
    });
  for (const item of transitionItems) {
    if (pausedIds.has(item.id) || activeIds.has(item.id)) continue;
    const progress = task.pausedProgress?.find((entry) => entry.photoId === item.id);
    active.push({
      key: `${task.id}:transition:${item.id}`,
      name: item.fileName || fallbackMediaName(item, task.done + active.length + 1),
      photoId: item.id,
      sizeBytes: item.sizeBytes ?? progress?.totalBytes ?? null,
      downloadedBytes: progress?.downloadedBytes ?? 0,
      totalBytes: progress?.totalBytes ?? item.sizeBytes ?? null,
    });
    activeIds.add(item.id);
  }
  const transitionOrder = new Map((task.transitionOrder ?? []).map((id, index) => [id, index]));
  if (transitionOrder.size > 0) {
    active.sort((left, right) => {
      const leftRank = left.photoId === null ? Number.MAX_SAFE_INTEGER : (transitionOrder.get(left.photoId) ?? Number.MAX_SAFE_INTEGER);
      const rightRank = right.photoId === null ? Number.MAX_SAFE_INTEGER : (transitionOrder.get(right.photoId) ?? Number.MAX_SAFE_INTEGER);
      return leftRank - rightRank;
    });
  }
  const queuedSnapshotItems = task.source.kind === "snapshot" && Array.isArray(task.source.pending) ? task.source.pending : [];
  const pendingCandidates = task.source.kind === "snapshot" ? uniquePhotosById([...queuedSnapshotItems, ...task.source.items.slice(task.nextIndex)]) : uniquePhotosById(task.source.pending);
  const pendingItems = pendingCandidates.filter((item) => !pausedIds.has(item.id) && !transitionIds.has(item.id)).slice(0, previewLimit);
  const pendingTotal = pendingCandidates.filter((item) => !pausedIds.has(item.id) && !transitionIds.has(item.id)).length;
  const paused = pausedItems.map((item, index) => {
    const progress = task.pausedProgress?.find((entry) => entry.photoId === item.id);
    return {
      key: `${task.id}:paused:${item.id}:${index}`,
      name: item.fileName || fallbackMediaName(item, task.done + index + 1),
      photoId: item.id,
      sizeBytes: item.sizeBytes ?? null,
      downloadedBytes: progress?.downloadedBytes ?? 0,
      totalBytes: progress?.totalBytes ?? item.sizeBytes ?? null,
    };
  });
  const pending = pendingItems.map((item, index) => ({
    key: `${task.id}:pending:${item.id}:${index}`,
    name: item.fileName || fallbackMediaName(item, task.done + index + 1),
    photoId: item.id,
    sizeBytes: item.sizeBytes ?? null,
  }));
  const total = totalOf(task);
  const remaining = total === null ? null : Math.max(0, total - taskFinished(task));
  const unfetched = task.source.kind === "range" && total !== null ? Math.max(0, total - taskFinished(task) - active.length - pendingTotal) : 0;
  return { active, pending, paused, pendingTotal, unfetched, remaining, failed: [...task.failed] };
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

export function fallbackMediaName(item: PhotoItem, sequence: number): string {
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

function downloadUrls(item: PhotoItem): string[] {
  const candidates = item.isVideo ? [item.originalUrl, item.videoUrl, item.displayUrl, item.thumbUrl] : [item.originalUrl, item.displayUrl, item.thumbUrl];
  return [...new Set(candidates.map((url) => url?.trim()).filter((url): url is string => Boolean(url)))];
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
  const cancelledTransfers = new Set<string>();
  const pumpFlights = new Map<string, Promise<void>>();
  const runTokens = new Map<string, number>();
  const pumpRestartRequests = new Set<string>();
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
    const paused = isItemPaused(task, transfer.item.id);
    const now = performance.now();
    const delta = Math.max(0, progress.downloadedBytes - transfer.lastBytes);
    const elapsed = Math.max(0.001, (now - transfer.lastAt) / 1000);
    if (delta > 0 && !paused) {
      const instant = delta / elapsed;
      transfer.speedBytesPerSec = transfer.speedBytesPerSec * 0.65 + instant * 0.35;
      transfer.lastAt = now;
      syncTaskSpeed(task.id);
      syncDownloadSpeed();
    }
    transfer.lastBytes = Math.max(transfer.lastBytes, progress.downloadedBytes);
    const taskProgress = task.activeProgress?.find((item) => item.transferId === progress.transferId);
    if (taskProgress) {
      taskProgress.downloadedBytes = Math.max(taskProgress.downloadedBytes, progress.downloadedBytes);
      taskProgress.totalBytes = progress.totalBytes ?? taskProgress.totalBytes;
    }
    if (paused) {
      transfer.speedBytesPerSec = 0;
      syncTaskSpeed(task.id);
      syncDownloadSpeed();
      addPausedItem(task, transfer.item, progress.downloadedBytes, progress.totalBytes ?? taskProgress?.totalBytes ?? transfer.item.sizeBytes ?? null);
    }
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
      if (transfer.taskId !== taskId) continue;
      cancelledTransfers.add(transferId);
      void cancelDownloadTransfer(transferId).catch(() => undefined);
    }
  }

  function isItemPaused(task: DownloadTask, photoId: string): boolean {
    return (task.pausedItems ?? []).some((item) => item.id === photoId);
  }

  function clearTransition(task: DownloadTask): void {
    task.transitionItems = [];
    task.transitionOrder = [];
  }

  function removeTransitionItem(task: DownloadTask, photoId: string): void {
    task.transitionItems = (task.transitionItems ?? []).filter((item) => item.id !== photoId);
    task.transitionOrder = (task.transitionOrder ?? []).filter((id) => id !== photoId);
    syncTransition(task);
  }

  function syncTransition(task: DownloadTask): void {
    if (!task.transitionItems?.length) {
      clearTransition(task);
      return;
    }
    const activeIds = new Set((task.activeItems ?? []).map((item) => item.id));
    if (task.transitionItems.every((item) => activeIds.has(item.id))) clearTransition(task);
  }

  function removeQueuedItem(task: DownloadTask, photoId: string): void {
    if (!Array.isArray(task.source.pending)) task.source.pending = [];
    task.source.pending = task.source.pending.filter((item) => item.id !== photoId);
  }

  function findActiveTransfer(taskId: string, photoId: string): { transferId: string; item: PhotoItem } | null {
    for (const [transferId, transfer] of activeTransfers) {
      if (transfer.taskId === taskId && transfer.item.id === photoId) return { transferId, item: transfer.item };
    }
    return null;
  }

  function hasActiveTransfer(taskId: string, photoId: string): boolean {
    return findActiveTransfer(taskId, photoId) !== null;
  }

  function addPausedItem(task: DownloadTask, item: PhotoItem, downloadedBytes = 0, totalBytes: number | null = item.sizeBytes ?? null): void {
    if (!Array.isArray(task.pausedItems)) task.pausedItems = [];
    if (!Array.isArray(task.pausedProgress)) task.pausedProgress = [];
    if (!isItemPaused(task, item.id)) task.pausedItems.push({ ...item });
    const existing = task.pausedProgress.find((entry) => entry.photoId === item.id);
    if (existing) {
      existing.downloadedBytes = Math.max(existing.downloadedBytes, downloadedBytes);
      existing.totalBytes = totalBytes ?? existing.totalBytes;
    } else {
      task.pausedProgress.push({ photoId: item.id, downloadedBytes, totalBytes });
    }
  }

  function removePausedItem(task: DownloadTask, photoId: string): void {
    if (!Array.isArray(task.pausedItems)) return;
    task.pausedItems = task.pausedItems.filter((item) => item.id !== photoId);
  }

  function pausedProgressFor(task: DownloadTask, photoId: string): { downloadedBytes: number; totalBytes: number | null } | null {
    return task.pausedProgress?.find((entry) => entry.photoId === photoId) ?? null;
  }

  function cancelTransferForItem(taskId: string, photoId: string): void {
    for (const [transferId, transfer] of activeTransfers) {
      if (transfer.taskId === taskId && transfer.item.id === photoId) {
        cancelledTransfers.add(transferId);
        void cancelDownloadTransfer(transferId).catch(() => undefined);
      }
    }
  }

  function requeueCancelledItem(task: DownloadTask, item: PhotoItem, _snapshotIndex: number | null): void {
    if (!Array.isArray(task.source.pending)) task.source.pending = [];
    if (!task.source.pending.some((pending) => pending.id === item.id)) task.source.pending.unshift(item);
  }
  function beginTransfer(task: DownloadTask, fileName: string, item: PhotoItem, snapshotIndex: number | null): string {
    const transferId = `${task.id}:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 8)}`;
    activeTransfers.set(transferId, { taskId: task.id, lastBytes: 0, lastAt: performance.now(), speedBytesPerSec: 0, item, snapshotIndex });
    task.activeFiles = [...(task.activeFiles ?? []), fileName];
    task.activeItems = [...(task.activeItems ?? []), item];
    const pausedProgress = pausedProgressFor(task, item.id);
    task.activeProgress = [...(task.activeProgress ?? []), { transferId, downloadedBytes: pausedProgress?.downloadedBytes ?? 0, totalBytes: pausedProgress?.totalBytes ?? item.sizeBytes ?? null }];
    task.speedBytesPerSec = task.speedBytesPerSec ?? 0;
    syncTransition(task);
    ensureSpeedTimer();
    return transferId;
  }

  function endTransfer(transferId: string, fileName: string): void {
    const transfer = activeTransfers.get(transferId);
    activeTransfers.delete(transferId);
    cancelledTransfers.delete(transferId);
    const task = transfer ? findTaskById(transfer.taskId) : null;
    if (task) {
      syncTaskSpeed(task.id);
      syncDownloadSpeed();
      task.lastFile = fileName;
      const index = task.activeProgress?.findIndex((progress) => progress.transferId === transferId) ?? -1;
      if (index >= 0) {
        task.activeFiles?.splice(index, 1);
        task.activeItems?.splice(index, 1);
        task.activeProgress?.splice(index, 1);
      }
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

  function recordFailedItem(task: DownloadTask, item: PhotoItem, rawName: string): void {
    if (!Array.isArray(task.failedItems)) task.failedItems = [];
    if (task.failedItems.some((failed) => failed.id === item.id)) return;
    task.failedItems.push({ ...item });
    task.failed.push(rawName);
    removeTransitionItem(task, item.id);
  }

  function releaseName(task: DownloadTask, name: string): void {
    const index = task.usedNames.lastIndexOf(name.toLowerCase());
    if (index >= 0) task.usedNames.splice(index, 1);
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
        schedulePersist(task.accountKey);
        return;
      }

      source.exhausted = true;
      task.error = page.message ?? "Could not load the photo list";
      toast.error(task.error);
      return;
    }

    if (usingRank && page.photos.length === 0) {
      source.startRank = null;
      source.nextRank = null;
      schedulePersist(task.accountKey);
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

  function isCurrentRun(task: DownloadTask, token: number): boolean {
    return task.status === "running" && runTokens.get(task.id) === token;
  }

  function invalidateRun(task: DownloadTask): void {
    runTokens.set(task.id, (runTokens.get(task.id) ?? 0) + 1);
  }

  async function takeFromRange(task: DownloadTask, source: RangeSource, token: number): Promise<PhotoItem | null> {
    while (isCurrentRun(task, token) && source.pending.length === 0) {
      if (source.exhausted) return null;
      await refill(task, source);
      if (!isCurrentRun(task, token)) return null;
    }
    if (!isCurrentRun(task, token)) return null;

    const item = source.pending.shift() ?? null;
    if (item && !source.exhausted && source.pending.length <= RANGE_PREFETCH_THRESHOLD) void refill(task, source);
    return item;
  }

  async function worker(task: DownloadTask, token: number): Promise<void> {
    while (isCurrentRun(task, token)) {
      let item: PhotoItem | null;
      let snapshotIndex: number | null = null;
      if (task.source.kind === "snapshot") {
        if (!Array.isArray(task.source.pending)) task.source.pending = [];
        const queued = task.source.pending.shift();
        if (queued) {
          item = queued;
        } else {
          const index = task.nextIndex;
          if (index >= task.source.items.length) return;
          task.nextIndex = index + 1;
          snapshotIndex = index;
          item = task.source.items[index] ?? null;
        }
      } else {
        item = await takeFromRange(task, task.source, token);
      }
      if (item === null) return;
      if (!isCurrentRun(task, token)) {
        if (!isItemPaused(task, item.id)) requeueCancelledItem(task, item, snapshotIndex);
        return;
      }
      if (isItemPaused(task, item.id) || hasActiveTransfer(task.id, item.id)) continue;

      const urls = downloadUrls(item);
      const rawName = item.fileName || fallbackMediaName(item, task.done + task.failed.length + 1);

      if (urls.length === 0) {
        recordFailedItem(task, item, rawName);
        task.lastFile = rawName;
        schedulePersist(task.accountKey);
        continue;
      }

      const savedName = uniqueName(task, rawName);
      const targetPath = `${task.directory}${separatorOf(task.directory)}${savedName}`;
      let completed = false;
      let interrupted = false;
      for (const url of urls) {
        const transferId = beginTransfer(task, rawName, item, snapshotIndex);
        try {
          const saved = await savePhotoTo(url, rawName, targetPath, transferId);
          if (!Array.isArray(task.completedFiles)) task.completedFiles = [];
          task.completedFiles.push({ name: savedName, sizeBytes: saved.sizeBytes });
          removeQueuedItem(task, item.id);
          removePausedItem(task, item.id);
          removeTransitionItem(task, item.id);
          task.pausedProgress = (task.pausedProgress ?? []).filter((entry) => entry.photoId !== item.id);
          task.done += 1;
          completed = true;
          break;
        } catch {
          const currentStatus = String(task.status);
          const itemPaused = isItemPaused(task, item.id);
          if (!isCurrentRun(task, token) || currentStatus === "paused" || currentStatus === "cancelled") {
            if (!itemPaused) requeueCancelledItem(task, item, snapshotIndex);
            interrupted = true;
            break;
          }
          if (cancelledTransfers.has(transferId) || itemPaused) {
            interrupted = true;
            break;
          }
        } finally {
          endTransfer(transferId, rawName);
        }
      }
      if (!completed && !interrupted) {
        releaseName(task, savedName);
        recordFailedItem(task, item, rawName);
      } else if (interrupted) {
        releaseName(task, savedName);
      }
      schedulePersist(task.accountKey);
    }
  }

  function drained(task: DownloadTask): boolean {
    if ((task.pausedItems?.length ?? 0) > 0) return false;
    if (task.source.kind === "snapshot") return task.nextIndex >= task.source.items.length && (task.source.pending?.length ?? 0) === 0;
    return task.source.exhausted && task.source.pending.length === 0;
  }

  function pump(task: DownloadTask): Promise<void> {
    const existing = pumpFlights.get(task.id);
    if (existing) {
      pumpRestartRequests.add(task.id);
      return existing;
    }
    if (task.status !== "running") return Promise.resolve();

    const token = (runTokens.get(task.id) ?? 0) + 1;
    runTokens.set(task.id, token);
    let run: Promise<void>;
    run = Promise.all(Array.from({ length: task.concurrency }, () => worker(task, token)))
      .then(() => {
        if (!isCurrentRun(task, token)) return;
        task.status = drained(task) ? "done" : "paused";
        schedulePersist(task.accountKey, true);
      })
      .finally(() => {
        if (pumpFlights.get(task.id) === run) pumpFlights.delete(task.id);
        const restart = pumpRestartRequests.delete(task.id);
        if (task.status === "running" && (restart || runTokens.get(task.id) !== token)) void pump(task);
      });
    pumpFlights.set(task.id, run);
    return run;
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
          ? { kind: "snapshot", items: [...input.photos], pending: [] }
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
      failedItems: [],
      completedFiles: [],
      pausedItems: [],
      pausedProgress: [],
      activeFiles: [],
      activeItems: [],
      activeProgress: [],
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
      const transitionItems = uniquePhotosById(task.transitionItems ?? []);
      const transitionIds = new Set(transitionItems.map((item) => item.id));
      for (const item of transitionItems) {
        const transferId = findActiveTransfer(task.id, item.id)?.transferId;
        const progress = transferId ? task.activeProgress?.find((entry) => entry.transferId === transferId) : undefined;
        addPausedItem(task, item, progress?.downloadedBytes ?? 0, progress?.totalBytes ?? item.sizeBytes ?? null);
        removeQueuedItem(task, item.id);
      }
      for (const [transferId, transfer] of activeTransfers) {
        if (transfer.taskId !== task.id || transitionIds.has(transfer.item.id)) continue;
        const progress = task.activeProgress?.find((entry) => entry.transferId === transferId);
        addPausedItem(task, transfer.item, progress?.downloadedBytes ?? 0, progress?.totalBytes ?? transfer.item.sizeBytes ?? null);
        removeQueuedItem(task, transfer.item.id);
      }
      clearTransition(task);
      let pausedCount = uniquePhotosById(task.pausedItems ?? []).length;
      while (pausedCount < task.concurrency && task.source.pending.length > 0) {
        const item = task.source.pending.shift();
        if (!item || isItemPaused(task, item.id)) continue;
        addPausedItem(task, item);
        pausedCount += 1;
      }
      invalidateRun(task);
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
    if (!Array.isArray(task.source.pending)) task.source.pending = [];
    const resumedItems = uniquePhotosById(task.pausedItems ?? []);
    const resumedIds = new Set(resumedItems.map((item) => item.id));
    task.source.pending = task.source.pending.filter((item) => !resumedIds.has(item.id));
    if (resumedItems.length > 0) {
      task.source.pending.unshift(...resumedItems);
      task.transitionItems = resumedItems.map((item) => ({ ...item }));
      task.transitionOrder = resumedItems.map((item) => item.id);
    }
    task.pausedItems = [];
    task.status = "running";
    task.error = null;
    schedulePersist(task.accountKey, true);
    void pump(task);
  }

  function pauseItem(taskId: string, photoId: string): void {
    const task = tasks.value.find((item) => item.id === taskId);
    if (!task || isItemPaused(task, photoId)) return;
    const activeTransfer = findActiveTransfer(taskId, photoId);
    const activeItem = activeTransfer?.item;
    const activeProgress = activeTransfer ? task.activeProgress?.find((entry) => entry.transferId === activeTransfer.transferId) : undefined;
    if (!Array.isArray(task.source.pending)) task.source.pending = [];
    const pendingIndex = task.source.pending.findIndex((item) => item.id === photoId);
    const snapshotItem = task.source.kind === "snapshot" ? task.source.items.slice(task.nextIndex).find((candidate) => candidate.id === photoId) : null;
    const item = activeItem ?? (pendingIndex >= 0 ? task.source.pending[pendingIndex] : null) ?? snapshotItem ?? null;
    if (!item) return;
    removeQueuedItem(task, photoId);
    removeTransitionItem(task, photoId);
    addPausedItem(task, item, activeProgress?.downloadedBytes ?? 0, activeProgress?.totalBytes ?? item.sizeBytes ?? null);
    if (activeTransfer) cancelTransferForItem(taskId, photoId);
    schedulePersist(task.accountKey, true);
  }

  function resumeItem(taskId: string, photoId: string): void {
    const task = tasks.value.find((item) => item.id === taskId);
    if (!task || !isItemPaused(task, photoId)) return;
    if (!activeSessionId.value) {
      toast.error("This account session is unavailable. Sign in again to continue.");
      return;
    }
    const item = task.pausedItems?.find((paused) => paused.id === photoId);
    if (!item) return;
    removePausedItem(task, photoId);
    if (!Array.isArray(task.source.pending)) task.source.pending = [];
    if (!task.source.pending.some((pending) => pending.id === photoId)) task.source.pending.unshift(item);
    if (task.status !== "running") {
      task.status = "running";
      task.error = null;
    }
    schedulePersist(task.accountKey, true);
    void pump(task);
  }

  function retryFailed(id: string): void {
    const task = tasks.value.find((item) => item.id === id);
    if (!task || task.status === "running") return;
    if (!activeSessionId.value) {
      toast.error("This account session is unavailable. Sign in again to continue.");
      return;
    }
    const failedItems = Array.isArray(task.failedItems) ? task.failedItems : [];
    if (failedItems.length === 0) return;
    if (!Array.isArray(task.source.pending)) task.source.pending = [];
    for (const item of failedItems) {
      if (!task.source.pending.some((pending) => pending.id === item.id)) task.source.pending.unshift({ ...item });
    }
    task.failed = [];
    task.failedItems = [];
    task.error = null;
    task.status = "running";
    schedulePersist(task.accountKey, true);
    void pump(task);
  }

  function cancel(id: string): void {
    const task = tasks.value.find((item) => item.id === id);
    if (task) {
      invalidateRun(task);
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
      invalidateRun(task);
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
    pauseItem,
    resumeItem,
    retryFailed,
    cancel,
    remove,
    clearFinished,
  };
});
