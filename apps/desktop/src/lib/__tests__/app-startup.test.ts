import { describe, expect, it, vi } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import App from "@/App.vue";
import { useAppStore } from "@/stores/app";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.reject(new Error("干净启动不应该调用后端"))),
  isTauri: vi.fn(() => false),
}));

describe("App 启动分支", () => {
  it("没有本地会话时落在唯一的 2FA 登录页", async () => {
    localStorage.clear();
    setActivePinia(createPinia());

    const wrapper = mount(App);

    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    expect(wrapper.find("h1").text()).toBe("Sign in to iCloud");
    expect(wrapper.text()).toContain("Sign in to view iCloud Photos and Find My devices");
    expect(wrapper.text()).toContain("Two-factor authentication is required");
    expect(wrapper.text()).toContain("Mainland China account");

    expect(wrapper.text()).not.toContain("Sign out");
    expect(wrapper.text()).not.toContain("Loading devices");

    expect(wrapper.text()).not.toContain("Browse iCloud Photos (2FA required) →");

    expect(useAppStore().view).toBe("photos-login");
  });

  it("未登录也能从右上角打开设置", async () => {
    localStorage.clear();
    setActivePinia(createPinia());

    const wrapper = mount(App);
    await new Promise((resolve) => setTimeout(resolve, 0));
    await wrapper.vm.$nextTick();

    const settings = wrapper.find('button[aria-label="Settings"]');
    expect(settings.exists()).toBe(true);
    await settings.trigger("click");
    await wrapper.vm.$nextTick();

    expect(useAppStore().view).toBe("settings");
    expect(wrapper.text()).toContain("Appearance");
    expect(wrapper.text()).toContain("About");

    const back = wrapper.find('button[aria-label="Back"]');
    expect(back.exists()).toBe(true);
    await back.trigger("click");
    await wrapper.vm.$nextTick();
    expect(useAppStore().view).toBe("photos-login");
  });
});
