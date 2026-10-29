import { describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import PhotosLoginView from "@/views/PhotosLoginView.vue";

vi.mock("@/lib/api", () => ({
  photosLogin: vi.fn(),
  photosVerify2fa: vi.fn(),
  photosFinish2fa: vi.fn(),
  photosSendCode: vi.fn(),
  photos2faOptions: vi.fn(),
  photosCheckApproval: vi.fn(),
  photosStatus: vi.fn(),
}));

vi.mock("@/lib/common/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn(), dismiss: vi.fn() },
}));

describe("MCP sign-in login view", () => {
  it("shows the authorized client and emits cancelMcp", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mount(PhotosLoginView, {
      props: {
        loginContext: { mode: "mcp", appleId: null, clientName: "Codex" },
      },
      global: { plugins: [pinia] },
    });

    const banner = wrapper.get('[data-testid="mcp-login-banner"]');
    expect(banner.text()).toContain("Codex");
    expect(banner.text()).toContain("is authorized to start sign-in");

    const cancel = wrapper.findAll("button").find((button) => button.text() === "Cancel MCP sign-in");
    expect(cancel).toBeDefined();
    await cancel!.trigger("click");
    expect(wrapper.emitted("cancelMcp")).toHaveLength(1);
  });
});
