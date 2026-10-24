import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { describe, expect, it } from "vitest";

import { StorageKey } from "@/constants";
import SettingsView from "@/views/SettingsView.vue";

describe("SettingsView map credentials", () => {
  it("keeps the API key hidden with a closed eye and reveals it on demand", async () => {
    localStorage.clear();
    localStorage.setItem(StorageKey.amapKey, "test-amap-key");

    const wrapper = mount(SettingsView, { global: { plugins: [createPinia()] } });
    const mapsButton = wrapper.findAll("nav button").find((button) => button.text() === "Maps");
    expect(mapsButton).toBeDefined();
    await mapsButton!.trigger("click");

    const input = wrapper.get("#map-credential-apiKey");
    expect(input.attributes("type")).toBe("password");
    expect(wrapper.get('button[aria-label="Show credential"]').html()).toContain("lucide-eye-off");

    await wrapper.get('button[aria-label="Show credential"]').trigger("click");
    expect(input.attributes("type")).toBe("text");
    expect(wrapper.get('button[aria-label="Hide credential"]').html()).toContain("lucide-eye");

    await wrapper.get('button[aria-label="Hide credential"]').trigger("click");
    expect(input.attributes("type")).toBe("password");
  });
});
