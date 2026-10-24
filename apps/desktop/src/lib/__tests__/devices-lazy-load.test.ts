import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

import { Status, StorageKey } from "@/constants";
import { mockAccountStore } from "@/lib/accounts/__tests__/helpers";
import { useAppStore } from "@/stores/app";
import type { KnownAccount } from "@/types";

const PHOTOS_SESSION = "session-photos-0001";
const ACCOUNT_KEY = "dsid:1001";

const invoked: string[] = [];

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  accountAvatar: vi.fn(async () => ({ dataUrl: "" })),
  runtimeInfo: vi.fn(async () => ({ vaultEnabled: true })),
  sessionState: vi.fn(async () => ({ valid: true, verified: true, firstName: null, lastName: null, appleId: "person@example.com", dsid: "1001", ttlMinutes: 720 })),
  sessionLogout: vi.fn(async () => undefined),
  playSound: vi.fn(async () => ({ success: true })),
  refreshDevices: vi.fn(async () => {
    invoked.push("refresh_devices");
    return { status: Status.success, message: null, dsid: "1001", firstName: null, lastName: null, deviceCount: 2, devices: [] };
  }),
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

function savedAccount(): KnownAccount {
  return {
    key: ACCOUNT_KEY,
    appleId: "person@example.com",
    dsid: "1001",
    firstName: null,
    lastName: null,
    photosSessionId: PHOTOS_SESSION,
    devicesSessionId: PHOTOS_SESSION,
    lastUsedAt: 1,
    lastKnownStatus: "valid",
  };
}

function saveSession(): void {
  mockAccountStore([savedAccount()], ACCOUNT_KEY);
}

describe("设备页默认落点与数据复用", () => {
  beforeEach(() => {
    localStorage.clear();
    invoked.length = 0;
    setActivePinia(createPinia());
  });

  it("恢复有效账号时默认进入设备页，并忽略上次停在照片页", async () => {
    localStorage.setItem(StorageKey.view, "photos-gallery");
    saveSession();

    const store = useAppStore();
    await store.restore();

    expect(store.view).toBe("findmy");
    expect(store.devices?.result.deviceCount).toBe(2);
    expect(store.devices?.sessionId).toBe(PHOTOS_SESSION);
    expect(invoked).toContain("refresh_devices");
  });

  it("设备数据已加载时不会重复请求", async () => {
    saveSession();

    const store = useAppStore();
    await store.restore();
    const first = invoked.filter((name) => name === "refresh_devices").length;

    await store.ensureDevices();
    const afterSecond = invoked.filter((name) => name === "refresh_devices").length;

    expect(store.view).toBe("findmy");
    expect(afterSecond).toBe(first);
  });

  it("一个会话都没有时回唯一入口页，而不是转圈", async () => {
    const store = useAppStore();
    await store.restore();
    expect(store.view).toBe("photos-login");

    await store.ensureDevices();
    expect(store.view).toBe("photos-login");
    expect(store.devices).toBeNull();
  });
});
