import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

const api = vi.hoisted(() => ({
  check: vi.fn(),
  download: vi.fn(),
  cancel: vi.fn(),
  install: vi.fn(),
  restart: vi.fn(),
  listenProgress: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  appUpdateCheck: api.check,
  appUpdateDownload: api.download,
  appUpdateCancelDownload: api.cancel,
  appUpdateInstall: api.install,
  appUpdateRestart: api.restart,
  listenAppUpdateProgress: api.listenProgress,
  openExternal: vi.fn(async () => undefined),
}));

import UpdateDialog from "@/components/layout/UpdateDialog.vue";
import { useUpdaterStore } from "@/stores/updater";

const update = {
  currentVersion: "1.4.6",
  version: "1.5.0",
  body: "### Changes\n- Device address fix",
  date: "2026-10-30T00:00:00Z",
};

describe("UpdateDialog", () => {
  let pinia: ReturnType<typeof createPinia>;

  beforeEach(() => {
    pinia = createPinia();
    setActivePinia(pinia);
    vi.clearAllMocks();
    api.listenProgress.mockResolvedValue(() => undefined);
    api.download.mockResolvedValue(undefined);
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: true, json: async () => [] }) as Response),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("shows update center details, release notes, and download action", async () => {
    api.check.mockResolvedValue(update);
    const updater = useUpdaterStore();
    await updater.check();
    const wrapper = mount(UpdateDialog, { props: { currentVersion: "1.4.6" }, global: { plugins: [pinia] } });

    expect(wrapper.get('[role="dialog"]').text()).toContain("Update center");
    expect(wrapper.text()).toContain("Changelog");
    expect(wrapper.text()).toContain("Current version v1.4.6. Latest version v1.5.0.");
    expect(wrapper.text()).toContain("Device address fix");
    expect(wrapper.findAll("button").some((button) => button.text().includes("Download in background"))).toBe(true);

    await wrapper
      .findAll("button")
      .find((button) => button.text().includes("Download in background"))!
      .trigger("click");
    await flushPromises();
    expect(api.download).toHaveBeenCalledOnce();
    expect(wrapper.text()).toContain("The update is ready to install.");
    expect(wrapper.text()).toContain("Exit and install");
  });

  it("supports changelog and collapsible version history tabs", async () => {
    api.check.mockResolvedValue(null);
    const updater = useUpdaterStore();
    await updater.check();
    const wrapper = mount(UpdateDialog, { props: { currentVersion: "1.4.6" }, global: { plugins: [pinia] } });
    await flushPromises();

    await wrapper
      .findAll('[role="tab"]')
      .find((tab) => tab.text() === "Changelog")!
      .trigger("click");
    expect(wrapper.text()).toContain("This release has no notes.");

    await wrapper
      .findAll('[role="tab"]')
      .find((tab) => tab.text() === "Cirrus")!
      .trigger("click");
    await wrapper
      .findAll("button")
      .find((button) => button.text() === "Version history")!
      .trigger("click");
    expect(wrapper.text()).toContain("No older versions available.");
  });
  it("shows an up-to-date state when no release is available", async () => {
    api.check.mockResolvedValue(null);
    const updater = useUpdaterStore();
    await updater.check();
    const wrapper = mount(UpdateDialog, { props: { currentVersion: "1.4.6" }, global: { plugins: [pinia] } });

    expect(wrapper.text()).toContain("Version v1.4.6 is up to date.");
    expect(wrapper.findAll("button").some((button) => button.text().includes("Check again"))).toBe(true);
  });
});
