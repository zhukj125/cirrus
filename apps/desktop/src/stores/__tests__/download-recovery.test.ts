import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

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
  savePhotoTo: vi.fn(),
  rekeyDownloadTasks: vi.fn(),
  deleteDownloadTasks: vi.fn(),
  setDownloadSpeedLimit: vi.fn(async () => undefined),
  listenDownloadProgress: vi.fn(async () => () => undefined),
  cancelDownloadTransfer: vi.fn(async () => undefined),
}));

function cancelledTask(): DownloadTask {
  return {
    accountKey: "dsid:a",
    id: "cancelled",
    title: "已取消任务",
    directory: "C:\\out",
    status: "cancelled",
    source: { kind: "snapshot", items: [], pending: [] },
    nextIndex: 0,
    done: 101,
    failed: [],
    failedItems: [],
    usedNames: [],
    concurrency: 4,
    createdAt: 1,
    error: null,
  };
}

describe("取消后的下载任务", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("可以从已取消状态恢复并继续", async () => {
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    store.restoreAccountTasks("dsid:a", [cancelledTask()]);

    store.resume("cancelled");

    await vi.waitFor(() => expect(store.tasks[0]?.status).toBe("done"));
    expect(store.tasks[0]?.error).toBeNull();
  });
});
