import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { UNBOUNDED } from "@/lib/photos/dates";
import { photosList, savePhotoTo } from "@/lib/api";
import { useDownloadsStore } from "@/stores/downloads";
import type { PhotoItem } from "@/types";

let releasePage: (() => void) | null = null;
let holdPage = false;
const listSessions: string[] = [];

vi.mock("@/lib/common/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  photosList: vi.fn(async (payload: { sessionId: string }) => {
    listSessions.push(payload.sessionId);
    if (holdPage) {
      await new Promise<void>((resolve) => {
        releasePage = resolve;
      });
    }
    return {
      status: "OK",
      message: null,
      photos: [
        {
          id: "p1",
          fileName: "p1.jpeg",
          takenAt: 1,
          isVideo: false,
          thumbUrl: "https://example.com/p1.jpeg",
          originalUrl: "https://example.com/p1-original.jpeg",
        } satisfies PhotoItem,
      ],
      nextCursor: null,
    };
  }),
  savePhotoTo: vi.fn(async (_url: string, _name: string, targetPath: string) => targetPath),
  loadDownloadTasks: vi.fn(async () => ({ tasks: [], persistenceEnabled: true, writable: true, warning: null })),
  saveDownloadTasks: vi.fn(async () => undefined),
  rekeyDownloadTasks: vi.fn(async () => undefined),
  deleteDownloadTasks: vi.fn(async () => undefined),
  setDownloadSpeedLimit: vi.fn(async () => undefined),
  listenDownloadProgress: vi.fn(async () => () => undefined),
  cancelDownloadTransfer: vi.fn(async () => undefined),
}));

describe("下载任务切换账号后的会话归属", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    holdPage = false;
    releasePage = null;
    listSessions.length = 0;
    vi.clearAllMocks();
  });

  it("账号 A 的在飞任务切到账号 B 后仍用会话 A 完成", async () => {
    holdPage = true;
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    const task = store.create({
      accountKey: "dsid:a",
      range: { sessionId: "session-a", bounds: UNBOUNDED, total: 1 },
      title: "账号 A 任务",
      directory: "C:\\out",
      concurrency: 1,
    });
    if (!task) throw new Error("任务创建失败");

    await vi.waitFor(() => expect(listSessions).toEqual(["session-a"]));
    store.activateAccount("dsid:b", "session-b");
    expect(store.tasks).toHaveLength(0);

    releasePage?.();
    await vi.waitFor(() => expect(task.status).toBe("done"));
    expect(listSessions).toEqual(["session-a"]);
    expect(photosList).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "session-a" }));
    expect(savePhotoTo).toHaveBeenCalledTimes(1);
  });

  it("重新登录只替换目标账号任务的 sessionId", () => {
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "old-a");
    store.restoreAccountTasks("dsid:a", [
      {
        accountKey: "dsid:a",
        id: "task-a",
        title: "A",
        directory: "C:\\out",
        status: "paused",
        source: {
          kind: "range",
          sessionId: "old-a",
          bounds: UNBOUNDED,
          total: 1,
          startRank: null,
          nextRank: null,
          cursor: null,
          pending: [],
          exhausted: false,
          oldestTakenAt: null,
          seen: new Set(),
        },
        nextIndex: 0,
        done: 0,
        failed: [],
        usedNames: [],
        concurrency: 1,
        createdAt: 1,
        error: null,
      },
    ]);
    store.activateAccount("dsid:b", "session-b");
    store.applySession("dsid:a", "new-a");
    store.activateAccount("dsid:a", "new-a");

    const source = store.tasks[0]?.source;
    expect(source?.kind).toBe("range");
    expect(source?.kind === "range" ? source.sessionId : null).toBe("new-a");
  });
});
