import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { Status } from "@/constants";
import { mockAccountStore } from "@/lib/accounts/__tests__/helpers";
import { loadDownloadTasks, refreshDevices, rekeyDownloadTasks, sessionLogout, sessionState } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import type { KnownAccount } from "@/types";

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  accountAvatar: vi.fn(async () => ({ dataUrl: "" })),
  runtimeInfo: vi.fn(async () => ({ vaultEnabled: true })),
  sessionState: vi.fn(),
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

const sessions: Record<string, { valid: boolean; verified: boolean; firstName: string | null; lastName: string | null; appleId: string | null; dsid: string | null; ttlMinutes: number }> = {};

describe("登录成功收口", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
    vi.clearAllMocks();
    for (const key of Object.keys(sessions)) delete sessions[key];
    vi.mocked(sessionState).mockImplementation(async (sessionId: string) => sessions[sessionId] ?? { valid: false, verified: false, firstName: null, lastName: null, appleId: null, dsid: null, ttlMinutes: 720 });
  });

  it("首次登录新增账号并设为当前账号", async () => {
    sessions["session-a"] = { valid: true, verified: true, firstName: "三", lastName: "张", appleId: "a@example.com", dsid: "111", ttlMinutes: 720 };

    const store = useAppStore();
    await store.loadAccountStore();
    const accepted = await store.acceptPhotosSession("session-a");

    expect(accepted).toBe(true);
    expect(store.accounts).toHaveLength(1);
    expect(store.accounts[0].appleId).toBe("a@example.com");
    expect(store.accounts[0].photosSessionId).toBe("session-a");
    expect(store.activeAccountKey).toBe("dsid:111");
    expect(store.view).toBe("findmy");
    expect(sessionLogout).not.toHaveBeenCalled();
    expect(loadDownloadTasks).toHaveBeenCalledWith("dsid:111");
    await vi.waitFor(() => expect(refreshDevices).toHaveBeenCalledWith("session-a"));
  });

  it("拿到 DSID 后把旧 Apple ID 任务分区迁移到新 key", async () => {
    const existing = account("apple:a@example.com", "a@example.com", "session-old");
    mockAccountStore([existing]);
    sessions["session-new"] = { valid: true, verified: true, firstName: "三", lastName: "张", appleId: "a@example.com", dsid: "111", ttlMinutes: 720 };

    const store = useAppStore();
    await store.loadAccountStore();
    await store.acceptPhotosSession("session-new");

    expect(rekeyDownloadTasks).toHaveBeenCalledWith("apple:a@example.com", "dsid:111");
    expect(loadDownloadTasks).toHaveBeenCalledWith("dsid:111");
  });

  it("重复登录同一账号只更新会话，不新增条目", async () => {
    const existing = account("dsid:111", "a@example.com", "session-old");
    mockAccountStore([existing]);
    sessions["session-new"] = { valid: true, verified: true, firstName: "三", lastName: "张", appleId: "a@example.com", dsid: "111", ttlMinutes: 720 };

    const store = useAppStore();
    await store.loadAccountStore();
    await store.acceptPhotosSession("session-new");

    expect(store.accounts).toHaveLength(1);
    expect(store.accounts[0].key).toBe("dsid:111");
    expect(store.accounts[0].photosSessionId).toBe("session-new");
    expect(store.accounts[0].devicesSessionId).toBe("session-new");
    expect(sessionLogout).not.toHaveBeenCalled();
  });

  it("改登其他账号保留原账号条目", async () => {
    const first = account("dsid:111", "a@example.com", "session-a");
    mockAccountStore([first]);
    sessions["session-b"] = { valid: true, verified: true, firstName: null, lastName: null, appleId: "b@example.com", dsid: "222", ttlMinutes: 720 };

    const store = useAppStore();
    await store.loadAccountStore();
    await store.acceptPhotosSession("session-b");

    expect(store.accounts).toHaveLength(2);
    expect(store.accounts.find((item) => item.key === first.key)?.photosSessionId).toBe("session-a");
    expect(store.activeAccountKey).toBe("dsid:222");
  });
});
