import { describe, expect, it } from "vitest";

import { mapWheelGesture } from "@/lib/maps/wheel-gesture";

describe("map wheel gestures", () => {
  it("maps a standard mouse wheel notch to one zoom level", () => {
    expect(mapWheelGesture(0, -100, 0, false)).toEqual({ type: "zoom", delta: 1 });
    expect(mapWheelGesture(0, 100, 0, false)).toEqual({ type: "zoom", delta: -1 });
  });

  it("uses faster pinch zoom in than zoom out", () => {
    const zoomIn = mapWheelGesture(0, -5, 0, true);
    const zoomOut = mapWheelGesture(0, 5, 0, true);
    expect(zoomIn.type).toBe("zoom");
    expect(zoomOut.type).toBe("zoom");
    if (zoomIn.type === "zoom") expect(zoomIn.delta).toBeCloseTo(0.25);
    if (zoomOut.type === "zoom") expect(zoomOut.delta).toBeCloseTo(-0.075);
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
