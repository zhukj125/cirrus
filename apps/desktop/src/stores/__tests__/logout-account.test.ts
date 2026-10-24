import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { mockAccountStore } from "@/lib/accounts/__tests__/helpers";
import { deleteDownloadTasks, sessionLogout } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import type { KnownAccount } from "@/types";

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  accountAvatar: vi.fn(async () => ({ dataUrl: "" })),
  runtimeInfo: vi.fn(async () => ({ vaultEnabled: true, downloadTaskPersistenceEnabled: true })),
  sessionState: vi.fn(async () => ({ valid: true, verified: true, firstName: null, lastName: null, appleId: "a@example.com", dsid: "111", ttlMinutes: 720 })),
  sessionLogout: vi.fn(async () => undefined),
  refreshDevices: vi.fn(async () => ({ status: "SUCCESS", message: null, dsid: null, firstName: null, lastName: null, deviceCount: 0, devices: [] })),
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

describe("退出当前账号", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("只删除当前账号并保留其他账号", async () => {
    const a = account("dsid:111", "a@example.com", "session-a");
    const b = account("dsid:222", "b@example.com", "session-b");
    mockAccountStore([a, b], b.key);

    const store = useAppStore();
    await store.loadAccountStore();
    await store.logoutCurrentAccount();

    expect(store.accounts.map((item) => item.key)).toEqual([a.key]);
    expect(store.activeAccountKey).toBeNull();
    expect(store.view).toBe("photos-login");
    expect(store.loginContext.mode).toBe("switch");
    expect(sessionLogout).toHaveBeenCalledWith("session-b");
    expect(sessionLogout).not.toHaveBeenCalledWith("session-a");
    expect(deleteDownloadTasks).toHaveBeenCalledWith(b.key);
    expect(deleteDownloadTasks).not.toHaveBeenCalledWith(a.key);
  });

  it("没有其他账号时回到普通登录页", async () => {
    const only = account("dsid:111", "a@example.com", "session-a");
    mockAccountStore([only], only.key);

    const store = useAppStore();
    await store.loadAccountStore();
    await store.logoutCurrentAccount();

    expect(store.accounts).toHaveLength(0);
    expect(store.activeAccountKey).toBeNull();
    expect(store.view).toBe("photos-login");
    expect(store.loginContext).toEqual({ mode: "normal", appleId: null });
    expect(sessionLogout).toHaveBeenCalledWith("session-a");
  });
});
