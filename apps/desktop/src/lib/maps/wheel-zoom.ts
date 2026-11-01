const WHEEL_LINE_HEIGHT_PX = 16;
const DEFAULT_PAGE_HEIGHT_PX = 800;
const WHEEL_PIXELS_PER_ZOOM_LEVEL = 100;
const PINCH_SENSITIVITY = 2;

function normalizeWheelPixels(deltaY: number, deltaMode: number, pageHeight: number): number {
  const multiplier = deltaMode === 1 ? WHEEL_LINE_HEIGHT_PX : deltaMode === 2 ? Math.max(pageHeight, 1) : 1;
  return deltaY * multiplier;
}

export function wheelZoomDelta(deltaY: number, deltaMode: number, ctrlKey: boolean, pageHeight = DEFAULT_PAGE_HEIGHT_PX): number {
  const pixels = normalizeWheelPixels(deltaY, deltaMode, pageHeight);
  if (!Number.isFinite(pixels) || pixels === 0) return 0;

  const levels = (-pixels / WHEEL_PIXELS_PER_ZOOM_LEVEL) * (ctrlKey ? PINCH_SENSITIVITY : 1);
  return Math.max(-1, Math.min(1, levels));
}
