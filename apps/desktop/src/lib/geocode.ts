import { getAmapConfig } from "./amap";

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

interface RegeoResponse {
  status?: string;
  info?: string;
  infocode?: string;
  regeocode?: {
    formatted_address?: string;
    addressComponent?: RegeoAddressComponent;
  };
}

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function buildAddress(result: RegeoResponse, language: "zh" | "en"): AddressInfo | null {
  const regeocode = result.regeocode;
  if (result.status !== "1" || !regeocode) return null;

  const ac = regeocode.addressComponent ?? {};
  const streetNumber = ac.streetNumber ?? {};
  const street = text(streetNumber.street);
  const number = text(streetNumber.number);
  const city = text(ac.city) || text(ac.province);
  const separator = language === "en" ? ", " : "，";
  const short = [street || text(ac.township) || text(ac.district), city].filter(Boolean).join(separator);
  const fallback = [text(ac.province), text(ac.city), text(ac.district), text(ac.township), street, number].filter(Boolean).join(separator);
  const full = text(regeocode.formatted_address).replace(/^\u4e2d\u56fd/, "") || fallback;
  return full ? { full, short } : null;
}

const cache = new Map<string, Promise<AddressInfo | null>>();

export function reverseGeocode(lnglat: [number, number], language: "zh" | "en" = "zh"): Promise<AddressInfo | null> {
  const [lng, lat] = lnglat;
  const key = `${language}:${lng.toFixed(5)},${lat.toFixed(5)}`;
  const cached = cache.get(key);
  if (cached) return cached;
  const request = fetchAddress(lnglat, language);
  cache.set(key, request);
  return request;
}

async function fetchAddress(lnglat: [number, number], language: "zh" | "en"): Promise<AddressInfo | null> {
  const { key } = getAmapConfig();
  if (!key) return null;
  const [lng, lat] = lnglat;
  const url = `https://restapi.amap.com/v3/geocode/regeo?key=${encodeURIComponent(key)}&location=${lng},${lat}&extensions=base&output=json&lang=${language}&langCode=${language === "en" ? "en" : "zh-Hans"}`;
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(12_000) });
    if (!response.ok) return null;
    const result = (await response.json()) as RegeoResponse;
    if (result.status !== "1") {
      console.warn("[GEO] AMap reverse geocoding failed", { info: result.info, infocode: result.infocode });
      return null;
    }
    return buildAddress(result, language);
  } catch {
    return null;
  }
}
