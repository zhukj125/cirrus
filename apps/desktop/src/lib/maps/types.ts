export type LngLat = [longitude: number, latitude: number];

export type MapLanguage = "zh" | "en";

export type MapPadding = [top: number, right: number, bottom: number, left: number];

export interface AddressInfo {
  full: string;
  short: string;
}

export interface MapCapabilities {
  rotation: boolean;
}

export interface MapCreateCallbacks {
  onRotate: () => void;
  onZoomChange: () => void;
  onZoomEnd: () => void;
  onClick: () => void;
}

export interface MapCreateOptions {
  center: LngLat;
  zoom: number;
  minZoom: number;
  maxZoom: number;
  dark: boolean;
  callbacks: MapCreateCallbacks;
}

export interface FitMarkersOptions {
  padding: MapPadding;
  maxZoom?: number;
}

export interface AccuracyCircleOptions {
  center: LngLat;
  radius: number;
  fillColor: string;
  fillOpacity: number;
  strokeColor: string;
  strokeOpacity: number;
  strokeWeight: number;
  zIndex: number;
}

export interface MapMarker {
  setPosition(position: LngLat): void;
  setOffset(offset: [x: number, y: number]): void;
  setContent(content: HTMLElement): void;
  setTitle(title: string): void;
  setZIndex(index: number): void;
  show(): void;
  hide(): void;
  destroy(): void;
}

export interface MapOverlay {
  destroy(): void;
}

export interface MapInstance {
  getRotation(): number;
  setRotation(rotation: number, immediately?: boolean): void;
  getZoom(): number;
  setZoom(zoom: number): void;
  setZoomAndCenter(zoom: number, center: LngLat): void;
  /** Pan by a content delta: positive x/y moves the visible map content right/down. */
  panBy(x: number, y: number): void;
  fitMarkers(markers: MapMarker[], options: FitMarkersOptions): void;
  zoomIn(): void;
  zoomOut(): void;
  setTheme(dark: boolean): void;
  destroy(): void;
}

export interface MapAdapter {
  readonly providerId: string;
  readonly capabilities: MapCapabilities;
  load(): Promise<void>;
  project(position: LngLat): LngLat;
  createMap(container: HTMLElement, options: MapCreateOptions): MapInstance;
  createMarker(position: LngLat, onClick: () => void): MapMarker;
  createAccuracyCircle(options: AccuracyCircleOptions): MapOverlay;
  reverseGeocode(position: LngLat, language: MapLanguage): Promise<AddressInfo | null>;
  mapUrl(position: LngLat, title: string): string;
}
