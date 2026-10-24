import { describe, expect, it } from "vitest";

import { DEFAULT_MAP_PROVIDER_ID, MAP_PROVIDERS, getMapProvider } from "@/lib/maps/providers";

describe("map provider registry", () => {
  it("exposes a default provider with credential fields", () => {
    const provider = getMapProvider(DEFAULT_MAP_PROVIDER_ID);
    expect(provider.id).toBe("amap");
    expect(provider.fields.length).toBeGreaterThan(0);
  });

  it("falls back safely for unknown provider ids", () => {
    expect(getMapProvider("unknown").id).toBe(MAP_PROVIDERS[0]?.id);
  });
});
