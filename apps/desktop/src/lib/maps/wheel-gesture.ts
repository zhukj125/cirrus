const WHEEL_LINE_HEIGHT_PX = 16;
const DEFAULT_PAGE_HEIGHT_PX = 800;
const WHEEL_PIXELS_PER_ZOOM_LEVEL = 100;
const PINCH_ZOOM_IN_SENSITIVITY = 5;
const PINCH_ZOOM_OUT_SENSITIVITY = 1.5;
const TOUCHPAD_PAN_MAX_PIXEL_DELTA = 50;

function normalizeWheelPixels(delta: number, deltaMode: number, pageHeight: number): number {
  const multiplier = deltaMode === 1 ? WHEEL_LINE_HEIGHT_PX : deltaMode === 2 ? Math.max(pageHeight, 1) : 1;
  return delta * multiplier;
}

export type MapWheelGesture = { type: "zoom"; delta: number } | { type: "pan"; deltaX: number; deltaY: number } | { type: "none" };

export function mapWheelPanGesture(deltaX: number, deltaY: number, deltaMode: number, pageHeight = DEFAULT_PAGE_HEIGHT_PX): MapWheelGesture {
  const pixelsX = normalizeWheelPixels(deltaX, deltaMode, pageHeight);
  const pixelsY = normalizeWheelPixels(deltaY, deltaMode, pageHeight);
  if (!Number.isFinite(pixelsX) || !Number.isFinite(pixelsY)) return { type: "none" };
  return { type: "pan", deltaX: -pixelsX, deltaY: -pixelsY };
}

export function mapWheelGesture(deltaX: number, deltaY: number, deltaMode: number, ctrlKey: boolean, pageHeight = DEFAULT_PAGE_HEIGHT_PX): MapWheelGesture {
  const pixelsX = normalizeWheelPixels(deltaX, deltaMode, pageHeight);
  const pixelsY = normalizeWheelPixels(deltaY, deltaMode, pageHeight);
  if (!Number.isFinite(pixelsX) || !Number.isFinite(pixelsY) || (pixelsX === 0 && pixelsY === 0)) return { type: "none" };

  // Browsers report a precision-touchpad pinch as ctrl + wheel. A regular
  // mouse wheel uses large pixel, line, or page deltas; small unmodified
  // deltas are two-finger scrolling and should pan instead of zoom.
  const mouseWheel = deltaMode !== 0 || Math.abs(pixelsX) >= TOUCHPAD_PAN_MAX_PIXEL_DELTA || Math.abs(pixelsY) >= TOUCHPAD_PAN_MAX_PIXEL_DELTA;
  if (ctrlKey || mouseWheel) {
    if (pixelsY === 0) return { type: "none" };
    const pinchSensitivity = pixelsY < 0 ? PINCH_ZOOM_IN_SENSITIVITY : PINCH_ZOOM_OUT_SENSITIVITY;
    const levels = (-pixelsY / WHEEL_PIXELS_PER_ZOOM_LEVEL) * (ctrlKey ? pinchSensitivity : 1);
    return { type: "zoom", delta: Math.max(-1, Math.min(1, levels)) };
  }

  return { type: "pan", deltaX: -pixelsX, deltaY: -pixelsY };
}
