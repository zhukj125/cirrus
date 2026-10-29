import { beforeEach, describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";

import AppTabBar from "@/components/layout/AppTabBar.vue";
import type { WorkspaceTab } from "@/constants";

function mountTabs(tabs: WorkspaceTab[], active: WorkspaceTab) {
  return mount(AppTabBar, {
    props: { tabs, active },
    global: { plugins: [createPinia()] },
  });
}

describe("AppTabBar", () => {
  beforeEach(() => localStorage.clear());

  it("renders open views as tabs and switches on selection", async () => {
    const wrapper = mountTabs(["findmy", "photos-gallery", "notes"], "photos-gallery");

    const tabs = wrapper.findAll('[role="tab"]');
    expect(tabs.map((tab) => tab.text())).toEqual(["Devices", "Photos", "Notes"]);
    expect(tabs[1].attributes("aria-selected")).toBe("true");

    await tabs[2].trigger("click");
    expect(wrapper.emitted("select")?.[0]).toEqual(["notes"]);
  });

  it("closes tabs and opens another view from the add menu", async () => {
    const wrapper = mountTabs(["findmy", "photos-gallery", "notes"], "photos-gallery");

    await wrapper.get('button[aria-label="Close Photos"]').trigger("click");
    expect(wrapper.emitted("close")?.[0]).toEqual(["photos-gallery"]);

    await wrapper.get('button[aria-label="Open a tab"]').trigger("click");
    const contacts = wrapper.findAll("button").find((button) => button.text() === "Contacts");
    expect(contacts).toBeDefined();
    await contacts!.trigger("click");
    expect(wrapper.emitted("add")?.[0]).toEqual(["contacts"]);
  });

  it("keeps the final workspace view open while settings remains closable", () => {
    const wrapper = mountTabs(["findmy", "settings"], "settings");

    expect(wrapper.find('button[aria-label="Close Devices"]').exists()).toBe(false);
    expect(wrapper.find('button[aria-label="Close Settings"]').exists()).toBe(true);
  });
});
