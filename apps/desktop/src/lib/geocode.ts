import { createMapAdapter } from "@/lib/maps";
import type { AddressInfo, LngLat, MapLanguage } from "@/lib/maps/types";

export type { AddressInfo } from "@/lib/maps/types";

const cache = new Map<string, Promise<AddressInfo | null>>();

export function reverseGeocode(position: LngLat, language: MapLanguage = "zh"): Promise<AddressInfo | null> {
  const [longitude, latitude] = position;
  const adapter = createMapAdapter();
  const key = `${adapter.providerId}:${language}:${longitude.toFixed(5)},${latitude.toFixed(5)}`;
  const cached = cache.get(key);
  if (cached) return cached;

  const request = adapter.reverseGeocode(position, language).then(
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
