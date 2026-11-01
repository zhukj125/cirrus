import { loadAmap } from "@/lib/amap";

import type { AddressInfo, LngLat, MapLanguage } from "./types";

interface RegeoAddressComponent {
  province?: unknown;
  city?: unknown;
  district?: unknown;
  township?: unknown;
  streetNumber?: { street?: unknown; number?: unknown } | null;
}

interface AmapGeocoderInstance {
  getAddress(lnglat: LngLat, callback: (status: string, result: unknown) => void): void;
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

function buildAddressInfo(formattedAddress: unknown, addressComponent: RegeoAddressComponent | undefined, language: MapLanguage): AddressInfo | null {
  const ac = addressComponent ?? {};
  const streetNumber = ac.streetNumber ?? {};
  const street = text(streetNumber.street);
  const number = text(streetNumber.number);
  const city = text(ac.city) || text(ac.province);
  const separator = language === "en" ? ", " : "，";
  const short = [street || text(ac.township) || text(ac.district), city].filter(Boolean).join(separator);
  const fallback = [text(ac.province), text(ac.city), text(ac.district), text(ac.township), street, number].filter(Boolean).join(separator);
  const full = text(formattedAddress).replace(/^中国/, "") || fallback;
  return full ? { full, short } : null;
}

function buildJsAddress(result: unknown, language: MapLanguage): AddressInfo | null {
  const regeocode = (result as AmapJsGeocodeResult | null)?.regeocode;
  if (!regeocode) return null;
  return buildAddressInfo(regeocode.formattedAddress ?? regeocode.formatted_address, regeocode.addressComponent, language);
}

async function geocoderConstructor(): Promise<AmapGeocoderConstructor> {
  const namespace = (await loadAmap()) as AmapGeocoderNamespace;
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

export async function reverseGeocodeWithAmap(position: LngLat, language: MapLanguage): Promise<AddressInfo | null> {
  try {
    const Geocoder = await geocoderConstructor();
    return await new Promise((resolve) => {
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
        geocoder.getAddress(position, (status, result) => {
          finish(status === "complete" ? buildJsAddress(result, language) : null);
        });
      } catch {
        finish(null);
      }
    });
  } catch (error) {
    console.warn("[GEO] AMap JS reverse geocoding failed", error);
    return null;
  }
}
