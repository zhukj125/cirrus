import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { loadDownloadTasks, photosList, savePhotoTo } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { serializeDownloadTask } from "@/lib/downloads/persistence";
import { UNBOUNDED } from "@/lib/photos/dates";
import { useDownloadsStore } from "@/stores/downloads";
import type { DownloadTask } from "@/stores/downloads";

vi.mock("@/lib/common/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  loadDownloadTasks: vi.fn(),
  photosList: vi.fn(),
  saveDownloadTasks: vi.fn(async () => undefined),
  savePhotoTo: vi.fn(async (_url: string, _name: string, targetPath: string) => ({ path: targetPath, sizeBytes: 1024 })),
  setDownloadSpeedLimit: vi.fn(async () => undefined),
  listenDownloadProgress: vi.fn(async () => () => undefined),
  cancelDownloadTransfer: vi.fn(async () => undefined),
}));

function persisted(id: string, status: DownloadTask["status"], withPending: boolean): DownloadTask {
  return {
    accountKey: "dsid:a",
    id,
    title: id,
    directory: "C:\\out",
    status,
    source: {
      kind: "range",
      sessionId: "old-session",
      bounds: UNBOUNDED,
      total: 1,
      startRank: null,
      nextRank: null,
      cursor: null,
      pending: withPending
        ? [
            {
              id: `photo-${id}`,
              fileName: "photo.jpeg",
              takenAt: 1,
              isVideo: false,
              thumbUrl: "https://example.com/photo.jpeg",
              originalUrl: "https://example.com/photo-original.jpeg",
            },
          ]
        : [],
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
    error: null,
  };
}

describe("下载任务跨启动恢复", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("加载时把 running 转为 paused，保留 done/cancelled，且不自动联网", async () => {
    vi.mocked(loadDownloadTasks).mockResolvedValue({
      tasks: [persisted("running", "running", true), persisted("done", "done", false), persisted("cancelled", "cancelled", false)].map(serializeDownloadTask),
      persistenceEnabled: true,
      writable: true,
      warning: null,
    });
    const store = useDownloadsStore();

    await store.loadAccountTasks("dsid:a", "new-session");

    expect(store.tasks.map((task) => task.status)).toEqual(["paused", "done", "cancelled"]);
    expect(photosList).not.toHaveBeenCalled();
    expect(loadDownloadTasks).toHaveBeenCalledWith("dsid:a");
  });

  it("用户继续恢复任务时使用当前账号的新会话", async () => {
    vi.mocked(loadDownloadTasks).mockResolvedValue({
      tasks: [serializeDownloadTask(persisted("paused", "paused", true))],
      persistenceEnabled: true,
      writable: true,
      warning: null,
    });
    const store = useDownloadsStore();
    await store.loadAccountTasks("dsid:a", "new-session");
    const task = store.tasks[0];
    if (!task) throw new Error("任务未恢复");

    store.resume(task.id);
    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(task.source.kind === "range" ? task.source.sessionId : null).toBe("new-session");
    expect(savePhotoTo).toHaveBeenCalledTimes(1);
  });

  it("会话不可用时拒绝继续，重新登录后才恢复", async () => {
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", null);
    store.restoreAccountTasks("dsid:a", [
      {
        accountKey: "dsid:a",
        id: "paused",
        title: "paused",
        directory: "C:\\out",
        status: "paused",
        source: { kind: "snapshot", items: [], pending: [] },
        nextIndex: 0,
        done: 0,
        failed: [],
        failedItems: [],
        usedNames: [],
        concurrency: 1,
        createdAt: 1,
        error: null,
      },
    ]);

    store.resume("paused");
    expect(store.tasks[0]?.status).toBe("paused");
    expect(toast.error).toHaveBeenCalledWith("This account session is unavailable. Sign in again to continue.");

    store.applySession("dsid:a", "session-new");
    store.resume("paused");
    await vi.waitFor(() => expect(store.tasks[0]?.status).toBe("done"));
  });
  it("已加载账号不会重复读取持久化", async () => {
    vi.mocked(loadDownloadTasks).mockResolvedValue({ tasks: [], persistenceEnabled: true, writable: true, warning: null });
    const store = useDownloadsStore();
    await store.loadAccountTasks("dsid:a", "s1");
    await store.loadAccountTasks("dsid:a", "s1");
    expect(loadDownloadTasks).toHaveBeenCalledTimes(1);
  });
});
