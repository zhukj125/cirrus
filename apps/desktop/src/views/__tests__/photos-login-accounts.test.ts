import { describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import PhotosLoginView from "@/views/PhotosLoginView.vue";
import type { KnownAccount, LoginContext } from "@/types";

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  photosLogin: vi.fn(),
  photosVerify2fa: vi.fn(),
  photosSendCode: vi.fn(),
  photos2faOptions: vi.fn(async () => ({ status: "OK", message: null, trustedDevice: true, trustedDeviceCount: 1, phones: [] })),
  photosCheckApproval: vi.fn(),
  photosStatus: vi.fn(async () => ({ state: "PENDING", authenticated: false })),
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

function account(key: string, appleId: string, status: KnownAccount["lastKnownStatus"] = "valid"): KnownAccount {
  return {
    key,
    appleId,
    dsid: key.startsWith("dsid:") ? key.slice(5) : null,
    firstName: "三",
    lastName: "张",
    photosSessionId: `session-${key}`,
    devicesSessionId: `session-${key}`,
    lastUsedAt: 1,
    lastKnownStatus: status,
  };
}

function mountLogin(context: Partial<LoginContext> = {}, accounts: KnownAccount[] = [], activeAccountKey: string | null = null, pendingReloginKey: string | null = null, avatars: Record<string, string> = {}) {
  const loginContext: LoginContext = { mode: "normal", appleId: null, ...context };
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(PhotosLoginView, { props: { accounts, activeAccountKey, avatars, loginContext, pendingReloginKey }, global: { plugins: [pinia] } });
}

function buttonByText(wrapper: ReturnType<typeof mountLogin>, text: string) {
  const button = wrapper.findAll("button").find((item) => item.text().includes(text));
  if (!button) throw new Error(`button not found: ${text}`);
  return button;
}

describe("登录页多账号入口", () => {
  it("选中过期账号后显示账号身份与两个选择", async () => {
    const expired = account("dsid:111", "old@example.com", "expired");
    const wrapper = mountLogin({ mode: "relogin-expired", appleId: expired.appleId }, [expired], expired.key, expired.key);

    expect(wrapper.get('[data-testid="relogin-prompt"]').text()).toContain("old@example.com");
    expect(wrapper.text()).toContain("Sign in to this account");
    expect(wrapper.text()).toContain("Use another account");
    expect(wrapper.find("#apple-id").exists()).toBe(false);
  });

  it("重新登录时 Apple ID 预填且只读，提交仍使用该账号", async () => {
    const expired = account("dsid:111", "old@example.com", "expired");
    const wrapper = mountLogin({ mode: "relogin-expired", appleId: expired.appleId }, [expired], expired.key, expired.key);

    await buttonByText(wrapper, "Sign in to this account").trigger("click");
    await wrapper.vm.$nextTick();

    const input = wrapper.get("#apple-id").element as HTMLInputElement;
    expect(input.value).toBe("old@example.com");
    expect(input.readOnly).toBe(true);
  });

  it("切换其他账号会清空重登上下文并允许编辑 Apple ID", async () => {
    const expired = account("dsid:111", "old@example.com", "expired");
    const wrapper = mountLogin({ mode: "relogin-expired", appleId: expired.appleId }, [expired], expired.key, expired.key);

    await buttonByText(wrapper, "Use another account").trigger("click");
    expect(wrapper.emitted("switchOther")).toHaveLength(1);

    await wrapper.setProps({ loginContext: { mode: "switch", appleId: null }, pendingReloginKey: null });
    const input = wrapper.get("#apple-id").element as HTMLInputElement;
    expect(input.value).toBe("");
    expect(input.readOnly).toBe(false);
    expect(wrapper.find('[data-testid="known-accounts"]').exists()).toBe(true);
  });

  it("已知账号列表优先显示缓存头像", () => {
    const expired = account("dsid:222", "b@example.com", "expired");
    const wrapper = mountLogin({ mode: "switch", appleId: null }, [expired], expired.key, null, {
      [expired.key]: "data:image/jpeg;base64,avatar",
    });

    const list = wrapper.get('[data-testid="known-accounts"]');
    expect(list.get("img").attributes("src")).toBe("data:image/jpeg;base64,avatar");
  });
  it("已知账号列表显示状态并发出选择事件", async () => {
    const active = account("dsid:111", "a@example.com", "valid");
    const expired = account("dsid:222", "b@example.com", "expired");
    const wrapper = mountLogin({ mode: "switch", appleId: null }, [active, expired], active.key);

    const list = wrapper.get('[data-testid="known-accounts"]');
    expect(list.text()).toContain("Available");
    expect(list.text()).toContain("Sign in again");
    expect(list.text()).toContain("Current");

    await wrapper.get('[aria-label="Switch to b@example.com"]').trigger("click");
    expect(wrapper.emitted("selectAccount")?.[0]).toEqual([expired.key]);
  });
});
