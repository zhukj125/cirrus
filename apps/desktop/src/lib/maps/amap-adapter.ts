import { loadAmap } from "@/lib/amap";
import { wgs84ToGcj02 } from "@/lib/geo";

import { reverseGeocodeWithAmap } from "./amap-geocode";
import type { AccuracyCircleOptions, FitMarkersOptions, LngLat, MapAdapter, MapCreateOptions, MapInstance, MapMarker, MapOverlay } from "./types";

interface AmapNativeMarker {
  on(event: "click", handler: () => void): void;
  show(): void;
  hide(): void;
  setPosition(position: LngLat): void;
  setOffset(offset: unknown): void;
  setContent(content: HTMLElement): void;
  setTitle(title: string): void;
  setzIndex?(index: number): void;
}

interface AmapNativeMap {
  on(event: string, handler: () => void): void;
  add(target: unknown): void;
  remove(target: unknown): void;
  getRotation(): number;
  setRotation(rotation: number, immediately?: boolean): void;
  getZoom(): number;
  setZoom(zoom: number): void;
  setZoomAndCenter(zoom: number, center: LngLat): void;
  panBy(x: number, y: number): void;
  setFitView(targets: unknown[], immediately?: boolean, margins?: number[], maxZoom?: number): void;
  zoomIn(): void;
  zoomOut(): void;
  setMapStyle?(style: string): void;
  destroy(): void;
}

interface AmapNamespace {
  Map: new (container: HTMLElement, options: Record<string, unknown>) => AmapNativeMap;
  Pixel: new (x: number, y: number) => unknown;
  Marker: new (options: { position: LngLat; offset: unknown; title: string }) => AmapNativeMarker;
  Circle: new (options: Record<string, unknown>) => unknown;
}

function validLngLat([longitude, latitude]: LngLat): boolean {
  return Number.isFinite(longitude) && Number.isFinite(latitude) && longitude >= -180 && longitude <= 180 && latitude >= -90 && latitude <= 90;
}

function assertLngLat(position: LngLat, label: string): void {
  if (!validLngLat(position)) throw new Error(`${label} coordinates are invalid`);
}

class AmapMarkerHandle implements MapMarker {
  constructor(
    private readonly map: AmapNativeMap,
    private readonly namespace: AmapNamespace,
    readonly native: AmapNativeMarker,
  ) {}

  setPosition(position: LngLat): void {
    if (!validLngLat(position)) return;
    this.native.setPosition(position);
  }

  setOffset([x, y]: [number, number]): void {
    if (!Number.isFinite(x) || !Number.isFinite(y)) return;
    this.native.setOffset(new this.namespace.Pixel(x, y));
  }

  setContent(content: HTMLElement): void {
    this.native.setContent(content);
  }

  setTitle(title: string): void {
    this.native.setTitle(title);
  }

  setZIndex(index: number): void {
    this.native.setzIndex?.(index);
  }

  show(): void {
    this.native.show();
  }

  hide(): void {
    this.native.hide();
  }

  destroy(): void {
    this.map.remove(this.native);
  }
}

class AmapOverlayHandle implements MapOverlay {
  constructor(
    private readonly map: AmapNativeMap,
    private readonly native: unknown,
  ) {}

  destroy(): void {
    this.map.remove(this.native);
  }
}

class AmapMapInstance implements MapInstance {
  constructor(private readonly map: AmapNativeMap) {}

  getRotation(): number {
    return this.map.getRotation();
  }

  setRotation(rotation: number, immediately = false): void {
    if (!Number.isFinite(rotation)) return;
    this.map.setRotation(rotation, immediately);
  }

  getZoom(): number {
    return this.map.getZoom();
  }

  setZoom(zoom: number): void {
    if (!Number.isFinite(zoom)) return;
    this.map.setZoom(zoom);
  }

  setZoomAndCenter(zoom: number, center: LngLat): void {
    if (!Number.isFinite(zoom)) return;
    assertLngLat(center, "AMap center");
    this.map.setZoomAndCenter(zoom, center);
  }

  panBy(x: number, y: number): void {
    if (!Number.isFinite(x) || !Number.isFinite(y)) return;
    this.map.panBy(x, y);
  }

  fitMarkers(markers: MapMarker[], options: FitMarkersOptions): void {
    if (markers.length === 0 || !options.padding.every(Number.isFinite) || !Number.isFinite(options.maxZoom ?? 18)) return;
    const nativeMarkers = markers.map((marker) => (marker as AmapMarkerHandle).native);
    this.map.setFitView(nativeMarkers, true, [...options.padding], options.maxZoom);
  }

  zoomIn(): void {
    this.map.zoomIn();
  }

  zoomOut(): void {
    this.map.zoomOut();
  }

  setTheme(dark: boolean): void {
    this.map.setMapStyle?.(dark ? "amap://styles/dark" : "amap://styles/normal");
  }

  destroy(): void {
    this.map.destroy();
  }
}

export class AmapMapAdapter implements MapAdapter {
  readonly providerId = "amap";
  readonly capabilities = { rotation: true };

  private namespace: AmapNamespace | null = null;
  private activeMap: AmapNativeMap | null = null;

  async load(): Promise<void> {
    this.namespace = (await loadAmap()) as AmapNamespace;
  }

  project([longitude, latitude]: LngLat): LngLat {
    assertLngLat([longitude, latitude], "AMap source");
    const projected = wgs84ToGcj02(longitude, latitude);
    assertLngLat(projected, "AMap projected");
    return projected;
  }

  createMap(container: HTMLElement, options: MapCreateOptions): MapInstance {
    const namespace = this.namespace;
    if (!namespace) throw new Error("AMap is not loaded");
    if (container.clientWidth <= 0 || container.clientHeight <= 0) {
      throw new Error("AMap container has no visible size");
    }
    assertLngLat(options.center, "AMap center");
    if (!Number.isFinite(options.zoom) || !Number.isFinite(options.minZoom) || !Number.isFinite(options.maxZoom)) {
      throw new Error("AMap zoom is invalid");
    }

    const map = new namespace.Map(container, {
      zoom: options.zoom,
      center: this.project(options.center),
      minZoom: options.minZoom,
      viewMode: "3D",
      pitch: 0,
      maxZoom: options.maxZoom,
      rotateEnable: true,
      mapStyle: options.dark ? "amap://styles/dark" : "amap://styles/normal",
    });
    this.activeMap = map;

    map.on("rotatechange", options.callbacks.onRotate);
    map.on("zoomchange", options.callbacks.onZoomChange);
    map.on("zoomend", options.callbacks.onZoomEnd);
    map.on("click", options.callbacks.onClick);
    return new AmapMapInstance(map);
  }

  createMarker(position: LngLat, onClick: () => void): MapMarker {
    const map = this.activeMap;
    const namespace = this.namespace;
    if (!map || !namespace) throw new Error("AMap map is not initialized");
    assertLngLat(position, "AMap marker");

    const native = new namespace.Marker({
      position,
      offset: new namespace.Pixel(0, 0),
      title: "",
    });
    native.on("click", onClick);
    map.add(native);
    return new AmapMarkerHandle(map, namespace, native);
  }

  createAccuracyCircle(options: AccuracyCircleOptions): MapOverlay {
    const map = this.activeMap;
    const namespace = this.namespace;
    if (!map || !namespace) throw new Error("AMap map is not initialized");
    assertLngLat(options.center, "AMap accuracy circle");
    if (!Number.isFinite(options.radius) || options.radius < 0) throw new Error("AMap accuracy radius is invalid");

    const native = new namespace.Circle({
      center: options.center,
      radius: options.radius,
      fillColor: options.fillColor,
      fillOpacity: options.fillOpacity,
      strokeColor: options.strokeColor,
      strokeWeight: options.strokeWeight,
      strokeOpacity: options.strokeOpacity,
      zIndex: options.zIndex,
      bubble: true,
    });
    map.add(native);
    return new AmapOverlayHandle(map, native);
  }

  reverseGeocode(position: LngLat, language: "zh" | "en") {
    return reverseGeocodeWithAmap(this.project(position), language);
  }

  mapUrl(position: LngLat, title: string): string {
    const [longitude, latitude] = this.project(position);
    return `https://uri.amap.com/marker?position=${longitude},${latitude}&name=${encodeURIComponent(title)}`;
  }
}
