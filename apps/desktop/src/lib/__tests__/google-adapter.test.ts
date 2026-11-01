import { beforeEach, describe, expect, it, vi } from "vitest";

const googleMapsMock = vi.hoisted(() => ({
  load: vi.fn(),
  namespace: null as Record<string, unknown> | null,
  addListener: vi.fn(),
}));

vi.mock("@/lib/maps/google-maps", () => ({
  loadGoogleMaps: googleMapsMock.load,
  hasEnvironmentGoogleMapsKey: () => false,
}));

import { GoogleMapsAdapter } from "@/lib/maps/google-adapter";
import type { MapCreateOptions } from "@/lib/maps/types";

let overlayPane: HTMLDivElement;

class FakeLatLng {
  constructor(
    readonly latitude: number,
    readonly longitude: number,
  ) {}

  lat(): number {
    return this.latitude;
  }

  lng(): number {
    return this.longitude;
  }
}

class FakeBounds {
  extend = vi.fn();
}

class FakeMap {
  setOptions = vi.fn();
  getZoom = vi.fn(() => 11);
  setZoom = vi.fn();
  setCenter = vi.fn();
  fitBounds = vi.fn();
}

class FakeOverlay {
  onAdd?: () => void;
  draw?: () => void;
  onRemove?: () => void;
  setMap = vi.fn((map: unknown) => {
    if (map) {
      this.onAdd?.();
      this.draw?.();
    } else {
      this.onRemove?.();
    }
  });
  getPanes = vi.fn(() => ({ overlayMouseTarget: overlayPane }));
  getProjection = vi.fn(() => ({ fromLatLngToDivPixel: () => ({ x: 10, y: 20 }) }));
}

class FakeCircle {
  setMap = vi.fn();
}

describe("GoogleMapsAdapter", () => {
  beforeEach(() => {
    overlayPane = document.createElement("div");
    googleMapsMock.addListener.mockReset();
    googleMapsMock.addListener.mockImplementation(() => ({ remove: vi.fn() }));
    googleMapsMock.namespace = {
      Map: FakeMap,
      LatLng: FakeLatLng,
      LatLngBounds: FakeBounds,
      OverlayView: FakeOverlay,
      Circle: FakeCircle,
      Geocoder: class {},
      event: { addListener: googleMapsMock.addListener },
    };
    googleMapsMock.load.mockReset();
    googleMapsMock.load.mockResolvedValue(googleMapsMock.namespace);
  });

  it("renders markers, binds events, and cleans up through the unified interface", async () => {
    const adapter = new GoogleMapsAdapter();
    await adapter.load();
    const container = document.createElement("div");
    const options: MapCreateOptions = {
      center: [-122.4194, 37.7749],
      zoom: 11,
      minZoom: 3,
      maxZoom: 20,
      dark: false,
      callbacks: { onRotate: vi.fn(), onZoomChange: vi.fn(), onZoomEnd: vi.fn(), onClick: vi.fn() },
    };
    const map = adapter.createMap(container, options);
    expect(googleMapsMock.addListener).toHaveBeenCalledTimes(3);

    const onClick = vi.fn();
    const marker = adapter.createMarker([-122.4194, 37.7749], onClick);
    marker.show();
    marker.setContent(document.createElement("span"));
    marker.setOffset([-20, -30]);
    marker.setZIndex(140);
    marker.setTitle("Phone");

    const markerRoot = overlayPane.querySelector('div[title="Phone"]');
    expect(markerRoot).not.toBeNull();
    markerRoot?.dispatchEvent(new MouseEvent("click"));
    expect(onClick).toHaveBeenCalledOnce();

    map.fitMarkers([marker], { padding: [100, 80, 360, 80] });
    map.setTheme(true);
    const circle = adapter.createAccuracyCircle({
      center: [-122.4194, 37.7749],
      radius: 24,
      fillColor: "#34c759",
      fillOpacity: 0.1,
      strokeColor: "#34c759",
      strokeOpacity: 0.2,
      strokeWeight: 1,
      zIndex: 108,
    });

    marker.destroy();
    circle.destroy();
    map.destroy();
    expect(container.childElementCount).toBe(0);
  });
});
