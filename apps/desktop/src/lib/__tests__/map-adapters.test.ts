import { describe, expect, it } from "vitest";

import { createMapAdapter } from "@/lib/maps";

describe("map adapter factory", () => {
  it("resolves Google Maps as a WGS84 provider", () => {
    const adapter = createMapAdapter("google");

    expect(adapter.capabilities.rotation).toBe(false);
    expect(adapter.project([121.4737, 31.2304])).toEqual([121.4737, 31.2304]);
    expect(adapter.mapUrl([121.4737, 31.2304], "Photo")).toBe("https://www.google.com/maps/search/?api=1&query=31.2304,121.4737");
  });

  it("keeps AMap on GCJ-02 for China coordinates", () => {
    const adapter = createMapAdapter("amap");
    const projected = adapter.project([121.4737, 31.2304]);

    expect(adapter.capabilities.rotation).toBe(true);
    expect(projected).not.toEqual([121.4737, 31.2304]);
    expect(adapter.mapUrl([121.4737, 31.2304], "Photo")).toContain("uri.amap.com/marker?position=");
    expect(adapter.mapUrl([121.4737, 31.2304], "Photo")).not.toContain("position=121.4737,31.2304");
  });
});
