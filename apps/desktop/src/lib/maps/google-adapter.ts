import { loadGoogleMaps, type GoogleGeocodeResult, type GoogleLatLng, type GoogleMapInstance, type GoogleMapsEventListener, type GoogleMapsNamespace, type GoogleOverlayView } from "./google-maps";
import type { AccuracyCircleOptions, AddressInfo, FitMarkersOptions, LngLat, MapAdapter, MapCreateOptions, MapInstance, MapLanguage, MapMarker, MapOverlay } from "./types";

const GOOGLE_DARK_STYLES: Array<Record<string, unknown>> = [
  { elementType: "geometry", stylers: [{ color: "#212121" }] },
  { elementType: "labels.icon", stylers: [{ visibility: "off" }] },
  { elementType: "labels.text.fill", stylers: [{ color: "#8a8a8a" }] },
  { elementType: "labels.text.stroke", stylers: [{ color: "#212121" }] },
  { featureType: "administrative", elementType: "geometry", stylers: [{ color: "#757575" }] },
  { featureType: "poi", elementType: "geometry", stylers: [{ color: "#181818" }] },
  { featureType: "poi.park", elementType: "geometry", stylers: [{ color: "#263c2b" }] },
  { featureType: "road", elementType: "geometry", stylers: [{ color: "#38414e" }] },
  { featureType: "road", elementType: "geometry.stroke", stylers: [{ color: "#212a37" }] },
  { featureType: "road.highway", elementType: "geometry", stylers: [{ color: "#746855" }] },
  { featureType: "transit", elementType: "geometry", stylers: [{ color: "#2f3948" }] },
  { featureType: "water", elementType: "geometry", stylers: [{ color: "#17263c" }] },
];

const GOOGLE_GEOCODE_TIMEOUT_MS = 8_000;

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function component(result: GoogleGeocodeResult, ...types: string[]): string {
  for (const type of types) {
    const match = result.address_components?.find((item) => item.types?.includes(type));
    const value = text(match?.long_name);
    if (value) return value;
  }
  return "";
}

function buildAddressInfo(result: GoogleGeocodeResult | undefined, language: MapLanguage): AddressInfo | null {
  if (!result) return null;
  const route = component(result, "route");
  const number = component(result, "street_number");
  const street = language === "en" ? [number, route].filter(Boolean).join(" ") : [route, number].filter(Boolean).join("");
  const area = component(result, "sublocality", "sublocality_level_1", "neighborhood", "postal_town");
  const city = component(result, "locality", "administrative_area_level_2", "administrative_area_level_1");
  const separator = language === "en" ? ", " : "，";
  const short = [street || area || city, city].filter(Boolean).join(separator);
  const full = text(result.formatted_address) || [component(result, "administrative_area_level_1"), city, area, street].filter(Boolean).join(separator);
  return full ? { full, short } : null;
}

async function reverseGeocodeWithGoogle(position: LngLat, language: MapLanguage): Promise<AddressInfo | null> {
  try {
    const namespace = await loadGoogleMaps();
    return await new Promise((resolve) => {
      let settled = false;
      const finish = (info: AddressInfo | null) => {
        if (settled) return;
        settled = true;
        window.clearTimeout(timer);
        resolve(info);
      };
      const timer = window.setTimeout(() => finish(null), GOOGLE_GEOCODE_TIMEOUT_MS);
      try {
        const geocoder = new namespace.Geocoder();
        geocoder.geocode(
          {
            location: { lat: position[1], lng: position[0] },
            language: language === "en" ? "en" : "zh-CN",
          },
          (results, status) => finish(status === "OK" ? buildAddressInfo(results?.[0], language) : null),
        );
      } catch {
        finish(null);
      }
    });
  } catch (error) {
    console.warn("[GEO] Google Maps reverse geocoding failed", error);
    return null;
  }
}

class GoogleMarkerHandle implements MapMarker {
  private readonly root = document.createElement("div");
  private readonly overlay: GoogleOverlayView;
  private position: GoogleLatLng;
  private offset: [number, number] = [0, 0];

  constructor(
    private readonly namespace: GoogleMapsNamespace,
    map: GoogleMapInstance,
    position: LngLat,
    onClick: () => void,
  ) {
    this.position = new namespace.LatLng(position[1], position[0]);
    this.root.style.position = "absolute";
    this.root.style.pointerEvents = "auto";
    this.root.style.zIndex = "120";
    this.root.addEventListener("click", (event) => {
      event.stopPropagation();
      onClick();
    });

    this.overlay = new namespace.OverlayView();
    this.overlay.onAdd = () => this.overlay.getPanes()?.overlayMouseTarget.appendChild(this.root);
    this.overlay.draw = () => this.draw();
    this.overlay.onRemove = () => this.root.remove();
    this.overlay.setMap(map);
  }

  private draw(): void {
    const pixel = this.overlay.getProjection()?.fromLatLngToDivPixel(this.position);
    if (!pixel) return;
    this.root.style.left = `${pixel.x + this.offset[0]}px`;
    this.root.style.top = `${pixel.y + this.offset[1]}px`;
  }

  getPosition(): LngLat {
    return [this.position.lng(), this.position.lat()];
  }

  setPosition([longitude, latitude]: LngLat): void {
    this.position = new this.namespace.LatLng(latitude, longitude);
    this.draw();
  }

  setOffset([x, y]: [number, number]): void {
    this.offset = [x, y];
    this.draw();
  }

  setContent(content: HTMLElement): void {
    this.root.replaceChildren(content);
  }

  setTitle(title: string): void {
    this.root.title = title;
  }

  setZIndex(index: number): void {
    this.root.style.zIndex = String(index);
  }

  show(): void {
    this.root.style.display = "";
  }

  hide(): void {
    this.root.style.display = "none";
  }

  destroy(): void {
    this.overlay.setMap(null);
    this.root.remove();
  }
}

class GoogleCircleHandle implements MapOverlay {
  constructor(private readonly circle: { setMap(map: GoogleMapInstance | null): void }) {}

  destroy(): void {
    this.circle.setMap(null);
  }
}

class GoogleMapController implements MapInstance {
  constructor(
    private readonly namespace: GoogleMapsNamespace,
    private readonly map: GoogleMapInstance,
    private readonly container: HTMLElement,
    private readonly listeners: GoogleMapsEventListener[],
  ) {}

  getRotation(): number {
    return 0;
  }

  setRotation(): void {}

  getZoom(): number {
    return this.map.getZoom();
  }

  setZoom(zoom: number): void {
    this.map.setZoom(zoom);
  }

  setZoomAndCenter(zoom: number, [longitude, latitude]: LngLat): void {
    this.map.setZoom(zoom);
    this.map.setCenter(new this.namespace.LatLng(latitude, longitude));
  }

  panBy(x: number, y: number): void {
    this.map.panBy(x, y);
  }

  fitMarkers(markers: MapMarker[], options: FitMarkersOptions): void {
    const values = markers as GoogleMarkerHandle[];
    if (values.length === 0) return;
    const [top, right, bottom, left] = options.padding;
    const target = new this.namespace.LatLngBounds();
    for (const marker of values) {
      const [longitude, latitude] = marker.getPosition();
      target.extend(new this.namespace.LatLng(latitude, longitude));
    }
    this.map.fitBounds(target, { padding: { top, right, bottom, left }, maxZoom: options.maxZoom ?? 19 });
  }

  zoomIn(): void {
    this.map.setZoom(this.map.getZoom() + 1);
  }

  zoomOut(): void {
    this.map.setZoom(this.map.getZoom() - 1);
  }

  setTheme(dark: boolean): void {
    this.map.setOptions({ styles: dark ? GOOGLE_DARK_STYLES : [] });
  }

  destroy(): void {
    for (const listener of this.listeners) listener.remove();
    this.container.replaceChildren();
  }
}

export class GoogleMapsAdapter implements MapAdapter {
  readonly providerId = "google";
  readonly capabilities = { rotation: false };

  private namespace: GoogleMapsNamespace | null = null;
  private activeMap: GoogleMapInstance | null = null;

  async load(): Promise<void> {
    this.namespace = await loadGoogleMaps();
  }

  project(position: LngLat): LngLat {
    return position;
  }

  createMap(container: HTMLElement, options: MapCreateOptions): MapInstance {
    const namespace = this.namespace;
    if (!namespace) throw new Error("Google Maps is not loaded");
    const map = new namespace.Map(container, {
      center: new namespace.LatLng(options.center[1], options.center[0]),
      zoom: options.zoom,
      minZoom: options.minZoom,
      maxZoom: options.maxZoom,
      disableDefaultUI: true,
      clickableIcons: false,
      gestureHandling: "greedy",
      keyboardShortcuts: false,
      styles: options.dark ? GOOGLE_DARK_STYLES : [],
    });
    this.activeMap = map;
    const listeners = [namespace.event.addListener(map, "zoom_changed", options.callbacks.onZoomChange), namespace.event.addListener(map, "idle", options.callbacks.onZoomEnd), namespace.event.addListener(map, "click", options.callbacks.onClick)];
    return new GoogleMapController(namespace, map, container, listeners);
  }

  createMarker(position: LngLat, onClick: () => void): MapMarker {
    const map = this.activeMap;
    const namespace = this.namespace;
    if (!map || !namespace) throw new Error("Google Maps map is not initialized");
    return new GoogleMarkerHandle(namespace, map, position, onClick);
  }

  createAccuracyCircle(options: AccuracyCircleOptions): MapOverlay {
    const map = this.activeMap;
    const namespace = this.namespace;
    if (!map || !namespace) throw new Error("Google Maps map is not initialized");
    const circle = new namespace.Circle({
      map,
      center: new namespace.LatLng(options.center[1], options.center[0]),
      radius: options.radius,
      fillColor: options.fillColor,
      fillOpacity: options.fillOpacity,
      strokeColor: options.strokeColor,
      strokeOpacity: options.strokeOpacity,
      strokeWeight: options.strokeWeight,
      zIndex: options.zIndex,
    });
    return new GoogleCircleHandle(circle);
  }

  reverseGeocode(position: LngLat, language: MapLanguage): Promise<AddressInfo | null> {
    return reverseGeocodeWithGoogle(position, language);
  }

  mapUrl([longitude, latitude]: LngLat): string {
    return `https://www.google.com/maps/search/?api=1&query=${latitude},${longitude}`;
  }
}
