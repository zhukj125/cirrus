import { describe, expect, it } from "vitest";

import { wheelZoomDelta } from "@/lib/maps/wheel-zoom";

describe("map wheel zoom", () => {
  it("maps a standard mouse wheel notch to one zoom level", () => {
    expect(wheelZoomDelta(-100, 0, false)).toBe(1);
    expect(wheelZoomDelta(100, 0, false)).toBe(-1);
  });

  it("keeps high-resolution touchpad deltas fractional", () => {
    expect(wheelZoomDelta(-5, 0, false)).toBeCloseTo(0.05);
    expect(wheelZoomDelta(-5, 0, true)).toBeCloseTo(0.1);
  });

  it("normalizes line and page wheel modes", () => {
    expect(wheelZoomDelta(-10, 1, false)).toBe(1);
    expect(wheelZoomDelta(-1, 2, false, 50)).toBeCloseTo(0.5);
  });

  it("ignores empty and invalid wheel events", () => {
    expect(wheelZoomDelta(0, 0, false)).toBe(0);
    expect(wheelZoomDelta(Number.NaN, 0, false)).toBe(0);
  });
});
