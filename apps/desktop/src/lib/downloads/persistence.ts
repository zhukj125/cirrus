import type { DateBounds } from "@/lib/photos/dates";
import type { CompletedFile, DownloadSource, DownloadTask, TaskStatus } from "@/stores/downloads";
import type { PhotoItem } from "@/types";

const STATUSES: readonly TaskStatus[] = ["running", "paused", "done", "cancelled"];

export interface PersistedDownloadTask {
  accountKey: string;
  id: string;
  title: string;
  directory: string;
  status: TaskStatus;
  source: unknown;
  nextIndex: number;
  done: number;
  failed: string[];
  failedItems: PhotoItem[];
  completedFiles: CompletedFile[];
  pausedItems: PhotoItem[];
  pausedProgress: Array<{ photoId: string; downloadedBytes: number; totalBytes: number | null }>;
  usedNames: string[];
  concurrency: number;
  createdAt: number;
  error: string | null;
}

function optionalString(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function finiteNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function nonNegativeInteger(value: unknown, fallback: number): number {
  const number = finiteNumber(value);
  return number !== null && number >= 0 ? Math.floor(number) : fallback;
}

function stringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string" && item.trim().length > 0);
}

function parsePhoto(value: unknown): PhotoItem | null {
  if (value === null || typeof value !== "object") return null;
  const raw = value as Record<string, unknown>;
  const id = optionalString(raw.id);
  const thumbUrl = optionalString(raw.thumbUrl);
  if (!id || !thumbUrl) return null;
  return {
    id,
    fileName: optionalString(raw.fileName),
    takenAt: finiteNumber(raw.takenAt),
    isVideo: typeof raw.isVideo === "boolean" ? raw.isVideo : null,
    thumbUrl,
    displayUrl: optionalString(raw.displayUrl),
    originalUrl: optionalString(raw.originalUrl),
    videoUrl: optionalString(raw.videoUrl),
    isLivePhoto: typeof raw.isLivePhoto === "boolean" ? raw.isLivePhoto : null,
    width: finiteNumber(raw.width),
    height: finiteNumber(raw.height),
    sizeBytes: finiteNumber(raw.sizeBytes),
  };
}

function parseBounds(value: unknown): DateBounds | null {
  if (value === null || typeof value !== "object") return null;
  const raw = value as Record<string, unknown>;
  return {
    after: finiteNumber(raw.after),
    before: finiteNumber(raw.before),
  };
}

function parseCompletedFiles(value: unknown): CompletedFile[] {
  if (!Array.isArray(value)) return [];
  const files: CompletedFile[] = [];
  for (const entry of value) {
    if (entry === null || typeof entry !== "object") continue;
    const raw = entry as Record<string, unknown>;
    const name = optionalString(raw.name);
    if (!name) continue;
    files.push({ name, sizeBytes: finiteNumber(raw.sizeBytes) });
  }
  return files;
}

function parseSource(value: unknown): DownloadSource | null {
  if (value === null || typeof value !== "object") return null;
  const raw = value as Record<string, unknown>;
  if (raw.kind === "snapshot") {
    if (!Array.isArray(raw.items)) return null;
    const items = raw.items.map(parsePhoto).filter((item): item is PhotoItem => item !== null);
    const pendingRaw = Array.isArray(raw.pending) ? raw.pending : [];
    const pending = pendingRaw.map(parsePhoto).filter((item): item is PhotoItem => item !== null);
    return { kind: "snapshot", items, pending };
  }
  if (raw.kind !== "range") return null;

  const bounds = parseBounds(raw.bounds);
  if (!bounds) return null;
  const pendingRaw = Array.isArray(raw.pending) ? raw.pending : [];
  const pending = pendingRaw.map(parsePhoto).filter((item): item is PhotoItem => item !== null);
  return {
    kind: "range",
    sessionId: typeof raw.sessionId === "string" ? raw.sessionId : "",
    bounds,
    total: finiteNumber(raw.total),
    startRank: finiteNumber(raw.startRank),
    nextRank: finiteNumber(raw.nextRank),
    cursor: optionalString(raw.cursor),
    pending,
    exhausted: raw.exhausted === true,
    oldestTakenAt: finiteNumber(raw.oldestTakenAt),
    seen: new Set(stringArray(raw.seen)),
  };
}

export function serializeDownloadTask(task: DownloadTask): PersistedDownloadTask {
  const source =
    task.source.kind === "range"
      ? {
          ...task.source,
          pending: task.source.pending.map((item) => ({ ...item })),
          seen: [...task.source.seen],
        }
      : { kind: "snapshot" as const, items: task.source.items.map((item) => ({ ...item })), pending: task.source.pending.map((item) => ({ ...item })) };
  return {
    accountKey: task.accountKey,
    id: task.id,
    title: task.title,
    directory: task.directory,
    status: task.status,
    source,
    nextIndex: task.nextIndex,
    done: task.done,
    failed: [...task.failed],
    failedItems: task.failedItems.map((item) => ({ ...item })),
    completedFiles: (task.completedFiles ?? []).map((file) => ({ ...file })),
    pausedItems: (task.pausedItems ?? []).map((item) => ({ ...item })),
    pausedProgress: (task.pausedProgress ?? []).map((entry) => ({ ...entry })),
    usedNames: [...task.usedNames],
    concurrency: task.concurrency,
    createdAt: task.createdAt,
    error: task.error,
  };
}

export function serializeDownloadTasks(tasks: readonly DownloadTask[]): PersistedDownloadTask[] {
  return tasks.map(serializeDownloadTask);
}

export function parseDownloadTask(value: unknown, fallbackAccountKey: string): DownloadTask | null {
  if (value === null || typeof value !== "object") return null;
  const raw = value as Record<string, unknown>;
  const id = optionalString(raw.id);
  const title = optionalString(raw.title);
  const directory = optionalString(raw.directory);
  const source = parseSource(raw.source);
  if (!id || !title || !directory || !source) return null;
  const status = STATUSES.includes(raw.status as TaskStatus) ? (raw.status as TaskStatus) : "paused";
  return {
    accountKey: optionalString(raw.accountKey) ?? fallbackAccountKey,
    id,
    title,
    directory,
    status,
    source,
    nextIndex: nonNegativeInteger(raw.nextIndex, 0),
    done: nonNegativeInteger(raw.done, 0),
    failed: stringArray(raw.failed),
    failedItems: Array.isArray(raw.failedItems) ? raw.failedItems.map(parsePhoto).filter((item): item is PhotoItem => item !== null) : [],
    completedFiles: parseCompletedFiles(raw.completedFiles),
    pausedItems: Array.isArray(raw.pausedItems) ? raw.pausedItems.map(parsePhoto).filter((item): item is PhotoItem => item !== null) : [],
    pausedProgress: Array.isArray(raw.pausedProgress)
      ? raw.pausedProgress
          .map((entry) => {
            if (entry === null || typeof entry !== "object") return null;
            const progress = entry as Record<string, unknown>;
            const photoId = optionalString(progress.photoId);
            const downloadedBytes = finiteNumber(progress.downloadedBytes);
            if (!photoId || downloadedBytes === null) return null;
            return { photoId, downloadedBytes: Math.max(0, downloadedBytes), totalBytes: finiteNumber(progress.totalBytes) };
          })
          .filter((entry): entry is { photoId: string; downloadedBytes: number; totalBytes: number | null } => entry !== null)
      : [],
    usedNames: stringArray(raw.usedNames),
    concurrency: Math.max(1, Math.min(16, nonNegativeInteger(raw.concurrency, 4))),
    createdAt: finiteNumber(raw.createdAt) ?? Date.now(),
    error: optionalString(raw.error),
  };
}

export function parseDownloadTasks(values: unknown, fallbackAccountKey: string): DownloadTask[] {
  if (!Array.isArray(values)) return [];
  const seen = new Set<string>();
  const tasks: DownloadTask[] = [];
  for (const value of values) {
    const task = parseDownloadTask(value, fallbackAccountKey);
    if (!task || seen.has(task.id)) continue;
    seen.add(task.id);
    tasks.push(task);
  }
  return tasks;
}
