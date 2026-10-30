import { getAmapConfig, loadAmap } from "./amap";
import { reverseGeocodeBackend } from "./api";
import { gcj02ToWgs84 } from "./geo";

export interface AddressInfo {
  full: string;
  short: string;
}

interface RegeoAddressComponent {
  province?: unknown;
  city?: unknown;
  district?: unknown;
  township?: unknown;
  streetNumber?: { street?: unknown; number?: unknown } | null;
}

interface AmapGeocoderInstance {
  getAddress(lnglat: [number, number], callback: (status: string, result: unknown) => void): void;
}

type AmapGeocoderConstructor = new (options: Record<string, unknown>) => AmapGeocoderInstance;

interface AmapGeocoderNamespace {
  Geocoder?: AmapGeocoderConstructor;
  plugin?: (plugins: string, callback: () => void) => void;
}

interface AmapJsRegeocode {
  formattedAddress?: unknown;
  formatted_address?: unknown;
  addressComponent?: RegeoAddressComponent;
}

interface AmapJsGeocodeResult {
  regeocode?: AmapJsRegeocode;
}

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function buildAddressInfo(formattedAddress: unknown, addressComponent: RegeoAddressComponent | undefined, language: "zh" | "en"): AddressInfo | null {
  const ac = addressComponent ?? {};
  const streetNumber = ac.streetNumber ?? {};
  const street = text(streetNumber.street);
  const number = text(streetNumber.number);
  const city = text(ac.city) || text(ac.province);
  const separator = language === "en" ? ", " : "，";
  const short = [street || text(ac.township) || text(ac.district), city].filter(Boolean).join(separator);
  const fallback = [text(ac.province), text(ac.city), text(ac.district), text(ac.township), street, number].filter(Boolean).join(separator);
  const full = text(formattedAddress).replace(/^\u4e2d\u56fd/, "") || fallback;
  return full ? { full, short } : null;
}

function buildJsAddress(result: unknown, language: "zh" | "en"): AddressInfo | null {
  const regeocode = (result as AmapJsGeocodeResult | null)?.regeocode;
  if (!regeocode) return null;
  return buildAddressInfo(regeocode.formattedAddress ?? regeocode.formatted_address, regeocode.addressComponent, language);
}

async function geocoderConstructor(): Promise<AmapGeocoderConstructor> {
  const amap = loadAmap() as Promise<AmapGeocoderNamespace>;
  const namespace = await amap;
  if (namespace.Geocoder) return namespace.Geocoder;
  if (!namespace.plugin) throw new Error("AMap Geocoder plugin is unavailable");

  await new Promise<void>((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error("AMap Geocoder plugin timed out")), 12_000);
    try {
      namespace.plugin?.("AMap.Geocoder", () => {
        window.clearTimeout(timer);
        if (namespace.Geocoder) resolve();
        else reject(new Error("AMap Geocoder plugin did not initialize"));
      });
    } catch (error) {
      window.clearTimeout(timer);
      reject(error);
    }
  });
  const loaded = namespace.Geocoder;
  if (!loaded) throw new Error("AMap Geocoder plugin did not initialize");
  return loaded;
}

const AMAP_JS_TIMEOUT_MS = 8_000;

async function fetchAddressWithAmap(lnglat: [number, number], language: "zh" | "en"): Promise<AddressInfo | null> {
  const Geocoder = await geocoderConstructor();
  return new Promise((resolve) => {
    let settled = false;
    const finish = (info: AddressInfo | null) => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timer);
      resolve(info);
    };
    const timer = window.setTimeout(() => finish(null), AMAP_JS_TIMEOUT_MS);
    try {
      const geocoder = new Geocoder({
        city: "全国",
        lang: language === "en" ? "en" : "zh_cn",
      });
      geocoder.getAddress(lnglat, (status, result) => {
        finish(status === "complete" ? buildJsAddress(result, language) : null);
      });
    } catch {
      finish(null);
    }
  });
}

async function fetchAddressWithBackend(lnglat: [number, number], language: "zh" | "en"): Promise<AddressInfo | null> {
  const [lng, lat] = lnglat;
  const [wgs84Lng, wgs84Lat] = gcj02ToWgs84(lng, lat);
  const { key } = getAmapConfig();
  return reverseGeocodeBackend({
    longitude: lng,
    latitude: lat,
    wgs84Longitude: wgs84Lng,
    wgs84Latitude: wgs84Lat,
    language,
    amapKey: key || null,
  });
}

const cache = new Map<string, Promise<AddressInfo | null>>();

export function reverseGeocode(lnglat: [number, number], language: "zh" | "en" = "zh"): Promise<AddressInfo | null> {
  const [lng, lat] = lnglat;
  const key = `${language}:${lng.toFixed(5)},${lat.toFixed(5)}`;
  const cached = cache.get(key);
  if (cached) return cached;
  const request = fetchAddress(lnglat, language).then(
    (info) => {
      if (!info) cache.delete(key);
      return info;
    },
    (error) => {
      cache.delete(key);
      throw error;
    },
  );
  cache.set(key, request);
  return request;
}

async function fetchAddress(lnglat: [number, number], language: "zh" | "en"): Promise<AddressInfo | null> {
  try {
    const info = await fetchAddressWithBackend(lnglat, language);
    if (info) return info;
  } catch (error) {
    console.warn("[GEO] backend reverse geocoding failed", error);
  }

  try {
    return await fetchAddressWithAmap(lnglat, language);
  } catch (error) {
    console.warn("[GEO] AMap JS reverse geocoding failed", error);
    return null;
  }
}
