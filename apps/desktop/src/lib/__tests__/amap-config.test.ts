import { beforeEach, describe, expect, it, vi } from "vitest";

import { StorageKey } from "@/constants";
import { writeStored } from "@/lib/common/storage";

describe("AMap configuration", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.unstubAllEnvs();
    vi.stubEnv("VITE_AMAP_KEY", "");
    vi.stubEnv("VITE_AMAP_SECURITY_CODE", "");
    vi.resetModules();
  });

  it("returns no key when neither settings nor environment provide one", async () => {
    const { getAmapConfig } = await import("@/lib/amap");
    expect(getAmapConfig()).toEqual({ key: "", securityCode: "" });
  });

  it("uses a local key and trims saved values", async () => {
    writeStored(StorageKey.amapKey, "  custom-amap-key  ");
    writeStored(StorageKey.amapSecurityCode, "  security-code  ");
    const { getAmapConfig } = await import("@/lib/amap");

    expect(getAmapConfig()).toEqual({ key: "custom-amap-key", securityCode: "security-code" });
  });

  it("falls back to environment values for preview", async () => {
    vi.stubEnv("VITE_AMAP_KEY", " env-key ");
    vi.stubEnv("VITE_AMAP_SECURITY_CODE", " env-security ");
    const { getAmapConfig, hasEnvironmentAmapKey } = await import("@/lib/amap");

    expect(getAmapConfig()).toEqual({ key: "env-key", securityCode: "env-security" });
    expect(hasEnvironmentAmapKey()).toBe(true);
  });

  it("prefers a local key over the environment key", async () => {
    vi.stubEnv("VITE_AMAP_KEY", "env-key");
    writeStored(StorageKey.amapKey, "local-key");
    const { getAmapConfig } = await import("@/lib/amap");

    expect(getAmapConfig().key).toBe("local-key");
  });
});
