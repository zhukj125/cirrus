import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";

import TitleBar from "@/components/layout/TitleBar.vue";

describe("TitleBar updates", () => {
  it("checks for updates from the top-right toolbar", async () => {
    const wrapper = mount(TitleBar, { props: { showUpdate: true }, global: { plugins: [createPinia()] } });
    const button = wrapper.get('button[aria-label="Check for updates"]');
    await button.trigger("click");
    expect(wrapper.emitted("check-update")).toHaveLength(1);
  });

  it("highlights an available update and disables the button while checking", async () => {
    const wrapper = mount(TitleBar, {
      props: { showUpdate: true, updateAvailable: true, checkingUpdate: true },
      global: { plugins: [createPinia()] },
    });
    const button = wrapper.get('button[aria-label="Update available"]');
    expect(button.attributes("disabled")).toBeDefined();
    expect(button.find("span.bg-primary").exists()).toBe(true);
  });
});
