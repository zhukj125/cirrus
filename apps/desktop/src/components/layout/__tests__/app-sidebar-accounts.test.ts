import { beforeEach, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import AppSidebar from "@/components/layout/AppSidebar.vue";
import { StorageKey } from "@/constants";
import type { KnownAccount } from "@/types";

function account(key: string, appleId: string, status: KnownAccount["lastKnownStatus"], lastUsedAt = Date.now()): KnownAccount {
  return {
    key,
    appleId,
    dsid: key.startsWith("dsid:") ? key.slice(5) : null,
    firstName: appleId.includes("long") ? "一个非常非常长的账号显示名称" : "三",
    lastName: "张",
    photosSessionId: `session-${key}`,
    devicesSessionId: `session-${key}`,
    lastUsedAt,
    lastKnownStatus: status,
  };
}

function mountSidebar() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const active = account("dsid:111", "a@example.com", "valid");
  const valid = account("dsid:222", "b@example.com", "valid", Date.now() - 2 * 60 * 60 * 1000);
  const expired = account("dsid:333", "long-name@example.com", "expired", Date.now() - 3 * 86_400_000);
  const wrapper = mount(AppSidebar, {
    props: {
      view: "findmy",
      account: { appleId: active.appleId, dsid: active.dsid, firstName: active.firstName, lastName: active.lastName },
      deviceCount: 2,
      vaultEnabled: true,
      accounts: [active, valid, expired],
      activeAccountKey: active.key,
      accountsLoading: false,
      avatars: { [active.key]: "data:image/jpeg;base64,avatar" },
    },
    global: { plugins: [pinia] },
  });
  return { wrapper, active, valid, expired };
}

describe("侧栏账号切换器", () => {
  beforeEach(() => localStorage.clear());

  it("展开时展示可用状态、当前标记和最近使用时间", async () => {
    const { wrapper, active, expired } = mountSidebar();
    expect(wrapper.get('button[aria-controls="account-switcher"] img').attributes("src")).toBe("data:image/jpeg;base64,avatar");
    await wrapper.get('button[aria-controls="account-switcher"]').trigger("click");

    expect(wrapper.emitted("validateAccounts")).toHaveLength(1);
    const list = wrapper.get("#account-switcher");
    expect(list.text()).toContain("Available");
    expect(list.text()).toContain("Sign in again");
    expect(list.text()).toContain("2 hr ago");
    expect(wrapper.get(`[aria-label="Switch to ${expired.appleId}, Sign in again"]`).text()).toContain("Sign in again");
    expect(list.get(`[aria-label="Switch to ${active.appleId}, Available"]`).attributes("aria-selected")).toBe("true");
  });

  it("选择其他账号会发出切换事件，添加与退出使用独立事件", async () => {
    const { wrapper, valid } = mountSidebar();
    await wrapper.get('button[aria-controls="account-switcher"]').trigger("click");
    await wrapper.get(`[aria-label="Switch to ${valid.appleId}, Available"]`).trigger("click");
    expect(wrapper.emitted("switchAccount")?.[0]).toEqual([valid.key]);

    await wrapper.get('button[aria-controls="account-switcher"]').trigger("click");
    await wrapper
      .findAll("button")
      .find((button) => button.text() === "Add account")!
      .trigger("click");
    expect(wrapper.emitted("addAccount")).toHaveLength(1);

    await wrapper.get('button[aria-controls="account-switcher"]').trigger("click");
    await wrapper
      .findAll("button")
      .find((button) => button.text() === "Sign out")!
      .trigger("click");
    expect(wrapper.emitted("logout")).toHaveLength(1);
  });

  it("支持 Escape 关闭并保持长账号文本可截断", async () => {
    const { wrapper, expired } = mountSidebar();
    await wrapper.get('button[aria-controls="account-switcher"]').trigger("click");

    const longRow = wrapper.get(`[aria-label="Switch to ${expired.appleId}, Sign in again"]`);
    expect(longRow.find(".truncate").exists()).toBe(true);
    expect(longRow.text()).toContain("Sign in again");

    await wrapper.get("#account-switcher").trigger("keydown", { key: "Escape" });
    expect(wrapper.find("#account-switcher").exists()).toBe(false);
  });

  it("可以折叠为图标栏并记住状态", async () => {
    const { wrapper } = mountSidebar();

    await wrapper.get('button[aria-label="Collapse sidebar"]').trigger("click");

    expect(wrapper.get("aside").classes()).toContain("w-14");
    expect(wrapper.find('button[aria-label="Expand sidebar"]').exists()).toBe(true);
    expect(wrapper.find('button[aria-label="Photos"]').exists()).toBe(true);
    expect(wrapper.text()).not.toContain("Browse");
    expect(localStorage.getItem(StorageKey.sidebarCollapsed)).toBe("true");
  });
});
