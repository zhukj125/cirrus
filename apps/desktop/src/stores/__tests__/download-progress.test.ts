import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { cancelDownloadTransfer, savePhotoTo, setDownloadSpeedLimit } from "@/lib/api";
import { readStored } from "@/lib/common/storage";
import { StorageKey } from "@/constants";
import { formatSpeed, queueView, useDownloadsStore } from "@/stores/downloads";
import type { PhotoItem } from "@/types";

let releases: Array<() => void> = [];
let transferId = "";
let transferIds: string[] = [];

vi.mock("@/lib/common/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  loadDownloadTasks: vi.fn(async () => ({ tasks: [], persistenceEnabled: true, writable: true, warning: null })),
  photosList: vi.fn(),
  saveDownloadTasks: vi.fn(async () => undefined),
  savePhotoTo: vi.fn(async (_url: string, _name: string, targetPath: string, id?: string) => {
    transferId = id ?? "";
    if (transferId) transferIds.push(transferId);
    await new Promise<void>((resolve) => {
      releases.push(resolve);
    });
    return { path: targetPath, sizeBytes: 1024 };
  }),
  rekeyDownloadTasks: vi.fn(async () => undefined),
  deleteDownloadTasks: vi.fn(async () => undefined),
  setDownloadSpeedLimit: vi.fn(async () => undefined),
  listenDownloadProgress: vi.fn(async () => () => undefined),
  cancelDownloadTransfer: vi.fn(async () => undefined),
}));

function photo(): PhotoItem {
  return {
    id: "p1",
    fileName: "IMG_0001.HEIC",
    takenAt: 1,
    isVideo: false,
    thumbUrl: "https://example.com/p1.jpeg",
    originalUrl: "https://example.com/p1-original.heic",
    sizeBytes: 1024,
  };
}

describe("下载文件与速度进度", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
    releases = [];
    transferId = "";
    transferIds = [];
    vi.clearAllMocks();
  });

  afterEach(() => {
    releases.forEach((release) => release());
    vi.useRealTimers();
  });

  it("运行中显示当前文件，完成后保留最近文件", async () => {
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    const task = store.create({ accountKey: "dsid:a", title: "一张", directory: "C:\\out", photos: [photo()], concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(transferId).not.toBe(""));
    expect(task.activeFiles).toEqual(["IMG_0001.HEIC"]);

    releases.forEach((release) => release());
    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.activeFiles).toEqual([]);
    expect(task.lastFile).toBe("IMG_0001.HEIC");
  });

  it("并发下载多个文件时显示活动文件数量", async () => {
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    const second = { ...photo(), id: "p2", fileName: "IMG_0002.HEIC" };
    const task = store.create({ accountKey: "dsid:a", title: "两张", directory: "C:\\out", photos: [photo(), second], concurrency: 2 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(releases).toHaveLength(2));
    expect(task.activeFiles).toHaveLength(2);
    expect(task.activeFiles).toEqual(expect.arrayContaining(["IMG_0001.HEIC", "IMG_0002.HEIC"]));

    releases.forEach((release) => release());
    await vi.waitFor(() => expect(task.status).toBe("done"));
  });

  it("任务详情队列包含在飞文件与后续文件", async () => {
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    const items = [photo(), { ...photo(), id: "p2", fileName: "IMG_0002.HEIC" }, { ...photo(), id: "p3", fileName: "IMG_0003.HEIC" }];
    const task = store.create({ accountKey: "dsid:a", title: "三张", directory: "C:\\out", photos: items, concurrency: 1 });
    if (!task) throw new Error("任务创建失败");
    await vi.waitFor(() => expect(releases).toHaveLength(1));

    const view = queueView(task);
    expect(view.active.map((item) => item.name)).toEqual(["IMG_0001.HEIC"]);
    expect(view.pending.map((item) => item.name)).toEqual(["IMG_0002.HEIC", "IMG_0003.HEIC"]);
    expect(view.pendingTotal).toBe(2);
    expect(view.pending.map((item) => item.sizeBytes)).toEqual([1024, 1024]);
    expect(view.remaining).toBe(3);
  });
  it("暂停会取消在飞传输并把快照文件放回断点", async () => {
    let rejectTransfer: ((error: Error) => void) | null = null;
    vi.mocked(savePhotoTo).mockImplementationOnce(
      (_url: string, _name: string, _targetPath: string, id?: string) =>
        new Promise<{ path: string; sizeBytes: number }>((_resolve, reject) => {
          transferId = id ?? "";
          rejectTransfer = reject;
        }),
    );
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    const task = store.create({ accountKey: "dsid:a", title: "一张", directory: "C:\\out", photos: [photo()], concurrency: 1 });
    if (!task) throw new Error("任务创建失败");
    await vi.waitFor(() => expect(transferId).not.toBe(""));
    store.ingestDownloadProgress({ transferId, fileName: "IMG_0001.HEIC", downloadedBytes: 512 * 1024, totalBytes: 4 * 1024 * 1024 });
    task.speedBytesPerSec = 12 * 1024 * 1024;
    store.downloadSpeed = 12 * 1024 * 1024;

    store.pause(task.id);
    store.ingestDownloadProgress({ transferId, fileName: "IMG_0001.HEIC", downloadedBytes: 700 * 1024, totalBytes: 4 * 1024 * 1024 });
    expect(task.status).toBe("paused");
    expect(task.speedBytesPerSec).toBe(0);
    expect(store.downloadSpeed).toBe(0);
    expect(cancelDownloadTransfer).toHaveBeenCalledWith(transferId);

    rejectTransfer?.(new Error("下载已暂停"));
    await vi.waitFor(() => expect(task.activeFiles).toEqual([]));
    expect(task.status).toBe("paused");
    expect(task.nextIndex).toBe(1);
    expect(task.pausedItems?.map((item) => item.id)).toEqual(["p1"]);
    expect(task.pausedProgress?.[0]).toMatchObject({ photoId: "p1", downloadedBytes: 700 * 1024, totalBytes: 4 * 1024 * 1024 });
    expect(task.failed).toEqual([]);
  });
  it("进度事件更新任务速度与全局速度，空闲后归零", async () => {
    vi.useFakeTimers();
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    const task = store.create({ accountKey: "dsid:a", title: "一张", directory: "C:\\out", photos: [photo()], concurrency: 1 });
    if (!task) throw new Error("任务创建失败");
    await vi.waitFor(() => expect(transferId).not.toBe(""));

    store.ingestDownloadProgress({ transferId, fileName: "IMG_0001.HEIC", downloadedBytes: 2 * 1024 * 1024, totalBytes: 4 * 1024 * 1024 });
    expect(task.activeProgress?.[0]).toMatchObject({ downloadedBytes: 2 * 1024 * 1024, totalBytes: 4 * 1024 * 1024 });
    await vi.advanceTimersByTimeAsync(500);
    expect(task.speedBytesPerSec).toBeGreaterThan(0);
    expect(store.downloadSpeed).toBeGreaterThan(0);

    releases.forEach((release) => release());
    await vi.waitFor(() => expect(task.status).toBe("done"));
    await vi.advanceTimersByTimeAsync(5000);
    expect(task.speedBytesPerSec).toBe(0);
    expect(store.downloadSpeed).toBe(0);
  });

  it("同一任务多个传输时，任务速度等于全局合计速度", async () => {
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    const second = { ...photo(), id: "p2", fileName: "IMG_0002.HEIC" };
    const task = store.create({ accountKey: "dsid:a", title: "两张", directory: "C:\\out", photos: [photo(), second], concurrency: 2 });
    if (!task) throw new Error("任务创建失败");
    await vi.waitFor(() => expect(transferIds).toHaveLength(2));

    store.ingestDownloadProgress({ transferId: transferIds[0] as string, fileName: "IMG_0001.HEIC", downloadedBytes: 1024 * 1024, totalBytes: 4 * 1024 * 1024 });
    store.ingestDownloadProgress({ transferId: transferIds[1] as string, fileName: "IMG_0002.HEIC", downloadedBytes: 2 * 1024 * 1024, totalBytes: 4 * 1024 * 1024 });

    expect(task.speedBytesPerSec).toBeGreaterThan(0);
    expect(task.speedBytesPerSec).toBe(store.downloadSpeed);
  });

  it("默认不限速，并支持设置任意数值后持久化", () => {
    const store = useDownloadsStore();
    expect(store.speedLimitBytesPerSec).toBe(0);
    const limit = Math.round(8.5 * 1024 * 1024);
    store.setSpeedLimit(limit);
    expect(store.speedLimitBytesPerSec).toBe(limit);
    expect(readStored(StorageKey.downloadSpeedLimit)).toBe(String(limit));
    expect(setDownloadSpeedLimit).toHaveBeenCalledWith(limit);
  });

  it("同时任务数默认持久化并限制在有效范围", () => {
    const store = useDownloadsStore();
    expect(store.maxConcurrentTasks).toBe(5);
    store.setMaxConcurrentTasks(3);
    expect(store.maxConcurrentTasks).toBe(3);
    expect(readStored(StorageKey.downloadTaskConcurrency)).toBe("3");
    store.setMaxConcurrentTasks(99);
    expect(store.maxConcurrentTasks).toBe(10);
  });

  it("每个任务的照片并发默认是 8", () => {
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    const task = store.create({ accountKey: "dsid:a", title: "默认并发", directory: "C:\\out", photos: [photo()] });
    expect(task?.concurrency).toBe(8);
  });

  it("速度格式化使用桌面下载工具习惯", () => {
    expect(formatSpeed(0)).toBe("0 B/s");
    expect(formatSpeed(1024)).toBe("1 KB/s");
    expect(formatSpeed(5 * 1024 * 1024)).toBe("5.0 MB/s");
  });
});
