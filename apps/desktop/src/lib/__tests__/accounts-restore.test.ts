import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { StorageKey } from "@/constants";
import { mockAccountStore } from "@/lib/accounts/__tests__/helpers";
import { loadDownloadTasks, sessionState } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import type { KnownAccount } from "@/types";

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  accountAvatar: vi.fn(async () => ({ dataUrl: "" })),
  runtimeInfo: vi.fn(async () => ({ vaultEnabled: true, downloadTaskPersistenceEnabled: true })),
  sessionState: vi.fn(),
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

describe("账号目录启动恢复", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("恢复最近使用的有效账号，默认进入设备页并忽略上次页面", async () => {
    const active = account("dsid:111", "a@example.com", "session-a");
    mockAccountStore([active], active.key);
    localStorage.setItem(StorageKey.view, "photos-gallery");
    vi.mocked(sessionState).mockResolvedValue({ valid: true, verified: true, firstName: "三", lastName: null, appleId: active.appleId, dsid: active.dsid, ttlMinutes: 720 });

    const store = useAppStore();
    await store.restore();

    expect(store.view).toBe("findmy");
    expect(store.devices).not.toBeNull();
    expect(store.activeAccountKey).toBe(active.key);
    expect(store.accounts).toHaveLength(1);
    expect(store.accounts[0].firstName).toBe("三");
    expect(sessionState).toHaveBeenCalledWith("session-a");
    expect(loadDownloadTasks).toHaveBeenCalledWith(active.key);
  });

  it("当前账号失效时保留其他账号并落在账号选择页", async () => {
    const active = account("dsid:111", "a@example.com", "session-a");
    const other = account("dsid:222", "b@example.com", "session-b");
    mockAccountStore([active, other], active.key);
    vi.mocked(sessionState).mockResolvedValue({ valid: false, verified: false, firstName: null, lastName: null, appleId: active.appleId, dsid: active.dsid, ttlMinutes: 720 });

    const store = useAppStore();
    await store.restore();

    expect(store.view).toBe("photos-login");
    expect(store.loginContext).toEqual({ mode: "switch", appleId: null });
    expect(store.activeAccountKey).toBe(active.key);
    expect(store.accounts).toHaveLength(2);
    expect(store.accounts.find((item) => item.key === active.key)?.lastKnownStatus).toBe("expired");
    expect(store.accounts.find((item) => item.key === other.key)?.lastKnownStatus).toBe("valid");
    expect(sessionState).toHaveBeenCalledTimes(1);
  });

  it("全部账号失效时仍保留目录并显示登录入口", async () => {
    const only = account("dsid:111", "a@example.com", "session-a");
    mockAccountStore([only], only.key);
    vi.mocked(sessionState).mockResolvedValue({ valid: false, verified: false, firstName: null, lastName: null, appleId: only.appleId, dsid: only.dsid, ttlMinutes: 720 });

    const store = useAppStore();
    await store.restore();

    expect(store.view).toBe("photos-login");
    expect(store.loginContext.mode).toBe("switch");
    expect(store.accounts).toHaveLength(1);
    expect(store.accounts[0].lastKnownStatus).toBe("expired");
  });
});
