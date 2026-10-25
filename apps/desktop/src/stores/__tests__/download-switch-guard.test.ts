import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { Status } from "@/constants";
import { mockAccountStore } from "@/lib/accounts/__tests__/helpers";
import { sessionState } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { useDownloadsStore } from "@/stores/downloads";
import type { KnownAccount } from "@/types";

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  accountAvatar: vi.fn(async () => ({ dataUrl: "" })),
  runtimeInfo: vi.fn(async () => ({ vaultEnabled: true, downloadTaskPersistenceEnabled: true })),
  sessionState: vi.fn(async () => ({ valid: true, verified: true, firstName: null, lastName: null, appleId: "b@example.com", dsid: "222", ttlMinutes: 720 })),
  sessionLogout: vi.fn(async () => undefined),
  refreshDevices: vi.fn(async () => ({ status: Status.success, message: null, dsid: null, firstName: null, lastName: null, deviceCount: 0, devices: [] })),
  playSound: vi.fn(async () => ({ success: true })),
  photosList: vi.fn(),
  savePhotoTo: vi.fn(),
  loadDownloadTasks: vi.fn(async () => ({ tasks: [], persistenceEnabled: true, writable: true, warning: null })),
  saveDownloadTasks: vi.fn(async () => undefined),
  rekeyDownloadTasks: vi.fn(async () => undefined),
  deleteDownloadTasks: vi.fn(async () => undefined),
  setDownloadSpeedLimit: vi.fn(async () => undefined),
  listenDownloadProgress: vi.fn(async () => () => undefined),
  cancelDownloadTransfer: vi.fn(async () => undefined),
}));

vi.mock("@/lib/common/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn(), dismiss: vi.fn() },
}));

function account(key: string, appleId: string, sessionId: string): KnownAccount {
  return {
    key,
    appleId,
    dsid: key.startsWith("dsid:") ? key.slice(5) : null,
    firstName: null,
    lastName: null,
    photosSessionId: sessionId,
    devicesSessionId: sessionId,
    lastUsedAt: 1,
    lastKnownStatus: "valid",
  };
}

describe("下载任务与账号切换", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("没有任务时允许切换", async () => {
    const a = account("dsid:111", "a@example.com", "session-a");
    const b = account("dsid:222", "b@example.com", "session-b");
    mockAccountStore([a, b], a.key);

    const store = useAppStore();
    await store.loadAccountStore();
    expect(await store.switchAccount(b.key)).toBe(true);
    expect(store.activeAccountKey).toBe(b.key);
  });

  it("有运行中任务时仍允许切换，任务保留在原账号且目标账号为空", async () => {
    const a = account("dsid:111", "a@example.com", "session-a");
    const b = account("dsid:222", "b@example.com", "session-b");
    mockAccountStore([a, b], a.key);

    const downloads = useDownloadsStore();
    downloads.activateAccount(a.key, "session-a");
    downloads.restoreAccountTasks(a.key, [
      {
        accountKey: a.key,
        id: "task-1",
        title: "下载测试",
        directory: "C:\\tmp",
        status: "running",
        source: { kind: "snapshot", items: [], pending: [] },
        nextIndex: 0,
        done: 0,
        failed: [],
        failedItems: [],
        usedNames: [],
        concurrency: 4,
        createdAt: 0,
        error: null,
      },
    ]);

    const store = useAppStore();
    await store.loadAccountStore();
    expect(await store.switchAccount(b.key)).toBe(true);
    expect(store.activeAccountKey).toBe(b.key);
    expect(downloads.tasks).toHaveLength(0);
    expect(sessionState).toHaveBeenCalled();

    expect(await store.switchAccount(a.key)).toBe(true);
    expect(downloads.tasks.map((task) => task.id)).toEqual(["task-1"]);
  });
});
