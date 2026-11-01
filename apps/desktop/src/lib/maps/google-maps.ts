import { StorageKey } from "@/constants";
import { readStored } from "@/lib/common/storage";

const ENV_GOOGLE_MAPS_API_KEY = String(import.meta.env.VITE_GOOGLE_MAPS_API_KEY ?? "").trim();
const GOOGLE_MAPS_CALLBACK = "__cirrusGoogleMapsReady";
const GOOGLE_MAPS_LOAD_TIMEOUT_MS = 20_000;

export interface GoogleMapsConfig {
  apiKey: string;
}

export interface GoogleLatLng {
  lat(): number;
  lng(): number;
}

export interface GooglePoint {
  x: number;
  y: number;
}

export interface GoogleMapInstance {
  setOptions(options: Record<string, unknown>): void;
  getZoom(): number;
  setZoom(zoom: number): void;
  setCenter(center: GoogleLatLng): void;
  panBy(x: number, y: number): void;
  fitBounds(bounds: GoogleLatLngBounds, options?: Record<string, unknown>): void;
}

export interface GoogleLatLngBounds {
  extend(position: GoogleLatLng): void;
}

export interface GoogleMapsEventListener {
  remove(): void;
}

export interface GoogleOverlayView {
  setMap(map: GoogleMapInstance | null): void;
  getPanes(): { overlayMouseTarget: HTMLElement } | null;
  getProjection(): { fromLatLngToDivPixel(position: GoogleLatLng): GooglePoint | null } | null;
  onAdd?: () => void;
  draw?: () => void;
  onRemove?: () => void;
}

export interface GoogleGeocodeAddressComponent {
  long_name?: string;
  types?: string[];
}

export interface GoogleGeocodeResult {
  formatted_address?: string;
  address_components?: GoogleGeocodeAddressComponent[];
}

export interface GoogleGeocoder {
  geocode(request: { location: { lat: number; lng: number }; language: string }, callback: (results: GoogleGeocodeResult[] | null, status: string) => void): void;
}

export interface GoogleCircleInstance {
  setMap(map: GoogleMapInstance | null): void;
}

export interface GoogleMapsNamespace {
  Map: new (container: HTMLElement, options: Record<string, unknown>) => GoogleMapInstance;
  LatLng: new (latitude: number, longitude: number) => GoogleLatLng;
  LatLngBounds: new () => GoogleLatLngBounds;
  OverlayView: new () => GoogleOverlayView;
  Circle: new (options: Record<string, unknown>) => GoogleCircleInstance;
  Geocoder: new () => GoogleGeocoder;
  event: {
    addListener(instance: unknown, eventName: string, handler: () => void): GoogleMapsEventListener;
  };
}

declare global {
  interface Window {
    google?: { maps: GoogleMapsNamespace };
    __cirrusGoogleMapsReady?: () => void;
  }
}

export function getGoogleMapsConfig(): GoogleMapsConfig {
  const localKey = (readStored(StorageKey.googleMapsApiKey) ?? "").trim();
  return { apiKey: localKey || ENV_GOOGLE_MAPS_API_KEY };
}

export function hasEnvironmentGoogleMapsKey(): boolean {
  return ENV_GOOGLE_MAPS_API_KEY.length > 0;
}

let googleMapsPromise: Promise<GoogleMapsNamespace> | null = null;
let googleMapsPromiseSignature: string | null = null;
let loadedSignature: string | null = null;
let googleMapsScript: HTMLScriptElement | null = null;

function resetGoogleMaps(): void {
  googleMapsScript?.remove();
  googleMapsScript = null;
  googleMapsPromise = null;
  googleMapsPromiseSignature = null;
  loadedSignature = null;
  if (typeof window !== "undefined") {
    window.__cirrusGoogleMapsReady = undefined;
    try {
      delete window.google;
    } catch {}
  }
}

export function loadGoogleMaps(config: GoogleMapsConfig = getGoogleMapsConfig()): Promise<GoogleMapsNamespace> {
  const apiKey = config.apiKey.trim();
  if (!apiKey) {
    resetGoogleMaps();
    return Promise.reject(new Error("Google Maps API key is not configured"));
  }

  if (window.google?.maps && loadedSignature === apiKey) return Promise.resolve(window.google.maps);
  if (loadedSignature !== apiKey) resetGoogleMaps();
  if (googleMapsPromise && googleMapsPromiseSignature === apiKey) return googleMapsPromise;

  googleMapsPromiseSignature = apiKey;
  googleMapsPromise = new Promise((resolve, reject) => {
    let settled = false;
    const finish = (namespace?: GoogleMapsNamespace, error?: Error) => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timer);
      if (namespace) {
        loadedSignature = apiKey;
        resolve(namespace);
        return;
      }
      resetGoogleMaps();
      reject(error ?? new Error("Could not load Google Maps. Check your network connection and API key."));
    };

    const timer = window.setTimeout(() => finish(undefined, new Error("Google Maps loading timed out")), GOOGLE_MAPS_LOAD_TIMEOUT_MS);
    window.__cirrusGoogleMapsReady = () => finish(window.google?.maps);

    const script = document.createElement("script");
    googleMapsScript = script;
    script.async = true;
    script.src = `https://maps.googleapis.com/maps/api/js?key=${encodeURIComponent(apiKey)}&v=weekly&loading=async&callback=${GOOGLE_MAPS_CALLBACK}`;
    script.onerror = () => finish(undefined, new Error("Could not load Google Maps. Check your network connection and API key."));
    script.onload = () => {
      if (window.google?.maps) finish(window.google.maps);
    };
    document.head.appendChild(script);
  });

  return googleMapsPromise;
}
