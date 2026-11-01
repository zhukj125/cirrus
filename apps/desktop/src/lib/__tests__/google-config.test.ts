import { beforeEach, describe, expect, it, vi } from "vitest";

import { StorageKey } from "@/constants";
import { writeStored } from "@/lib/common/storage";

describe("Google Maps configuration", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.unstubAllEnvs();
    vi.stubEnv("VITE_GOOGLE_MAPS_API_KEY", "");
    vi.resetModules();
  });

  it("returns no key when neither settings nor environment provide one", async () => {
    const { getGoogleMapsConfig } = await import("@/lib/maps/google-maps");
    expect(getGoogleMapsConfig()).toEqual({ apiKey: "" });
  });

  it("prefers a trimmed local key over the environment key", async () => {
    vi.stubEnv("VITE_GOOGLE_MAPS_API_KEY", "env-key");
    writeStored(StorageKey.googleMapsApiKey, "  local-google-key  ");
    const { getGoogleMapsConfig, hasEnvironmentGoogleMapsKey } = await import("@/lib/maps/google-maps");

    expect(getGoogleMapsConfig()).toEqual({ apiKey: "local-google-key" });
    expect(hasEnvironmentGoogleMapsKey()).toBe(true);
  });

  it("uses the environment value for preview builds", async () => {
    vi.stubEnv("VITE_GOOGLE_MAPS_API_KEY", " env-google-key ");
    const { getGoogleMapsConfig } = await import("@/lib/maps/google-maps");
    expect(getGoogleMapsConfig()).toEqual({ apiKey: "env-google-key" });
  });
});
