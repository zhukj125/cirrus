import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { saveDownloadTasks } from "@/lib/api";
import { toast } from "@/lib/common/toast";
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
  setDownloadSpeedLimit: vi.fn(async () => undefined),
  listenDownloadProgress: vi.fn(async () => () => undefined),
  cancelDownloadTransfer: vi.fn(async () => undefined),
}));

function task(status: DownloadTask["status"]): DownloadTask {
  return {
    accountKey: "dsid:a",
    id: "task-a",
    title: "A",
    directory: "C:\\out",
    status,
    source: { kind: "snapshot", items: [], pending: [] },
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

describe("下载任务保存调度", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    setActivePinia(createPinia());
    vi.clearAllMocks();
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    store.restoreAccountTasks("dsid:a", [task("running")]);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("短时间多次进度变化合并成一次写入", async () => {
    const store = useDownloadsStore();
    store.schedulePersist("dsid:a");
    store.schedulePersist("dsid:a");
    store.schedulePersist("dsid:a");
    await vi.advanceTimersByTimeAsync(399);
    expect(saveDownloadTasks).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(saveDownloadTasks).toHaveBeenCalledTimes(1);
  });

  it("写入进行中产生的新变化会在完成后补写", async () => {
    let release: (() => void) | null = null;
    vi.mocked(saveDownloadTasks).mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    const store = useDownloadsStore();
    store.schedulePersist("dsid:a");
    await vi.advanceTimersByTimeAsync(400);
    expect(saveDownloadTasks).toHaveBeenCalledTimes(1);

    store.schedulePersist("dsid:a");
    await vi.advanceTimersByTimeAsync(400);
    expect(saveDownloadTasks).toHaveBeenCalledTimes(1);

    release?.();
    await store.flushPersistence();
    expect(saveDownloadTasks).toHaveBeenCalledTimes(2);
  });

  it("暂停状态变更会立即写入", async () => {
    const store = useDownloadsStore();
    store.pause("task-a");
    await vi.waitFor(() => expect(saveDownloadTasks).toHaveBeenCalledTimes(1));
    expect(saveDownloadTasks).toHaveBeenCalledWith("dsid:a", expect.arrayContaining([expect.objectContaining({ status: "paused" })]));
  });

  it("关闭前 flush 会绕过防抖并写完全部 dirty 账号", async () => {
    const store = useDownloadsStore();
    store.schedulePersist("dsid:a");
    await store.flushPersistence();
    expect(saveDownloadTasks).toHaveBeenCalledTimes(1);
  });

  it("写入失败不阻断任务且只提示一次", async () => {
    vi.mocked(saveDownloadTasks).mockRejectedValue(new Error("disk full"));
    const store = useDownloadsStore();
    store.schedulePersist("dsid:a");
    await vi.advanceTimersByTimeAsync(400);
    await store.flushPersistence();
    store.schedulePersist("dsid:a");
    await vi.advanceTimersByTimeAsync(400);
    await store.flushPersistence();

    expect(store.tasks).toHaveLength(1);
    expect(store.persistenceWarning).toContain("disk full");
    expect(toast.error).toHaveBeenCalledTimes(1);
  });
});
