import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { Status } from "@/constants";
import { mockAccountStore } from "@/lib/accounts/__tests__/helpers";
import { accountAvatar, accountStoreSave, sessionLogout, sessionState } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import type { KnownAccount, SessionStateResponse } from "@/types";

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  accountAvatar: vi.fn(async () => ({ dataUrl: "" })),
  runtimeInfo: vi.fn(async () => ({ vaultEnabled: true })),
  sessionState: vi.fn(),
  sessionLogout: vi.fn(async () => undefined),
  refreshDevices: vi.fn(async () => ({ status: Status.success, message: null, dsid: "123", firstName: "三", lastName: "张", deviceCount: 1, devices: [] })),
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

function state(overrides: Partial<SessionStateResponse> = {}): SessionStateResponse {
  return { valid: true, verified: true, firstName: null, lastName: null, appleId: "a@example.com", dsid: "123", ttlMinutes: 720, ...overrides };
}

describe("应用 store 多账号基础状态", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
    vi.clearAllMocks();
    vi.mocked(accountAvatar).mockResolvedValue({ dataUrl: "data:image/jpeg;base64,avatar" });
  });

  it("账号校验成功后会加载并缓存头像", async () => {
    const valid = account("dsid:111", "a@example.com", "session-a");
    mockAccountStore([valid], valid.key);
    vi.mocked(sessionState).mockResolvedValue(state({ appleId: valid.appleId, dsid: valid.dsid }));

    const store = useAppStore();
    await store.loadAccountStore();
    await store.validateAccounts();

    await vi.waitFor(() => expect(store.accountAvatars[valid.key]).toBe("data:image/jpeg;base64,avatar"));
    expect(accountAvatar).toHaveBeenCalledWith("session-a");
    await store.flushAccountPersistence();
    expect(accountStoreSave).toHaveBeenCalledWith(expect.objectContaining({ avatars: { [valid.key]: "data:image/jpeg;base64,avatar" } }));
  });
  it("有效账号直接切换且不注销原账号", async () => {
    const a = account("dsid:111", "a@example.com", "session-a");
    const b = account("dsid:222", "b@example.com", "session-b");
    mockAccountStore([a, b]);
    vi.mocked(sessionState).mockResolvedValue(state({ appleId: b.appleId, dsid: b.dsid }));

    const store = useAppStore();
    await store.loadAccountStore();
    const switched = await store.switchAccount(b.key);

    expect(switched).toBe(true);
    expect(store.activeAccountKey).toBe(b.key);
    expect(store.photosSession).toBe("session-b");
    expect(sessionLogout).not.toHaveBeenCalled();
    await vi.waitFor(() => expect(store.devices?.sessionId).toBe("session-b"));
  });

  it("过期账号保留目录并进入重新登录上下文", async () => {
    const expired = account("dsid:111", "a@example.com", "session-a");
    mockAccountStore([expired]);
    vi.mocked(sessionState).mockResolvedValue(state({ valid: false, verified: false, appleId: expired.appleId, dsid: expired.dsid }));

    const store = useAppStore();
    await store.loadAccountStore();
    const switched = await store.switchAccount(expired.key);

    expect(switched).toBe(false);
    expect(store.view).toBe("photos-login");
    expect(store.loginContext).toEqual({ mode: "relogin-expired", appleId: expired.appleId });
    expect(store.accounts).toHaveLength(1);
    expect(store.accounts[0].lastKnownStatus).toBe("expired");
    expect(sessionLogout).not.toHaveBeenCalled();
  });

  it("无法查询会话状态时标记 unknown，不误报过期", async () => {
    const unknown = account("dsid:111", "a@example.com", "session-a");
    mockAccountStore([unknown]);
    vi.mocked(sessionState).mockRejectedValue(new Error("backend unavailable"));

    const store = useAppStore();
    await store.loadAccountStore();
    const switched = await store.switchAccount(unknown.key);

    expect(switched).toBe(false);
    expect(store.accounts[0].lastKnownStatus).toBe("unknown");
    expect(store.view).toBe("photos-login");
  });
});
