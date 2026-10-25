import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { toast } from "@/lib/common/toast";
import { useDownloadsStore } from "@/stores/downloads";
import type { DownloadTask } from "@/stores/downloads";

vi.mock("@/lib/common/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

function task(accountKey: string, id: string, status: DownloadTask["status"] = "paused"): DownloadTask {
  return {
    accountKey,
    id,
    title: id,
    directory: "C:\\out",
    status,
    source: { kind: "snapshot", items: [], pending: [] },
    nextIndex: 0,
    done: 0,
    failed: [],
    failedItems: [],
    usedNames: [],
    concurrency: 2,
    createdAt: 1,
    error: null,
  };
}

describe("下载任务账号作用域", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("账号 A/B 任务互不可见，摘要只统计当前账号", () => {
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    store.restoreAccountTasks("dsid:a", [task("dsid:a", "a1", "running"), task("dsid:a", "a2")]);
    store.activateAccount("dsid:b", "session-b");
    store.restoreAccountTasks("dsid:b", [task("dsid:b", "b1")]);

    expect(store.tasks.map((item) => item.id)).toEqual(["b1"]);
    expect(store.summary?.tasks).toBe(1);

    store.activateAccount("dsid:a", "session-a");
    expect(store.tasks.map((item) => item.id)).toEqual(["a1", "a2"]);
    expect(store.summary?.tasks).toBe(2);
  });

  it("key 从 Apple ID 升级为 DSID 后任务仍归属同一账号", () => {
    const store = useDownloadsStore();
    store.activateAccount("apple:test@example.com", "session-a");
    store.restoreAccountTasks("apple:test@example.com", [task("apple:test@example.com", "a1")]);

    store.rekeyAccount("apple:test@example.com", "dsid:1");

    expect(store.activeAccountKey).toBe("dsid:1");
    expect(store.tasks[0]?.accountKey).toBe("dsid:1");
    store.activateAccount("apple:test@example.com", null);
    expect(store.tasks).toHaveLength(0);
    store.activateAccount("dsid:1", "session-a");
    expect(store.tasks.map((item) => item.id)).toEqual(["a1"]);
  });

  it("create 只允许给当前活动账号建任务", () => {
    const store = useDownloadsStore();
    store.activateAccount("dsid:a", "session-a");
    const created = store.create({
      accountKey: "dsid:b",
      title: "错误账号",
      directory: "C:\\out",
      photos: [{ id: "p", fileName: "p.jpeg", takenAt: 1, isVideo: false, thumbUrl: "https://example.com/p.jpeg" }],
    });
    expect(created).toBeNull();
    expect(toast.error).toHaveBeenCalledWith("Sign in to the account that owns this task");
  });
});
