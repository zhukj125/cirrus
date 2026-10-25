import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { computed } from "vue";

import { fallbackMediaName, formatBytes, queueView, totalOf, useDownloadsStore } from "@/stores/downloads";
import { UNBOUNDED } from "@/lib/photos/dates";
import type { PhotoItem } from "@/types";

const saved: string[] = [];
const saveAttempts: string[] = [];
const failedSaveUrls = new Set<string>();
let inFlight = 0;
let maxInFlight = 0;

let gates: Array<() => void> = [];
const transferCancellations = new Map<string, () => void>();
let deferCancellation = false;
let deferredCancellations: Array<() => void> = [];
let hold = false;

let listPages: Record<string, { photos: PhotoItem[]; nextCursor: string | null }> = {};

let listCalls: Array<string | null> = [];

let rankPages: Record<number, { photos: PhotoItem[]; nextCursor: string | null }> = {};

let rankCalls: number[] = [];

function setLibrary(pages: Array<{ photos: PhotoItem[]; nextCursor: string | null }>): void {
  listPages = {};
  listCalls = [];
  let cursor: string | null = null;
  for (const page of pages) {
    listPages[cursor ?? ""] = page;
    cursor = page.nextCursor;
  }
}

function setRankLibrary(pages: Array<{ startRank: number; photos: PhotoItem[]; nextCursor?: string | null }>): void {
  rankPages = {};
  rankCalls = [];
  for (const page of pages) rankPages[page.startRank] = { photos: page.photos, nextCursor: page.nextCursor ?? null };
}

vi.mock("@/lib/common/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  photosList: vi.fn(async (payload: { cursor?: string | null; startRank?: number | null }) => {
    if (typeof payload.startRank === "number") {
      rankCalls.push(payload.startRank);
      const ranked = rankPages[payload.startRank];
      if (!ranked) return { status: "ERROR", message: `没有这个名次: ${payload.startRank}`, photos: [], nextCursor: null };
      return { status: "OK", message: null, photos: ranked.photos, nextCursor: ranked.nextCursor };
    }
    const cursor = payload.cursor ?? null;
    listCalls.push(cursor);
    const page = listPages[cursor ?? ""];
    if (!page) return { status: "ERROR", message: `没有这一页: ${cursor}`, photos: [], nextCursor: null };
    return { status: "OK", message: null, photos: page.photos, nextCursor: page.nextCursor };
  }),
  savePhotoTo: vi.fn(async (_url: string, name: string, targetPath: string, transferId?: string) => {
    saveAttempts.push(_url);
    inFlight += 1;
    maxInFlight = Math.max(maxInFlight, inFlight);
    try {
      if (hold) {
        await new Promise<void>((resolve, reject) => {
          let settled = false;
          const settle = (error?: Error) => {
            if (settled) return;
            settled = true;
            inFlight -= 1;
            if (transferId) transferCancellations.delete(transferId);
            if (error) reject(error);
            else resolve();
          };
          gates.push(() => settle());
          if (transferId) transferCancellations.set(transferId, () => settle(new Error("cancelled")));
        });
      } else {
        inFlight -= 1;
      }

      if (failedSaveUrls.has(_url) || name.startsWith("bad")) throw new Error("403 Forbidden");
      saved.push(targetPath);
      return { path: targetPath, sizeBytes: 1024 };
    } finally {
      if (!hold) inFlight = Math.max(0, inFlight);
    }
  }),
  loadDownloadTasks: vi.fn(async () => ({ tasks: [], persistenceEnabled: true, writable: true, warning: null })),
  saveDownloadTasks: vi.fn(async () => undefined),
  rekeyDownloadTasks: vi.fn(async () => undefined),
  deleteDownloadTasks: vi.fn(async () => undefined),
  setDownloadSpeedLimit: vi.fn(async () => undefined),
  listenDownloadProgress: vi.fn(async () => () => undefined),
  cancelDownloadTransfer: vi.fn(async (transferId: string) => {
    const cancel = transferCancellations.get(transferId);
    if (!cancel) return;
    if (deferCancellation) deferredCancellations.push(cancel);
    else cancel();
  }),
}));

function photo(id: string, fileName?: string, takenAt = 1_700_000_000_000): PhotoItem {
  return {
    id,
    fileName: fileName ?? `${id}.jpeg`,
    takenAt,
    isVideo: false,
    thumbUrl: `https://example.com/${id}.jpeg`,
    displayUrl: `https://example.com/${id}-display.jpeg`,
    originalUrl: `https://example.com/${id}-original.jpeg`,
    videoUrl: null,
    isLivePhoto: false,
    sizeBytes: 1024,
  };
}

function savedNames(): string[] {
  return saved.map((path) => path.slice(path.lastIndexOf("\\") + 1)).sort();
}

async function driveUntilDone(task: { status: string; done: number; failed: string[] }): Promise<void> {
  for (let guard = 0; guard < 1000 && task.status !== "done"; guard += 1) {
    const batch = gates;
    gates = [];
    transferCancellations.clear();
    batch.forEach((release) => release());
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

describe("下载任务", () => {
  beforeEach(() => {
    saved.length = 0;
    saveAttempts.length = 0;
    failedSaveUrls.clear();
    inFlight = 0;
    maxInFlight = 0;
    gates = [];
    transferCancellations.clear();
    deferCancellation = false;
    deferredCancellations = [];
    hold = false;
    setLibrary([]);
    setRankLibrary([]);
    setActivePinia(createPinia());
    useDownloadsStore().activateAccount("dsid:test", "s1");
  });

  it("照片页带入的日期区间只消费一次，不会污染下次打开", () => {
    const store = useDownloadsStore();
    const bounds = { after: 100, before: 200 };
    store.openDialog(bounds);
    expect(store.dialogOpen).toBe(true);
    expect(store.consumeRequestedBounds()).toEqual(bounds);
    expect(store.consumeRequestedBounds()).toBeNull();
  });

  it("按并发档位并行下载，全部完成后状态为 done", async () => {
    hold = true;
    const store = useDownloadsStore();
    const photos = Array.from({ length: 12 }, (_, index) => photo(`p${index}`));

    const task = store.create({ accountKey: "dsid:test", photos, title: "全部照片 · 12 张", directory: "C:\\out", concurrency: 4 });
    expect(task).not.toBeNull();

    await vi.waitFor(() => expect(maxInFlight).toBe(4));

    if (!task) throw new Error("任务创建失败");
    await driveUntilDone(task);
    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task?.done).toBe(12);
    expect(task?.failed).toHaveLength(0);
    expect(saved).toHaveLength(12);
  });

  it("worker 的进度写入是响应式的（界面上的进度条会动）", async () => {
    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", photos: [photo("a"), photo("b")], title: "全部照片 · 2 张", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    const progress = computed(() => store.tasks[0]?.done ?? -1);
    expect(progress.value).toBe(0);
    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(progress.value).toBe(2);
  });

  it("暂停后不再取新任务，继续时从断点接着下", async () => {
    hold = true;
    const store = useDownloadsStore();
    const photos = Array.from({ length: 10 }, (_, index) => photo(`p${index}`));
    const task = store.create({ accountKey: "dsid:test", photos, title: "全部照片 · 10 张", directory: "C:\\out", concurrency: 2 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(gates).toHaveLength(2));

    store.pause(task.id);

    gates.forEach((release) => release());
    gates = [];
    transferCancellations.clear();

    await vi.waitFor(() => {
      expect(inFlight).toBe(0);
      expect(task.pausedItems?.length ?? 0).toBe(2);
    });

    expect(task.done).toBe(0);
    expect(task.nextIndex).toBe(2);

    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(task.done).toBe(0);
    expect(gates).toHaveLength(0);

    store.resume(task.id);
    await vi.waitFor(() => expect(task.status).toBe("running").valueOf());
    await driveUntilDone(task);
    await vi.waitFor(() => expect(task.done).toBe(10));
    expect(task.status).toBe("done");
  });

  it("只剩暂停文件时任务进入 paused 状态", async () => {
    hold = true;
    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", photos: [photo("only")], title: "仅剩暂停", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.activeFiles).toHaveLength(1));
    store.pauseItem(task.id, "only");
    await vi.waitFor(() => expect(task.status).toBe("paused"));
    expect(task.pausedItems?.map((item) => item.id)).toEqual(["only"]);

    hold = false;
    store.resumeItem(task.id, "only");
    for (let guard = 0; guard < 100 && task.status !== "done"; guard += 1) {
      const batch = gates;
      gates = [];
      batch.forEach((release) => release());
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.done).toBe(1);
  });

  it("单个文件暂停会占住下载槽，不会继续拉新文件", async () => {
    hold = true;
    const store = useDownloadsStore();
    const photos = Array.from({ length: 10 }, (_, index) => photo(`slot-${index}`));
    const task = store.create({ accountKey: "dsid:test", photos, title: "暂停槽位", directory: "C:\\out", concurrency: 4 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.activeFiles).toHaveLength(4));
    const pausedIds: string[] = [];
    for (let index = 0; index < 4; index += 1) {
      const target = task.activeItems?.find((item) => !pausedIds.includes(item.id));
      if (!target) throw new Error("没有可暂停的活动文件");
      pausedIds.push(target.id);
      store.pauseItem(task.id, target.id);
      await vi.waitFor(() => {
        const queue = queueView(task, Number.MAX_SAFE_INTEGER);
        expect(queue.active.length).toBe(3 - index);
        expect(queue.paused.length).toBe(index + 1);
        expect(queue.active.length + queue.paused.length).toBe(4);
      });
    }
    expect(task.nextIndex).toBe(4);

    for (let index = 0; index < pausedIds.length; index += 1) {
      store.resumeItem(task.id, pausedIds[index] as string);
      await vi.waitFor(() => {
        const queue = queueView(task, Number.MAX_SAFE_INTEGER);
        expect(queue.active.length).toBe(index + 1);
        expect(queue.paused.length).toBe(3 - index);
        expect(queue.active.length + queue.paused.length).toBe(4);
      });
    }

    hold = false;
    for (let guard = 0; guard < 500 && task.status !== "done"; guard += 1) {
      const batch = gates;
      gates = [];
      batch.forEach((release) => release());
      await new Promise((resolve) => setTimeout(resolve, 0));
    }

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.done).toBe(10);
    expect(task.failed).toEqual([]);
  });
  it("两个任务写同一目标文件时会串行覆盖", async () => {
    hold = true;
    const store = useDownloadsStore();
    const first = store.create({ accountKey: "dsid:test", photos: [photo("a", "same.jpeg")], title: "任务一", directory: "C:\\out", concurrency: 1 });
    const second = store.create({ accountKey: "dsid:test", photos: [photo("b", "same.jpeg")], title: "任务二", directory: "C:\\out", concurrency: 1 });
    if (!first || !second) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(gates).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(gates).toHaveLength(1);

    gates[0]?.();
    await vi.waitFor(() => expect(gates).toHaveLength(2));
    hold = false;
    gates[1]?.();
    await vi.waitFor(() => expect(first.status).toBe("done"));
    await vi.waitFor(() => expect(second.status).toBe("done"));
    expect(saved).toEqual(["C:\\out\\same.jpeg", "C:\\out\\same.jpeg"]);
  });

  it("非法文件名和路径分隔符不会逃出下载目录", async () => {
    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", photos: [photo("safe", "../../evil:name?.jpeg")], title: "安全文件名", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(saved).toEqual(["C:\\out\\evil_name_.jpeg"]);
  });

  it("同名文件直接覆盖，完成列表只保留最后一次结果", async () => {
    const store = useDownloadsStore();
    const photos = [photo("a", "IMG_0001.HEIC"), photo("b", "IMG_0001.HEIC"), photo("c", "IMG_0001.HEIC")];

    const task = store.create({ accountKey: "dsid:test", photos, title: "全部照片 · 3 张", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");
    await vi.waitFor(() => expect(task.status).toBe("done"));

    expect(saved).toEqual(["C:\\out\\IMG_0001.HEIC", "C:\\out\\IMG_0001.HEIC", "C:\\out\\IMG_0001.HEIC"]);
    expect(task.completedFiles).toEqual([{ name: "IMG_0001.HEIC", sizeBytes: 1024 }]);
  });

  it("单张失败不中断整批，失败会被记下来", async () => {
    const store = useDownloadsStore();
    const photos = [photo("a"), photo("bad-1"), photo("c")];

    const task = store.create({ accountKey: "dsid:test", photos, title: "全部照片 · 3 张", directory: "C:\\out", concurrency: 1 });
    await vi.waitFor(() => expect(task?.status).toBe("done"));

    expect(task?.done).toBe(2);
    expect(task?.failed).toEqual(["bad-1.jpeg"]);
  });
});

describe("下载任务 · 区间来源", () => {
  beforeEach(() => {
    saved.length = 0;
    saveAttempts.length = 0;
    failedSaveUrls.clear();
    inFlight = 0;
    maxInFlight = 0;
    gates = [];
    transferCancellations.clear();
    deferCancellation = false;
    deferredCancellations = [];
    hold = false;
    setLibrary([]);
    setRankLibrary([]);
    setActivePinia(createPinia());
    useDownloadsStore().activateAccount("dsid:test", "s1");
  });

  it("边下边翻: 两页都下完，且每页只翻一次", async () => {
    setLibrary([
      { photos: [photo("a", undefined, 30), photo("b", undefined, 25), photo("c", undefined, 19), photo("d", undefined, 15)], nextCursor: "c1" },

      { photos: [photo("e", undefined, 12), photo("f", undefined, 5), photo("g", undefined, 1)], nextCursor: null },
    ]);

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", range: { sessionId: "s1", bounds: { after: 10, before: 20 }, total: 3 }, title: "区间 · 3 张", directory: "C:\\out", concurrency: 2 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(savedNames()).toEqual(["c.jpeg", "d.jpeg", "e.jpeg"]);
    expect(task.done).toBe(3);
    expect(task.failed).toHaveLength(0);
    expect(task.error).toBeNull();

    expect(listCalls).toEqual([null, "c1"]);
    expect(totalOf(task)).toBe(3);
  });

  it("比区间新的跳过，翻过下界就停（不会把整个库翻完）", async () => {
    setLibrary([
      { photos: [photo("n1", undefined, 40), photo("n2", undefined, 30)], nextCursor: "c1" },
      { photos: [photo("in1", undefined, 19), photo("in2", undefined, 15)], nextCursor: "c2" },
      { photos: [photo("old1", undefined, 8), photo("old2", undefined, 5)], nextCursor: "c3" },
      { photos: [photo("never", undefined, 1)], nextCursor: null },
    ]);

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", range: { sessionId: "s1", bounds: { after: 10, before: 20 }, total: null }, title: "区间", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(savedNames()).toEqual(["in1.jpeg", "in2.jpeg"]);

    expect(listCalls).toEqual([null, "c1", "c2"]);
    expect(task.error).toBeNull();

    expect(totalOf(task)).toBeNull();
  });

  it("边下边翻时的并发仍然达到设置的档位", async () => {
    hold = true;
    const photos = Array.from({ length: 12 }, (_, index) => photo(`p${index}`, undefined, 100 - index));
    setLibrary([{ photos, nextCursor: null }]);

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", range: { sessionId: "s1", bounds: UNBOUNDED, total: 12 }, title: "全部照片 · 12 张", directory: "C:\\out", concurrency: 4 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(maxInFlight).toBe(4));
    expect(listCalls).toEqual([null]);

    hold = false;
    for (let guard = 0; guard < 500 && task.status === "running"; guard += 1) {
      const batch = gates;
      gates = [];
      transferCancellations.clear();
      batch.forEach((release) => release());
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    }

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.done).toBe(12);
    expect(saved).toHaveLength(12);
  });

  it("区间来源暂停后不再取新照片，继续时接着下（pending 与游标都不丢）", async () => {
    hold = true;
    setLibrary([
      { photos: [photo("a", undefined, 19), photo("b", undefined, 18), photo("c", undefined, 17)], nextCursor: "c1" },
      { photos: [photo("d", undefined, 15)], nextCursor: null },
    ]);

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", range: { sessionId: "s1", bounds: { after: 10, before: 20 }, total: 4 }, title: "区间 · 4 张", directory: "C:\\out", concurrency: 2 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(gates).toHaveLength(2));
    store.pause(task.id);
    gates.forEach((release) => release());
    gates = [];
    transferCancellations.clear();
    await vi.waitFor(() => expect(inFlight).toBe(0));

    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(task.done).toBe(0);
    expect(gates).toHaveLength(0);
    expect(listCalls).toEqual([null, "c1"]);
    const pausedQueue = queueView(task, Number.MAX_SAFE_INTEGER);
    expect(pausedQueue.paused.length + pausedQueue.pendingTotal + (pausedQueue.unfetched ?? 0)).toBe(4);

    hold = false;
    store.resume(task.id);
    for (let guard = 0; guard < 200 && task.done + task.failed.length < 4; guard += 1) {
      const batch = gates;
      gates = [];
      transferCancellations.clear();
      batch.forEach((release) => release());
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    }

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.done).toBe(4);
    expect(savedNames()).toEqual(["a.jpeg", "b.jpeg", "c.jpeg", "d.jpeg"]);
  });

  it("区间接口报错时收工并记下错误，不会对着后端死循环", async () => {
    setLibrary([]);

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", range: { sessionId: "s1", bounds: { after: 10, before: 20 }, total: null }, title: "区间", directory: "C:\\out", concurrency: 2 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.error).not.toBeNull();
    expect(listCalls).toEqual([null]);

    setLibrary([{ photos: [photo("recovered", undefined, 15)], nextCursor: null }]);
    store.resume(task.id);
    await vi.waitFor(() => expect(task.done).toBe(1));
    expect(task.error).toBeNull();
  });

  it("落点名次: 从最老端起跳、遇到上界就停，不碰游标翻页", async () => {
    setRankLibrary([{ startRank: 100, photos: [photo("a", undefined, 19), photo("b", undefined, 18), photo("newer", undefined, 21)] }]);

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", range: { sessionId: "s1", bounds: { after: 10, before: 20 }, total: 2, startRank: 100 }, title: "区间 · 约 2 张", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(rankCalls).toEqual([100]);
    expect(listCalls).toEqual([]);
    expect(savedNames()).toEqual(["a.jpeg", "b.jpeg"]);
    expect(task.done).toBe(2);
    expect(totalOf(task)).toBe(2);
  });

  it("名次取页失败时退回游标翻页，任务照常下完", async () => {
    setLibrary([{ photos: [photo("c", undefined, 19)], nextCursor: null }]);

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", range: { sessionId: "s1", bounds: { after: 10, before: 20 }, total: 1, startRank: 5 }, title: "区间 · 约 1 张", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(rankCalls).toEqual([5]);
    expect(listCalls).toEqual([null]);
    expect(savedNames()).toEqual(["c.jpeg"]);
    expect(task.error).toBeNull();

    expect(task.source.kind === "range" ? task.source.startRank : "not-range").toBeNull();
  });
  it("名次页为空时自动退回游标扫描，不会生成空队列", async () => {
    setRankLibrary([{ startRank: 5, photos: [] }]);
    setLibrary([{ photos: [photo("c", undefined, 19)], nextCursor: null }]);

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", range: { sessionId: "s1", bounds: { after: 10, before: 20 }, total: 1, startRank: 5 }, title: "区间 · 1 张", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(rankCalls).toEqual([5]);
    expect(listCalls).toEqual([null]);
    expect(task.done).toBe(1);
    expect(task.failed).toEqual([]);
    expect(savedNames()).toEqual(["c.jpeg"]);
  });

  it("原始下载链接失败时自动尝试备用链接", async () => {
    const item = photo("fallback");
    failedSaveUrls.add(item.originalUrl ?? "");

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", photos: [item], title: "备用链接", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(saveAttempts).toEqual([item.originalUrl, item.displayUrl]);
    expect(task.done).toBe(1);
    expect(task.failed).toEqual([]);
    expect(savedNames()).toEqual(["fallback.jpeg"]);
  });
  it("区间拉取失败后继续会重新打开数据源", async () => {
    setLibrary([{ photos: [photo("recovered", undefined, 5)], nextCursor: null }]);
    const store = useDownloadsStore();
    store.restoreAccountTasks("dsid:test", [
      {
        accountKey: "dsid:test",
        id: "range-retry",
        title: "区间重试",
        directory: "C:\\out",
        status: "paused",
        source: {
          kind: "range",
          sessionId: "s1",
          bounds: { after: 0, before: 10 },
          total: 1,
          startRank: null,
          nextRank: null,
          cursor: null,
          pending: [],
          exhausted: true,
          oldestTakenAt: null,
          seen: new Set(),
        },
        nextIndex: 0,
        done: 0,
        failed: [],
        failedItems: [],
        usedNames: [],
        concurrency: 1,
        createdAt: 1,
        error: "Temporary list failure",
      },
    ]);

    store.resume("range-retry");
    await vi.waitFor(() => expect(store.tasks[0]?.status).toBe("done"));
    expect(store.tasks[0]?.done).toBe(1);
  });

  it("区间队列会提前预取下一页，下载时不会短暂清空", async () => {
    hold = true;
    setLibrary([
      { photos: [photo("a", undefined, 19)], nextCursor: "c1" },
      { photos: [photo("b", undefined, 18)], nextCursor: null },
    ]);

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", range: { sessionId: "s1", bounds: { after: 10, before: 20 }, total: 2 }, title: "预取 · 2 张", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => {
      expect(gates).toHaveLength(1);
      expect(listCalls).toEqual([null, "c1"]);
    });

    hold = false;
    gates.forEach((release) => release());
    gates = [];
    transferCancellations.clear();
    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.done).toBe(2);
    expect(savedNames()).toEqual(["a.jpeg", "b.jpeg"]);
  });
  it("快速继续时过渡帧保持四个下载项和原顺序", async () => {
    hold = true;
    const store = useDownloadsStore();
    const photos = Array.from({ length: 8 }, (_, index) => photo(`stable-${index}`));
    const task = store.create({ accountKey: "dsid:test", photos, title: "稳定过渡", directory: "C:\\out", concurrency: 4 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.activeFiles).toHaveLength(4));
    const originalOrder = [...(task.activeFiles ?? [])];
    deferCancellation = true;
    store.pause(task.id);
    expect(queueView(task).paused.map((item) => item.name)).toEqual(originalOrder);

    store.resume(task.id);
    expect(queueView(task).active.map((item) => item.name)).toEqual(originalOrder);

    deferredCancellations.splice(0, 2).forEach((cancel) => cancel());
    await vi.waitFor(() => expect(task.activeFiles).toHaveLength(2));
    expect(queueView(task).active.map((item) => item.name)).toEqual(originalOrder);

    deferredCancellations.splice(0).forEach((cancel) => cancel());
    await vi.waitFor(() => expect(task.activeFiles).toHaveLength(4));
    expect(queueView(task).active.map((item) => item.name)).toEqual(originalOrder);

    hold = false;
    for (let guard = 0; guard < 500 && task.status !== "done"; guard += 1) {
      const batch = gates;
      gates = [];
      batch.forEach((release) => release());
      await new Promise((resolve) => setTimeout(resolve, 0));
    }

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.done).toBe(8);
    expect(task.failed).toEqual([]);
  });
  it("快速暂停和恢复不会把取消中的下载记为失败", async () => {
    hold = true;
    setLibrary([{ photos: [photo("a", undefined, 19), photo("b", undefined, 18), photo("c", undefined, 17), photo("d", undefined, 16)], nextCursor: null }]);

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", range: { sessionId: "s1", bounds: { after: 10, before: 20 }, total: 4 }, title: "快速切换", directory: "C:\\out", concurrency: 4 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(gates.length).toBeGreaterThan(0));
    for (let iteration = 0; iteration < 12; iteration += 1) {
      store.pause(task.id);
      store.resume(task.id);
      await Promise.resolve();
    }

    store.pause(task.id);
    await vi.waitFor(() => {
      const queue = queueView(task, Number.MAX_SAFE_INTEGER);
      expect(queue.active).toHaveLength(0);
      expect(queue.paused.length).toBe(4);
      expect(queue.paused.length + queue.pendingTotal).toBe(queue.remaining);
    });
    const queue = queueView(task, Number.MAX_SAFE_INTEGER);
    const remainingIds = [...queue.paused.map((item) => item.photoId), ...queue.pending.map((item) => item.photoId)];
    expect(new Set(remainingIds).size).toBe(remainingIds.length);

    store.resume(task.id);
    await vi.waitFor(() => expect(task.activeFiles).toHaveLength(4));
    expect(gates.length).toBeGreaterThanOrEqual(4);

    hold = false;
    for (let guard = 0; guard < 500 && task.status !== "done"; guard += 1) {
      const batch = gates;
      gates = [];
      batch.forEach((release) => release());
      await new Promise((resolve) => setTimeout(resolve, 0));
    }

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.failed).toEqual([]);
    expect(task.done).toBe(4);
    expect(inFlight).toBe(0);
  });
  it("暂停时已完成的项目不会因为其它项取消而重复下载", async () => {
    hold = true;
    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", photos: [photo("a"), photo("b")], title: "已完成不回退", directory: "C:\\out", concurrency: 2 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(gates).toHaveLength(2));
    gates[1]?.();
    await vi.waitFor(() => expect(task.done).toBe(1));

    store.pause(task.id);
    await vi.waitFor(() => {
      expect(inFlight).toBe(0);
      expect(task.pausedItems?.length ?? 0).toBe(1);
    });
    expect(task.nextIndex).toBe(2);
    expect(task.pausedItems?.map((item) => item.id)).toHaveLength(1);

    hold = false;
    store.resume(task.id);
    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.done).toBe(2);
    expect(task.failed).toEqual([]);
    expect(saved).toHaveLength(2);
    expect(new Set(savedNames()).size).toBe(2);
  });
  it("只对真实失败的文件执行重试", async () => {
    const item = photo("retry-me");
    failedSaveUrls.add(item.originalUrl ?? "");
    failedSaveUrls.add(item.displayUrl ?? "");
    failedSaveUrls.add(item.thumbUrl);

    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", photos: [item], title: "真实失败", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.done).toBe(0);
    expect(task.failed).toEqual(["retry-me.jpeg"]);
    expect(task.failedItems.map((failed) => failed.id)).toEqual(["retry-me"]);
    expect(saveAttempts).not.toContain(item.thumbUrl);

    failedSaveUrls.clear();
    store.retryFailed(task.id);
    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.done).toBe(1);
    expect(task.failed).toEqual([]);
    expect(task.failedItems).toEqual([]);
    expect(task.completedFiles).toEqual([{ name: "retry-me.jpeg", sizeBytes: 1024 }]);
  });

  it("文件大小按可读单位显示", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1 KB");
    expect(formatBytes(1024 * 1024)).toBe("1.0 MB");
  });
  it("没有真实文件名时使用时间命名，不显示 UUID", () => {
    const item = { ...photo("UUID-1234"), fileName: null, takenAt: 1_711_339_140_000 };
    expect(fallbackMediaName(item, 1)).toBe("20240325_115900_0001.jpeg");
  });
  it("快速暂停和继续单个文件不会减少并发 worker", async () => {
    hold = true;
    const store = useDownloadsStore();
    const photos = Array.from({ length: 8 }, (_, index) => photo(`item-${index}`));
    const task = store.create({ accountKey: "dsid:test", photos, title: "单项快速切换", directory: "C:\\out", concurrency: 4 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(task.activeFiles).toHaveLength(4));
    for (let iteration = 0; iteration < 6; iteration += 1) {
      const target = task.activeItems?.[0];
      if (!target) throw new Error("没有活动文件");
      store.pauseItem(task.id, target.id);
      await Promise.resolve();
      store.resumeItem(task.id, target.id);
      await vi.waitFor(() => {
        expect(task.activeFiles).toHaveLength(4);
        expect(task.pausedItems).toEqual([]);
      });
    }
    expect(task.failed).toEqual([]);

    hold = false;
    for (let guard = 0; guard < 500 && task.status !== "done"; guard += 1) {
      const batch = gates;
      gates = [];
      batch.forEach((release) => release());
      await new Promise((resolve) => setTimeout(resolve, 0));
    }

    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.done).toBe(8);
    expect(inFlight).toBe(0);
    expect(new Set(savedNames()).size).toBe(8);
  });
  it("可以单独暂停和恢复某个文件", async () => {
    hold = true;
    const store = useDownloadsStore();
    const task = store.create({ accountKey: "dsid:test", photos: [photo("a"), photo("b")], title: "单项暂停", directory: "C:\\out", concurrency: 1 });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(gates).toHaveLength(1));
    store.pauseItem(task.id, "a");
    await vi.waitFor(() => {
      expect(task.pausedItems?.map((item) => item.id)).toEqual(["a"]);
      expect(task.activeFiles).not.toContain("a.jpeg");
    });
    expect(task.failed).toEqual([]);

    hold = false;
    const batch = gates;
    gates = [];
    batch.forEach((release) => release());
    store.resumeItem(task.id, "a");
    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.done).toBe(2);
    expect(task.pausedItems).toEqual([]);
    expect(savedNames()).toEqual(["a.jpeg", "b.jpeg"]);
  });
});
