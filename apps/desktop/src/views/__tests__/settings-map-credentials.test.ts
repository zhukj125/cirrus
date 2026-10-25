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

  it("uses editable download concurrency defaults of 5 tasks and 8 photos", async () => {
    localStorage.clear();
    const wrapper = mount(SettingsView, { global: { plugins: [createPinia()] } });
    const downloadsButton = wrapper.findAll("nav button").find((button) => button.text() === "Downloads");
    expect(downloadsButton).toBeDefined();
    await downloadsButton!.trigger("click");

    const taskInput = wrapper.get('input[aria-label="Concurrent tasks"]');
    const photoInput = wrapper.get('input[aria-label="Photos per task"]');
    expect((taskInput.element as HTMLInputElement).value).toBe("5");
    expect((photoInput.element as HTMLInputElement).value).toBe("8");

    await taskInput.setValue("3");
    await taskInput.trigger("change");
    await photoInput.setValue("10");
    await photoInput.trigger("change");
    expect(localStorage.getItem(StorageKey.downloadTaskConcurrency)).toBe("3");
    expect(localStorage.getItem(StorageKey.downloadConcurrency)).toBe("10");
  });
});
