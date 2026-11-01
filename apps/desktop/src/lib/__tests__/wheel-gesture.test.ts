import { describe, expect, it } from "vitest";

import { mapWheelGesture } from "@/lib/maps/wheel-gesture";

describe("map wheel gestures", () => {
  it("maps a standard mouse wheel notch to one zoom level", () => {
    expect(mapWheelGesture(0, -100, 0, false)).toEqual({ type: "zoom", delta: 1 });
    expect(mapWheelGesture(0, 100, 0, false)).toEqual({ type: "zoom", delta: -1 });
  });

  it("uses pinch wheel deltas for fractional touchpad zoom", () => {
    const gesture = mapWheelGesture(0, -5, 0, true);
    expect(gesture.type).toBe("zoom");
    if (gesture.type === "zoom") expect(gesture.delta).toBeCloseTo(0.1);
  });

  it("pans with unmodified two-finger touchpad scrolling", () => {
    expect(mapWheelGesture(10, 20, 0, false)).toEqual({ type: "pan", deltaX: -10, deltaY: -20 });
  });

  it("normalizes line and page wheel modes", () => {
    expect(mapWheelGesture(0, -10, 1, false)).toEqual({ type: "zoom", delta: 1 });
    expect(mapWheelGesture(0, -1, 2, false, 50)).toEqual({ type: "zoom", delta: 0.5 });
  });

  it("ignores empty and invalid wheel events", () => {
    expect(mapWheelGesture(0, 0, 0, false)).toEqual({ type: "none" });
    expect(mapWheelGesture(0, Number.NaN, 0, false)).toEqual({ type: "none" });
  });
});
