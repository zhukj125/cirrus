import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import PhotosLoginView from "@/views/PhotosLoginView.vue";
import { photosFinish2fa, photosLogin, photosVerify2fa } from "@/lib/api";

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  photosLogin: vi.fn(),
  photosVerify2fa: vi.fn(),
  photosFinish2fa: vi.fn(),
  photosSendCode: vi.fn(),
  photos2faOptions: vi.fn(async () => ({ status: "OK", message: null, trustedDevice: true, trustedDeviceCount: 1, phones: [] })),
  photosCheckApproval: vi.fn(async () => ({ status: "ERROR", message: "Waiting for approval", sessionId: null, authType: null, firstName: null, lastName: null })),
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

const loginResult = {
  status: "NEED_2FA" as const,
  message: null,
  sessionId: "session-1",
  authType: "hsa2",
  firstName: null,
  lastName: null,
};

const trustRequired = {
  status: "TRUST_BROWSER_REQUIRED" as const,
  message: null,
  sessionId: "session-1",
  authType: null,
  firstName: null,
  lastName: null,
};

const finished = {
  status: "OK" as const,
  message: null,
  sessionId: "session-1",
  authType: null,
  firstName: "Peiqi",
  lastName: "Zhu",
};

function mountLogin() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(PhotosLoginView, {
    props: { loginContext: { mode: "normal", appleId: null } },
    global: { plugins: [pinia] },
  });
}

async function reachTrustPrompt(wrapper: ReturnType<typeof mountLogin>): Promise<void> {
  await wrapper.get("#apple-id").setValue("user@example.com");
  await wrapper.get("#apple-password").setValue("password");
  await wrapper.get("form").trigger("submit");
  await vi.waitFor(() => expect(wrapper.findAll('input[inputmode="numeric"]')).toHaveLength(6));

  const digits = wrapper.findAll('input[inputmode="numeric"]');
  for (let index = 0; index < digits.length; index += 1) {
    await digits[index]!.setValue(String(index + 1));
  }

  await vi.waitFor(() => expect(vi.mocked(photosVerify2fa)).toHaveBeenCalledWith({ sessionId: "session-1", code: "123456" }));
  await vi.waitFor(() => expect(wrapper.find('[data-testid="trust-browser-prompt"]').exists()).toBe(true));
}

describe("2FA browser trust choice", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(photosLogin).mockResolvedValue(loginResult);
    vi.mocked(photosVerify2fa).mockResolvedValue(trustRequired);
    vi.mocked(photosFinish2fa).mockResolvedValue(finished);
  });

  it("trusts the browser when the user chooses Trust", async () => {
    const wrapper = mountLogin();
    await reachTrustPrompt(wrapper);

    expect(wrapper.get('[data-testid="trust-browser-prompt"]').text()).toContain("Trust this browser?");
    await wrapper.get('[data-testid="trust-browser-prompt"]').findAll("button")[0]!.trigger("click");

    await vi.waitFor(() => expect(vi.mocked(photosFinish2fa)).toHaveBeenCalledWith({ sessionId: "session-1", trustBrowser: true }));
    await vi.waitFor(() => expect(wrapper.emitted("success")?.[0]).toEqual(["session-1"]));
    wrapper.unmount();
  });

  it("continues without trusting the browser when the user declines", async () => {
    const wrapper = mountLogin();
    await reachTrustPrompt(wrapper);

    await wrapper.get('[data-testid="trust-browser-prompt"]').findAll("button")[1]!.trigger("click");

    await vi.waitFor(() => expect(vi.mocked(photosFinish2fa)).toHaveBeenCalledWith({ sessionId: "session-1", trustBrowser: false }));
    await vi.waitFor(() => expect(wrapper.emitted("success")?.[0]).toEqual(["session-1"]));
    wrapper.unmount();
  });
});

describe("登录区域策略", () => {
  it("前端只调用一次登录接口，不负责地区回退", async () => {
    vi.mocked(photosLogin).mockReset();
    vi.mocked(photosLogin).mockResolvedValueOnce(finished);

    const wrapper = mountLogin();
    expect(wrapper.find("#china-mainland").exists()).toBe(false);

    await wrapper.get("#apple-id").setValue("user@example.com");
    await wrapper.get("#apple-password").setValue("password");
    await wrapper.get("form").trigger("submit");

    await vi.waitFor(() => expect(vi.mocked(photosLogin)).toHaveBeenCalledTimes(1));
    expect(vi.mocked(photosLogin)).toHaveBeenCalledWith({
      appleId: "user@example.com",
      password: "password",
    });
    await vi.waitFor(() => expect(wrapper.emitted("success")?.[0]).toEqual(["session-1"]));
    wrapper.unmount();
  });
});
