import { describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";

const api = vi.hoisted(() => ({
  check: vi.fn(async () => ({
    currentVersion: "1.4.3",
    version: "1.5.0",
    body: "Signed release notes",
    date: "2026-10-30T00:00:00Z",
  })),
  listenProgress: vi.fn(async () => () => undefined),
}));

vi.mock("@/lib/api", () => ({
  appUpdateCheck: api.check,
  appUpdateDownload: vi.fn(),
  appUpdateCancelDownload: vi.fn(),
  appUpdateInstall: vi.fn(),
  appUpdateRestart: vi.fn(),
  listenAppUpdateProgress: api.listenProgress,
}));

import SettingsView from "@/views/SettingsView.vue";

describe("SettingsView updates", () => {
  it("checks for signed updates and shows release notes in About", async () => {
    const wrapper = mount(SettingsView, { props: { initialCategory: "about", version: "1.4.3" }, global: { plugins: [createPinia()] } });

    const card = wrapper.get('[data-testid="app-update-card"]');
    expect(card.text()).toContain("Software update");
    await card
      .findAll("button")
      .find((button) => button.text().includes("Check for updates"))!
      .trigger("click");
    await flushPromises();

    expect(api.check).toHaveBeenCalledOnce();
    expect(card.text()).toContain("Version v1.5.0");
    expect(card.text()).toContain("Signed release notes");
    expect(card.text()).toContain("Download update");
  });
});
