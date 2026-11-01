import { StorageKey } from "@/constants";
import { readStored } from "@/lib/common/storage";

import { AmapMapAdapter } from "./amap-adapter";
import { GoogleMapsAdapter } from "./google-adapter";
import { DEFAULT_MAP_PROVIDER_ID, getMapProvider } from "./providers";
import type { MapAdapter } from "./types";

const adapters = new Map<string, MapAdapter>();
const adapterFactories: Record<string, () => MapAdapter> = {
  amap: () => new AmapMapAdapter(),
  google: () => new GoogleMapsAdapter(),
};

export function createMapAdapter(providerId?: string | null): MapAdapter {
  const selectedId = providerId ?? readStored(StorageKey.mapProvider) ?? DEFAULT_MAP_PROVIDER_ID;
  const normalizedId = getMapProvider(selectedId).id;
  const cached = adapters.get(normalizedId);
  if (cached) return cached;

  const create = adapterFactories[normalizedId];
  if (!create) throw new Error(`Map adapter is not registered for provider: ${normalizedId}`);
  const adapter = create();
  adapters.set(normalizedId, adapter);
  return adapter;
}

export type { AddressInfo, LngLat, MapAdapter, MapInstance, MapMarker, MapOverlay, MapPadding } from "./types";
