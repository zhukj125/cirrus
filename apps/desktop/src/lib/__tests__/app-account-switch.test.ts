import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import App from "@/App.vue";
import { Status } from "@/constants";
import { mockAccountStore } from "@/lib/accounts/__tests__/helpers";
import { sessionState } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import type { KnownAccount } from "@/types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => false),
}));

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  accountAvatar: vi.fn(async () => ({ dataUrl: "" })),
  runtimeInfo: vi.fn(async () => ({ vaultEnabled: true })),
  sessionState: vi.fn(),
  sessionLogout: vi.fn(async () => undefined),
  refreshDevices: vi.fn(async () => ({ status: Status.success, message: null, dsid: null, firstName: null, lastName: null, deviceCount: 0, devices: [] })),
  playSound: vi.fn(async () => ({ success: true })),
  photosList: vi.fn(async () => ({ status: "OK", message: null, photos: [], nextCursor: null })),
  photosTotal: vi.fn(),
  photosAssetDistribution: vi.fn(),
  savePhotoTo: vi.fn(),
  loadDownloadTasks: vi.fn(async () => ({ tasks: [], persistenceEnabled: true, writable: true, warning: null })),
  saveDownloadTasks: vi.fn(async () => undefined),
  rekeyDownloadTasks: vi.fn(async () => undefined),
  deleteDownloadTasks: vi.fn(async () => undefined),
  setDownloadSpeedLimit: vi.fn(async () => undefined),
  listenDownloadProgress: vi.fn(async () => () => undefined),
  cancelDownloadTransfer: vi.fn(async () => undefined),
}));

vi.mock("@/views/PhotosGalleryView.vue", async () => {
  const { defineComponent, h } = await import("vue");
  return {
    default: defineComponent({
      props: { sessionId: { type: String, required: true } },
      setup(props) {
        return () => h("div", { "data-testid": "gallery" }, props.sessionId);
      },
    }),
  };
});

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

describe("App 账号切换接线", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("侧栏点击添加账号会进入可输入新 Apple ID 的登录页", async () => {
    const a = account("dsid:111", "a@example.com", "session-a");
    mockAccountStore([a], a.key);
    vi.mocked(sessionState).mockResolvedValue({ valid: true, verified: true, firstName: null, lastName: null, appleId: a.appleId, dsid: a.dsid, ttlMinutes: 720 });

    const store = useAppStore();
    const wrapper = mount(App);
    await vi.waitFor(() => expect(store.view).toBe("findmy"));
    await vi.waitFor(() => expect(wrapper.get('button[aria-controls="account-switcher"]').exists()).toBe(true));

    await wrapper.get('button[aria-controls="account-switcher"]').trigger("click");
    const add = wrapper.findAll("button").find((button) => button.text() === "Add account");
    expect(add).toBeTruthy();
    await add!.trigger("click");

    await vi.waitFor(() => expect(store.view).toBe("photos-login"));
    expect(wrapper.get("#apple-id").exists()).toBe(true);
    expect((wrapper.get("#apple-id").element as HTMLInputElement).readOnly).toBe(false);
  });
  it("切换账号后照片页使用新会话，旧账号内容不再显示", async () => {
    const a = account("dsid:111", "a@example.com", "session-a");
    const b = account("dsid:222", "b@example.com", "session-b");
    mockAccountStore([a, b], a.key);
    vi.mocked(sessionState).mockImplementation(async (sessionId: string) => ({
      valid: true,
      verified: true,
      firstName: null,
      lastName: null,
      appleId: sessionId === "session-a" ? a.appleId : b.appleId,
      dsid: sessionId === "session-a" ? a.dsid : b.dsid,
      ttlMinutes: 720,
    }));

    const store = useAppStore();
    const wrapper = mount(App);
    await vi.waitFor(() => expect(store.view).toBe("findmy"));

    store.goTo("photos-gallery");
    await vi.waitFor(() => expect(wrapper.get('[data-testid="gallery"]').text()).toBe("session-a"));

    await store.switchAccount(b.key);
    await vi.waitFor(() => expect(wrapper.get('[data-testid="gallery"]').text()).toBe("session-b"));
    expect(wrapper.text()).not.toContain("session-a");
  });
});
