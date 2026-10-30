import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";

vi.mock("@/lib/amap", () => ({
  loadAmap: vi.fn(async () => ({
    Map: class {
      on() {}
      add() {}
      remove() {}
      getRotation() {
        return 0;
      }
      setRotation() {}
      getZoom() {
        return 11;
      }
      setZoom() {}
      setZoomAndCenter() {}
      setFitView() {}
      zoomIn() {}
      zoomOut() {}
      destroy() {}
      setMapStyle() {}
    },
    Pixel: class {},
    TileLayer: class {},
    Marker: class {
      on() {}
      show() {}
      hide() {}
      setPosition() {}
      setOffset() {}
      setContent() {}
      setTitle() {}
      setzIndex() {}
    },
    Circle: class {},
  })),
}));

const geocodeMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/geocode", () => ({
  reverseGeocode: geocodeMock,
}));

import MapView from "@/views/MapView.vue";
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
});
