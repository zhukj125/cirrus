import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

const mapMocks = vi.hoisted(() => ({
  load: vi.fn(async () => undefined),
  project: vi.fn((position: [number, number]) => position),
  createMap: vi.fn(),
  createMarker: vi.fn(),
  createAccuracyCircle: vi.fn(),
  setTheme: vi.fn(),
  setZoom: vi.fn(),
  panBy: vi.fn(),
}));

vi.mock("@/lib/maps", () => ({
  createMapAdapter: () => ({
    providerId: "amap",
    capabilities: { rotation: true },
    load: mapMocks.load,
    project: mapMocks.project,
    createMap: mapMocks.createMap,
    createMarker: mapMocks.createMarker,
    createAccuracyCircle: mapMocks.createAccuracyCircle,
  }),
}));

const geocodeMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/geocode", () => ({
  reverseGeocode: geocodeMock,
}));

import MapView from "@/views/MapView.vue";
import { useLocaleStore } from "@/stores/locale";
import { useThemeStore } from "@/stores/theme";
import type { DevicesResponse } from "@/types";

const data: DevicesResponse = {
  sessionId: "session",
  result: {
    status: "SUCCESS",
    message: null,
    dsid: "1",
    firstName: "Test",
    lastName: "User",
    deviceCount: 1,
    devices: [
      {
        name: "Test Phone",
        modelDisplayName: "iPhone",
        deviceDisplayName: "iPhone",
        deviceClass: "iPhone",
        serialNumber: null,
        udid: "device-1",
        osVersion: null,
        batteryStatus: "Charging",
        batteryLevel: 0.8,
        location: { latitude: 30.123456, longitude: 120.56789, horizontalAccuracy: 10, timeStamp: Date.now(), isOld: false },
        deviceImageUrl: null,
        deviceId: "device-1",
        activationLocked: false,
        lostModeEnabled: false,
        lostModePending: false,
        locationEnabled: true,
        deviceStatus: "200",
      },
    ],
  },
};

function movedData(latitude: number, longitude: number): DevicesResponse {
  const next = structuredClone(data);
  next.result.devices[0]!.location = {
    ...next.result.devices[0]!.location!,
    latitude,
    longitude,
  };
  return next;
}

describe("MapView location fallback", () => {
  beforeEach(() => {
    geocodeMock.mockReset();
    geocodeMock.mockResolvedValue(null);
    mapMocks.setTheme.mockReset();
    mapMocks.setZoom.mockReset();
    mapMocks.panBy.mockReset();
    mapMocks.createMap.mockReset();
    mapMocks.createMarker.mockReset();
    mapMocks.createAccuracyCircle.mockReset();
    mapMocks.createMap.mockReturnValue({
      getRotation: () => 0,
      setRotation: vi.fn(),
      getZoom: () => 11,
      setZoom: mapMocks.setZoom,
      setZoomAndCenter: vi.fn(),
      panBy: mapMocks.panBy,
      fitMarkers: vi.fn(),
      zoomIn: vi.fn(),
      zoomOut: vi.fn(),
      setTheme: mapMocks.setTheme,
      destroy: vi.fn(),
    });
    mapMocks.createMarker.mockImplementation(() => ({
      setPosition: vi.fn(),
      setOffset: vi.fn(),
      setContent: vi.fn(),
      setTitle: vi.fn(),
      setZIndex: vi.fn(),
      show: vi.fn(),
      hide: vi.fn(),
      destroy: vi.fn(),
    }));
    mapMocks.createAccuracyCircle.mockReturnValue({ destroy: vi.fn() });
  });

  it("applies light and dark map themes in English mode", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    useLocaleStore().setLocale("en-US");
    useThemeStore().setMode("dark");

    const wrapper = mount(MapView, { props: { data }, global: { plugins: [pinia] } });
    await flushPromises();

    expect(mapMocks.setTheme).toHaveBeenCalledWith(true);

    useThemeStore().setMode("light");
    await flushPromises();

    expect(mapMocks.setTheme).toHaveBeenCalledWith(false);
    wrapper.unmount();
  });

  it("shows coordinates immediately while reverse geocoding is pending", async () => {
    geocodeMock.mockImplementation(() => new Promise(() => undefined));
    const wrapper = mount(MapView, { props: { data }, global: { plugins: [createPinia()] } });
    await flushPromises();

    expect(wrapper.text()).toContain("Test Phone");
    expect(wrapper.text()).toContain("Location 30.1235, 120.5679");
    wrapper.unmount();
  });

  it("requests a new address when the device coordinates change", async () => {
    geocodeMock.mockResolvedValueOnce({ full: "First address", short: "First address" }).mockResolvedValueOnce({ full: "Second address", short: "Second address" });
    const wrapper = mount(MapView, { props: { data }, global: { plugins: [createPinia()] } });
    await flushPromises();
    expect(wrapper.text()).toContain("First address");

    await wrapper.setProps({ data: movedData(30.223456, 120.66789) });
    await flushPromises();

    expect(geocodeMock).toHaveBeenCalledTimes(2);
    expect(wrapper.text()).toContain("Second address");
    expect(wrapper.text()).not.toContain("First address");
    wrapper.unmount();
  });

  it("zooms with the mouse wheel over the AMap container", async () => {
    const wrapper = mount(MapView, { props: { data }, global: { plugins: [createPinia()] } });
    await flushPromises();

    const container = mapMocks.createMap.mock.calls[0]?.[0] as HTMLDivElement;
    const wheel = new WheelEvent("wheel", { deltaY: -100, bubbles: true, cancelable: true });
    container.dispatchEvent(wheel);

    expect(wheel.defaultPrevented).toBe(true);
    expect(mapMocks.setZoom).toHaveBeenCalledWith(12);
    wrapper.unmount();
  });

  it("zooms with a precision-touchpad pinch gesture", async () => {
    const wrapper = mount(MapView, { props: { data }, global: { plugins: [createPinia()] } });
    await flushPromises();

    const container = mapMocks.createMap.mock.calls[0]?.[0] as HTMLDivElement;
    const wheel = new WheelEvent("wheel", { deltaY: -50, bubbles: true, cancelable: true });
    Object.defineProperty(wheel, "ctrlKey", { value: true });
    container.dispatchEvent(wheel);

    expect(wheel.defaultPrevented).toBe(true);
    expect(mapMocks.setZoom).toHaveBeenCalledWith(12);
    wrapper.unmount();
  });

  it("pans with unmodified two-finger touchpad scrolling", async () => {
    const wrapper = mount(MapView, { props: { data }, global: { plugins: [createPinia()] } });
    await flushPromises();

    const container = mapMocks.createMap.mock.calls[0]?.[0] as HTMLDivElement;
    const first = new WheelEvent("wheel", { deltaX: 8, deltaY: 20, bubbles: true, cancelable: true });
    const fast = new WheelEvent("wheel", { deltaX: 20, deltaY: 140, bubbles: true, cancelable: true });
    container.dispatchEvent(first);
    container.dispatchEvent(fast);

    expect(first.defaultPrevented).toBe(true);
    expect(fast.defaultPrevented).toBe(true);
    expect(mapMocks.panBy).toHaveBeenCalledWith(-8, -20);
    expect(mapMocks.panBy).toHaveBeenCalledWith(-20, -140);
    expect(mapMocks.setZoom).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
