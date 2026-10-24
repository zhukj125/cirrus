import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { computed } from "vue";

import { totalOf, useDownloadsStore } from "@/stores/downloads";
import { UNBOUNDED } from "@/lib/photos/dates";
import type { PhotoItem } from "@/types";

const saved: string[] = [];
let inFlight = 0;
let maxInFlight = 0;

let gates: Array<() => void> = [];
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
  savePhotoTo: vi.fn(async (_url: string, name: string, targetPath: string) => {
    saved.push(targetPath);
    inFlight += 1;
    maxInFlight = Math.max(maxInFlight, inFlight);
    try {
      if (hold) {
        await new Promise<void>((resolve) => {
          gates.push(() => {
            inFlight -= 1;
            resolve();
          });
        });
      } else {
        inFlight -= 1;
      }

      if (name.startsWith("bad")) throw new Error("403 Forbidden");
      return targetPath;
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
  cancelDownloadTransfer: vi.fn(async () => undefined),
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
  };
}

function savedNames(): string[] {
  return saved.map((path) => path.slice(path.lastIndexOf("\\") + 1)).sort();
}

describe("下载任务", () => {
  beforeEach(() => {
    saved.length = 0;
    inFlight = 0;
    maxInFlight = 0;
    gates = [];
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

    await Promise.resolve();
    expect(maxInFlight).toBe(4);

    while (gates.length > 0) {
      const batch = gates;
      gates = [];
      batch.forEach((release) => release());
      await Promise.resolve();
      await Promise.resolve();
    }

    await vi.waitFor(() => expect(task?.status).toBe("done"));
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

    await Promise.resolve();
    expect(gates).toHaveLength(2);

    store.pause(task.id);

    gates.forEach((release) => release());
    gates = [];

    await vi.waitFor(() => expect(task.done).toBe(2));

    const doneWhilePaused = task.done;
    expect(doneWhilePaused).toBe(2);
    expect(task.nextIndex).toBe(2);

    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(task.done).toBe(2);
    expect(gates).toHaveLength(0);

    store.resume(task.id);
    await vi.waitFor(() => expect(task.status).toBe("running").valueOf());
    while (task.status === "running") {
      const batch = gates;
      gates = [];
      batch.forEach((release) => release());
      await Promise.resolve();
      await Promise.resolve();
      if (gates.length === 0 && inFlight === 0 && task.done + task.failed.length < (totalOf(task) ?? 0)) break;
    }
    await vi.waitFor(() => expect(task.done).toBe(10));
    expect(task.status).toBe("done");
  });

  it("同名文件自动补 -2，不互相覆盖", async () => {
    const store = useDownloadsStore();
    const photos = [photo("a", "IMG_0001.HEIC"), photo("b", "IMG_0001.HEIC"), photo("c", "IMG_0001.HEIC")];

    const task = store.create({ accountKey: "dsid:test", photos, title: "全部照片 · 3 张", directory: "C:\\out", concurrency: 1 });
    await vi.waitFor(() => expect(task?.status).toBe("done"));

    expect(saved).toEqual(["C:\\out\\IMG_0001.HEIC", "C:\\out\\IMG_0001-2.HEIC", "C:\\out\\IMG_0001-3.HEIC"]);
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
    inFlight = 0;
    maxInFlight = 0;
    gates = [];
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
    await vi.waitFor(() => expect(task.done).toBe(2));

    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(task.done).toBe(2);
    expect(gates).toHaveLength(0);
    expect(listCalls).toEqual([null]);

    hold = false;
    store.resume(task.id);
    for (let guard = 0; guard < 200 && task.done + task.failed.length < 4; guard += 1) {
      const batch = gates;
      gates = [];
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
});
