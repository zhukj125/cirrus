import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { Status } from "@/constants";
import { mockAccountStore } from "@/lib/accounts/__tests__/helpers";
import { refreshDevices, sessionState } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { useAppStore } from "@/stores/app";
import type { DeviceInfoResult, KnownAccount } from "@/types";

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  accountAvatar: vi.fn(async () => ({ dataUrl: "" })),
  runtimeInfo: vi.fn(async () => ({ vaultEnabled: true })),
  sessionState: vi.fn(),
  sessionLogout: vi.fn(async () => undefined),
  refreshDevices: vi.fn(),
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

function success(deviceCount: number): DeviceInfoResult {
  return { status: Status.success, message: null, dsid: null, firstName: null, lastName: null, deviceCount, devices: [] };
}

describe("账号切换竞态", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("A 的迟到设备结果不会覆盖已经切换到的 B", async () => {
    const a = account("dsid:111", "a@example.com", "session-a");
    const b = account("dsid:222", "b@example.com", "session-b");
    mockAccountStore([a, b], a.key);
    vi.mocked(sessionState).mockImplementation(async (sessionId: string) => ({ valid: true, verified: true, firstName: null, lastName: null, appleId: sessionId === "session-a" ? a.appleId : b.appleId, dsid: sessionId === "session-a" ? "111" : "222", ttlMinutes: 720 }));

    let resolveA: ((value: DeviceInfoResult) => void) | null = null;
    vi.mocked(refreshDevices).mockImplementation((sessionId: string) => {
      if (sessionId === "session-a") return new Promise((resolve) => (resolveA = resolve));
      return Promise.resolve(success(2));
    });

    const store = useAppStore();
    await store.loadAccountStore();
    await store.switchAccount(a.key);
    await vi.waitFor(() => expect(refreshDevices).toHaveBeenCalledWith("session-a"));

    await store.switchAccount(b.key);
    await vi.waitFor(() => expect(store.devices?.result.deviceCount).toBe(2));
    resolveA?.(success(99));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(store.activeAccountKey).toBe(b.key);
    expect(store.devices?.sessionId).toBe("session-b");
    expect(store.devices?.result.deviceCount).toBe(2);
  });

  it("旧账号迟到的刷新错误不会弹出到新账号界面", async () => {
    const a = account("dsid:111", "a@example.com", "session-a");
    const b = account("dsid:222", "b@example.com", "session-b");
    mockAccountStore([a, b], a.key);
    vi.mocked(sessionState).mockImplementation(async (sessionId: string) => ({ valid: true, verified: true, firstName: null, lastName: null, appleId: sessionId === "session-a" ? a.appleId : b.appleId, dsid: sessionId === "session-a" ? "111" : "222", ttlMinutes: 720 }));

    let resolveLateError: ((value: DeviceInfoResult) => void) | null = null;
    vi.mocked(refreshDevices).mockResolvedValueOnce(success(1));

    const store = useAppStore();
    await store.loadAccountStore();
    await store.switchAccount(a.key);
    await vi.waitFor(() => expect(store.devices?.sessionId).toBe("session-a"));

    vi.mocked(refreshDevices).mockReset();
    vi.mocked(refreshDevices).mockImplementation((sessionId: string) => {
      if (sessionId === "session-a") return new Promise((resolve) => (resolveLateError = resolve));
      return Promise.resolve(success(2));
    });
    const refreshingA = store.refreshActiveDevices();
    await vi.waitFor(() => expect(resolveLateError).not.toBeNull());

    await store.switchAccount(b.key);
    await vi.waitFor(() => expect(store.devices?.sessionId).toBe("session-b"));
    resolveLateError?.({
      status: Status.error,
      message: "A account failed",
      dsid: null,
      firstName: null,
      lastName: null,
      deviceCount: 0,
      devices: [],
    });
    await refreshingA;

    expect(vi.mocked(toast.error).mock.calls.some(([message]) => message === "A account failed")).toBe(false);
    expect(store.devices?.sessionId).toBe("session-b");
  });
});
