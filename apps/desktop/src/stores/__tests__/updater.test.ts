import { beforeEach, describe, expect, it, vi } from "vitest";
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
}));

import { useUpdaterStore } from "@/stores/updater";

const update = {
  currentVersion: "1.4.5",
  version: "1.5.0",
  body: "A signed update.",
  date: "2026-10-30T00:00:00Z",
};

describe("updater store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    api.listenProgress.mockResolvedValue(() => undefined);
  });

  it("reports when the app is already up to date", async () => {
    api.check.mockResolvedValue(null);
    const updater = useUpdaterStore();

    await expect(updater.check()).resolves.toBeNull();
    expect(updater.checked).toBe(true);
    expect(updater.hasUpdate).toBe(false);
    expect(updater.error).toBeNull();
  });

  it("downloads an available signed update and prepares it for install", async () => {
    api.check.mockResolvedValue(update);
    api.download.mockResolvedValue(undefined);
    const updater = useUpdaterStore();

    await updater.check();
    expect(updater.info?.version).toBe("1.5.0");

    await updater.download();
    expect(api.download).toHaveBeenCalledOnce();
    expect(updater.downloaded).toBe(true);
    expect(updater.progress).toBe(100);
  });

  it("marks the update as installed and keeps errors visible", async () => {
    api.check.mockResolvedValue(update);
    api.download.mockResolvedValue(undefined);
    api.install.mockResolvedValue(undefined);
    const updater = useUpdaterStore();

    await updater.check();
    await updater.download();
    await updater.install();
    expect(updater.installed).toBe(true);
    expect(updater.downloaded).toBe(false);

    api.check.mockRejectedValue(new Error("network offline"));
    await updater.check();
    expect(updater.error).toBe("network offline");
  });
});
