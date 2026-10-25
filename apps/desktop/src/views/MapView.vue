<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { storeToRefs } from "pinia";
import { ArrowLeft, ChevronDown, ChevronUp, CircleAlert, Crosshair, Volume2, X } from "@lucide/vue";

import BatteryIcon from "@/components/BatteryIcon.vue";
import DeviceIcon, { deviceEmoji } from "@/components/DeviceIcon.vue";
import LockBadge, { deviceLockLabel, hasLockBadge } from "@/components/LockBadge.vue";
import RadarGlyph from "@/components/RadarGlyph.vue";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { loadAmap } from "@/lib/amap";
import { toast } from "@/lib/common/toast";
import { wgs84ToGcj02 } from "@/lib/geo";
import { reverseGeocode, type AddressInfo } from "@/lib/geocode";
import { relativeTimeLabel, staleTimeLabel as formatStaleTimeLabel } from "@/lib/time";
import { useAppStore } from "@/stores/app";
import { useLocaleStore } from "@/stores/locale";
import { useThemeStore } from "@/stores/theme";
import type { Device, DevicesResponse } from "@/types";

const REFRESH_INTERVAL = 60_000;

const DEVICE_FOCUS_MIN_ZOOM = 13;
const DEVICE_FOCUS_MAX_ZOOM = 19;

const DEVICE_PANEL_LEFT = 16;
const DEVICE_PANEL_WIDTH = 320;
const DEVICE_PANEL_GAP = 32;
const DEVICE_FOCUS_RIGHT_GUTTER = 32;
const DEVICE_FOCUS_TOP_GUTTER = 100;
const DEVICE_FOCUS_BOTTOM_GUTTER = 100;

const WORLD_METERS_AT_ZOOM_0 = 156_543.03392;

const CLUSTER_RADIUS_M = 100;

const MAP_STYLE_LIGHT = "amap://styles/normal";
const MAP_STYLE_DARK = "amap://styles/dark";

const MARKER_BASE_SIZE = 56;

const MARKER_CLICK_GRACE_MS = 300;

const FIT_VIEW_MARGINS = [100, 80, 360, 80];
const FIT_VIEW_MARGINS_COLLAPSED = [80, 80, 80, 80];

interface AmapMarker {
  on(event: "click", handler: () => void): void;
  show(): void;
  hide(): void;
  setPosition(position: [number, number]): void;
  setOffset(offset: unknown): void;
  setContent(content: HTMLElement): void;
  setTitle(title: string): void;
  setzIndex?(index: number): void;
}

interface AmapMapInstance {
  on(event: "rotatechange" | "click" | "zoomchange" | "zoomend", handler: () => void): void;
  add(target: unknown): void;
  remove(target: unknown): void;
  getRotation(): number;
  setRotation(rotation: number, immediately?: boolean): void;
  getZoom(): number;
  setZoom(zoom: number): void;
  setZoomAndCenter(zoom: number, center: [number, number]): void;
  setFitView(targets: unknown[], immediately?: boolean, margins?: number[], maxZoom?: number): void;
  zoomIn(): void;
  zoomOut(): void;
  destroy(): void;
  setMapStyle?(style: string): void;
}

interface AmapNamespace {
  Map: new (container: HTMLElement, options: Record<string, unknown>) => AmapMapInstance;
  Pixel: new (x: number, y: number) => unknown;
  Marker: new (options: { position: [number, number]; offset: unknown; title: string }) => AmapMarker;
  Circle: new (options: Record<string, unknown>) => unknown;
}

function deviceKey(device: Device): string {
  return device.udid ?? device.deviceDisplayName ?? device.name ?? "device";
}

function isDeviceOnline(device: Device): boolean {
  return device.location?.latitude != null && device.location.longitude != null;
}

function hasNoLocation(device: Device): boolean {
  return !isDeviceOnline(device);
}

function devicePosition(device: Device): [number, number] | null {
  const location = device.location;
  if (location?.longitude == null || location.latitude == null) return null;

  return wgs84ToGcj02(location.longitude, location.latitude);
}

function locationTimeMs(timestamp: number | null | undefined): number | null {
  if (timestamp == null || timestamp <= 0) return null;
  if (timestamp < 1e10) return timestamp * 1000 + 978307200000;
  if (timestamp < 1e12) return timestamp * 1000;
  return timestamp;
}

function timeAgo(timestamp: number | null | undefined): string {
  const ms = locationTimeMs(timestamp);
  return ms == null ? "" : relativeTimeLabel(ms, Date.now(), locale.isEnglish);
}

function staleTimeLabel(timestamp: number | null | undefined): string {
  const ms = locationTimeMs(timestamp);
  return ms == null ? "" : formatStaleTimeLabel(ms, new Date(), locale.isEnglish);
}

function isDeviceReportedOnline(device: Device): boolean {
  if (device.deviceStatus) return device.deviceStatus === "200";
  return isDeviceOnline(device);
}

function deviceRowContent(device: Device, addressShort: string | undefined): { address: string; status: string; stale: boolean } {
  const location = device.location;
  const online = isDeviceReportedOnline(device);
  if (!isDeviceOnline(device)) {
    return {
      address: t(device.locationEnabled === false ? "map.locationServicesOff" : "map.locationUnavailable"),
      status: t(online ? "map.online" : "map.offline"),
      stale: false,
    };
  }

  const stale = location?.isOld === true;
  const status = stale ? staleTimeLabel(location?.timeStamp) : timeAgo(location?.timeStamp);
  return { address: addressShort || t("map.findingAddress"), status: status || t(online ? "map.online" : "map.offline"), stale };
}

function distanceMeters(a: [number, number], b: [number, number]): number {
  const rad = Math.PI / 180;
  const dLat = (b[1] - a[1]) * rad;
  const dLng = (b[0] - a[0]) * rad;
  const s = Math.sin(dLat / 2) ** 2 + Math.cos(a[1] * rad) * Math.cos(b[1] * rad) * Math.sin(dLng / 2) ** 2;
  return 2 * 6371000 * Math.asin(Math.sqrt(s));
}

function clusterDevices(devices: Device[]): Device[][] {
  const groups: Device[][] = [];
  for (const device of devices) {
    const position = devicePosition(device);
    if (!position) continue;
    let hit: Device[] | undefined;
    for (const group of groups) {
      const near = group.some((member) => {
        const memberPosition = devicePosition(member);
        return memberPosition != null && distanceMeters(memberPosition, position) <= CLUSTER_RADIUS_M;
      });
      if (near) {
        hit = group;
        break;
      }
    }
    if (hit) hit.push(device);
    else groups.push([device]);
  }
  groups.forEach((group) => group.sort((a, b) => deviceKey(a).localeCompare(deviceKey(b))));
  return groups;
}

function markerSize(selected: boolean): number {
  return selected ? MARKER_BASE_SIZE + 8 : MARKER_BASE_SIZE;
}

function cssToken(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

const props = defineProps<{ data: DevicesResponse }>();
const emit = defineEmits<{ "play-sound": [deviceId: string]; "open-settings": [] }>();

const store = useAppStore();
const theme = useThemeStore();
const locale = useLocaleStore();
const t = locale.t;
const { devicesLoading } = storeToRefs(store);

const mapContainer = ref<HTMLDivElement | null>(null);
const listRef = ref<HTMLDivElement | null>(null);
const compassRef = ref<HTMLButtonElement | null>(null);

const devicePanelCollapsed = ref(false);

const clusterPopup = ref<Device[] | null>(null);

const liveDevices = ref<Device[]>(props.data.result.devices ?? []);
const selectedKey = ref<string | null>(null);
const lastUpdated = ref(new Date());

const addresses = ref<Record<string, AddressInfo>>({});

const addressFallbacks = ref<Record<string, string>>({});

const failedImages = ref(new Set<string>());

const mapError = ref<string | null>(null);

let map: AmapMapInstance | null = null;
let amap: AmapNamespace | null = null;

function applyMapStyle(): void {
  map?.setMapStyle?.(theme.isDark ? MAP_STYLE_DARK : MAP_STYLE_LIGHT);
}

// Keep world tiles tall enough to fill the viewport.
function minimumWorldZoom(): number {
  const height = Math.max(mapContainer.value?.clientHeight ?? 1, 1);
  return Math.max(3, Math.ceil(Math.log2(height / 256)));
}

// Enforce the minimum for wheel, touch, and button zoom changes.
function enforceMapMinZoom(): void {
  const mapInstance = map;
  if (!mapInstance) return;
  const minimum = minimumWorldZoom();
  if (mapInstance.getZoom() < minimum - 0.01) mapInstance.setZoom(minimum);
}

let accuracyCircle: unknown = null;

const markers = new Map<string, AmapMarker>();
const addressCache = new Map<string, AddressInfo>();
const addressInflight = new Set<string>();
const addressRetryAt = new Map<string, number>();
const addressRetryTimers = new Map<string, number>();
let markerClickAt = 0;
let compassDrag: { startX: number; startY: number; startRot: number; moved: boolean } | null = null;
let pollTimer: number | null = null;
let scrollRaf: number | null = null;

let disposed = false;

const selectedDevice = computed<Device | null>(() => liveDevices.value.find((device) => deviceKey(device) === selectedKey.value) ?? null);

const blurMap = computed(() => selectedDevice.value != null && hasNoLocation(selectedDevice.value));

const deviceRows = computed(() => {
  const rows = liveDevices.value.map((device) => {
    const key = deviceKey(device);
    const online = isDeviceReportedOnline(device);
    const resolved = addresses.value[key];
    const content = deviceRowContent(device, resolved?.short || resolved?.full || addressFallbacks.value[key]);
    return { device, key, ...content, online, locationTime: locationTimeMs(device.location?.timeStamp) ?? 0 };
  });
  return rows.sort((a, b) => {
    const priority = Number(!a.online) - Number(!b.online);
    if (priority !== 0) return priority;
    if (a.locationTime !== b.locationTime) return b.locationTime - a.locationTime;
    return (a.device.name ?? "").localeCompare(b.device.name ?? "");
  });
});

const detailAddress = computed(() => {
  const device = selectedDevice.value;
  if (!device) return "";
  if (isDeviceOnline(device)) {
    const address = addresses.value[deviceKey(device)]?.full;
    if (address) return address;
    const location = device.location;
    return location?.latitude != null && location.longitude != null ? `${location.latitude.toFixed(6)}, ${location.longitude.toFixed(6)}` : t("map.locationFound");
  }
  return t(device.locationEnabled === false ? "map.locationServicesOff" : "map.locationUnavailable");
});

const detailTime = computed(() => {
  const location = selectedDevice.value?.location;
  return location?.isOld ? staleTimeLabel(location.timeStamp) : timeAgo(location?.timeStamp);
});

const detailStale = computed(() => selectedDevice.value?.location?.isOld === true);

const lastSyncTime = computed(() => lastUpdated.value.toTimeString().slice(0, 8));

function requestRefresh(): void {
  if (devicesLoading.value) return;
  void store.refreshActiveDevices();
}

function showImage(device: Device): boolean {
  const url = device.deviceImageUrl;
  return url != null && url !== "" && !failedImages.value.has(url);
}

function markImageFailed(device: Device): void {
  const url = device.deviceImageUrl;
  if (url) failedImages.value.add(url);
}

function markerGlyph(device: Device, size: number): HTMLElement {
  const emojiFallback = (): HTMLElement => {
    const span = document.createElement("span");
    span.style.cssText = `width:100%;height:100%;display:flex;align-items:center;justify-content:center;font-size:${Math.round(size * 0.52)}px;line-height:1`;
    span.textContent = deviceEmoji(device.deviceClass);
    return span;
  };
  const url = device.deviceImageUrl;
  if (!url) return emojiFallback();

  const image = document.createElement("img");
  image.alt = "";
  image.style.cssText = `width:100%;height:100%;object-fit:contain;padding:${Math.round(size * 0.12)}px;box-sizing:border-box`;
  image.addEventListener("error", () => image.replaceWith(emojiFallback()), { once: true });
  image.src = url;
  return image;
}

function markerElement(device: Device, selected: boolean, size: number): HTMLDivElement {
  const wrap = document.createElement("div");
  wrap.style.cssText = `position:relative;width:${size}px;height:${size}px;cursor:pointer`;

  if (selected) {
    const tail = document.createElement("div");
    tail.style.cssText = `position:absolute;left:50%;bottom:-5px;width:14px;height:14px;transform:translateX(-50%) rotate(45deg);border-radius:3px;background:var(--elevated);box-shadow:1px 1px 0 color-mix(in srgb, var(--border) 70%, transparent)`;
    wrap.append(tail);
  }

  const box = document.createElement("div");
  box.style.cssText = `position:absolute;inset:0;z-index:1;overflow:hidden;border-radius:${Math.round(size * 0.28)}px;background:var(--elevated);border:1px solid color-mix(in srgb, var(--border) 70%, transparent);box-shadow:0 6px 20px color-mix(in srgb, var(--foreground) 20%, transparent), inset 0 1px 0 color-mix(in srgb, white 55%, transparent)`;
  box.append(markerGlyph(device, size));
  wrap.append(box);
  return wrap;
}

function clusterCenter(members: Device[]): [number, number] | null {
  const positions = members.map(devicePosition).filter((position): position is [number, number] => position !== null);
  if (positions.length === 0) return null;
  return [positions.reduce((sum, position) => sum + position[0], 0) / positions.length, positions.reduce((sum, position) => sum + position[1], 0) / positions.length];
}

function clusterKey(members: Device[]): string {
  return `cluster:${members.map(deviceKey).sort().join("|")}`;
}

function markerKeyForGroup(members: Device[]): string {
  return members.length === 1 ? `device:${deviceKey(members[0]!)}` : clusterKey(members);
}

function clusterMarkerWidth(count: number, size: number): number {
  const shown = Math.min(count, 3);
  const tile = Math.round(size * 0.72);
  const step = Math.round(size * 0.68);
  return 12 + tile + Math.max(shown - 1, 0) * step;
}

function clusterMarkerElement(members: Device[], size: number): HTMLDivElement {
  const shown = members.slice(0, 3);
  const width = clusterMarkerWidth(members.length, size);
  const tile = Math.round(size * 0.72);
  const step = Math.round(size * 0.68);
  const padding = 6;
  const top = Math.round((size - tile) / 2);
  const wrap = document.createElement("div");
  wrap.style.cssText = `position:relative;width:${width}px;height:${size}px;cursor:pointer`;

  const box = document.createElement("div");
  box.style.cssText = `position:absolute;inset:0;z-index:1;border-radius:999px;background:color-mix(in srgb, var(--card) 38%, transparent);box-shadow:0 10px 28px color-mix(in srgb, var(--foreground) 12%, transparent);backdrop-filter:blur(10px)`;

  for (const [index, member] of shown.entries()) {
    const cell = document.createElement("div");
    cell.style.cssText = `position:absolute;left:${padding + index * step}px;top:${top}px;width:${tile}px;height:${tile}px;z-index:${index + 2};display:grid;place-items:center;overflow:hidden;border-radius:${Math.round(tile * 0.28)}px;background:var(--elevated);border:1px solid color-mix(in srgb, var(--border) 70%, transparent);box-shadow:0 4px 13px color-mix(in srgb, var(--foreground) 16%, transparent), inset 0 1px 0 color-mix(in srgb, white 50%, transparent)`;
    cell.append(markerGlyph(member, tile));
    box.append(cell);
  }

  const badge = document.createElement("div");
  badge.textContent = String(members.length);
  badge.style.cssText = `position:absolute;right:-5px;top:-5px;z-index:5;display:grid;place-items:center;min-width:19px;height:19px;padding:0 5px;box-sizing:border-box;border:2px solid var(--card);border-radius:999px;background:var(--primary);color:var(--primary-foreground);font:650 10px/1 system-ui,sans-serif;box-shadow:0 3px 8px color-mix(in srgb, var(--foreground) 25%, transparent)`;
  wrap.append(box, badge);
  return wrap;
}

function syncMarkers(): void {
  const mapInstance = map;
  const amapInstance = amap;
  if (!mapInstance || !amapInstance) return;

  const groups = clusterDevices(liveDevices.value);
  const desired = new Map<string, { members: Device[]; position: [number, number]; selectedKey: string | null }>();

  for (const members of groups) {
    const selectedMember = selectedKey.value == null ? null : members.find((member) => deviceKey(member) === selectedKey.value);

    if (selectedMember) {
      const position = devicePosition(selectedMember);
      if (position) desired.set(`device:${deviceKey(selectedMember)}`, { members: [selectedMember], position, selectedKey: deviceKey(selectedMember) });
      continue;
    }

    const position = members.length === 1 ? devicePosition(members[0]!) : clusterCenter(members);
    if (!position) continue;
    desired.set(markerKeyForGroup(members), { members, position, selectedKey: null });
  }

  for (const [markerKey, entry] of desired) {
    let marker = markers.get(markerKey);
    if (!marker) {
      marker = new amapInstance.Marker({ position: entry.position, offset: new amapInstance.Pixel(0, 0), title: "" });
      marker.on("click", () => {
        markerClickAt = Date.now();
        const selectedTarget = liveDevices.value.find((device) => `device:${deviceKey(device)}` === markerKey);
        if (selectedTarget) {
          focusDevice(selectedTarget);
          return;
        }
        const members = clusterDevices(liveDevices.value).find((group) => markerKeyForGroup(group) === markerKey);
        if (!members) return;
        if (members.length > 1) {
          clusterPopup.value = [...members];
          return;
        }
        const target = liveDevices.value.find((device) => deviceKey(device) === deviceKey(members[0]!));
        if (target) focusDevice(target);
      });
      mapInstance.add(marker);
      markers.set(markerKey, marker);
    }

    const isGroup = entry.members.length > 1;
    const device = entry.members[0]!;
    const selected = entry.selectedKey != null;
    const size = isGroup ? MARKER_BASE_SIZE : markerSize(selected);
    const width = isGroup ? clusterMarkerWidth(entry.members.length, size) : size;
    const offsetY = -size / 2 + (!isGroup && selected ? 10 : 0);
    marker.show();
    marker.setPosition(entry.position);
    marker.setOffset(new amapInstance.Pixel(-width / 2, offsetY));
    if (isGroup) {
      marker.setContent(clusterMarkerElement(entry.members, size));
      marker.setTitle(`${entry.members.length} ${entry.members.length === 1 ? "device" : "devices"} at this location`);
      marker.setzIndex?.(selected ? 140 : 130);
    } else {
      marker.setContent(markerElement(device, selected, size));
      marker.setTitle(device.name ?? "");
      marker.setzIndex?.(selected ? 140 : 120);
    }
  }

  markers.forEach((marker, key) => {
    if (desired.has(key)) return;
    mapInstance.remove(marker);
    markers.delete(key);
  });

  const selected = liveDevices.value.find((device) => deviceKey(device) === selectedKey.value);
  const selectedPosition = selected ? devicePosition(selected) : null;
  if (accuracyCircle) {
    mapInstance.remove(accuracyCircle);
    accuracyCircle = null;
  }
  if (selected && selectedPosition) {
    const accuracy = selected.location?.horizontalAccuracy ?? null;
    const accent = cssToken("--success-solid") || "#34c759";

    const radius = Math.max(accuracy ?? 0, 8);
    const fillOpacity = theme.isDark ? 0.075 : 0.13;
    accuracyCircle = new amapInstance.Circle({
      center: selectedPosition,
      radius,
      fillColor: accent,
      fillOpacity,
      strokeColor: accent,
      strokeWeight: 1,
      strokeOpacity: theme.isDark ? 0.18 : 0.26,
      zIndex: 108,
      bubble: true,
    });
    mapInstance.add(accuracyCircle);
  }
}

function addressCacheKey(device: Device): string | null {
  const position = devicePosition(device);
  return position ? `${position[0].toFixed(5)},${position[1].toFixed(5)}` : null;
}

function coordinateFallback(device: Device): string | null {
  const location = device.location;
  if (location?.latitude == null || location.longitude == null) return null;
  return t("map.locationCoordinates", { coordinates: `${location.latitude.toFixed(4)}, ${location.longitude.toFixed(4)}` });
}

// Reuse one geocode result for every device at the same coordinate.
function applyAddressSuccess(cacheKey: string, info: AddressInfo): void {
  const nextAddresses = { ...addresses.value };
  const nextFallbacks = { ...addressFallbacks.value };
  for (const device of liveDevices.value) {
    if (addressCacheKey(device) !== cacheKey) continue;
    const key = deviceKey(device);
    nextAddresses[key] = info;
    delete nextFallbacks[key];
  }
  addresses.value = nextAddresses;
  addressFallbacks.value = nextFallbacks;
}

function applyAddressFallback(cacheKey: string): void {
  const nextFallbacks = { ...addressFallbacks.value };
  for (const device of liveDevices.value) {
    if (addressCacheKey(device) !== cacheKey) continue;
    const fallback = coordinateFallback(device);
    if (fallback) nextFallbacks[deviceKey(device)] = fallback;
  }
  addressFallbacks.value = nextFallbacks;
}

function scheduleAddressRetry(cacheKey: string): void {
  if (addressRetryTimers.has(cacheKey)) return;
  const timer = window.setTimeout(() => {
    addressRetryTimers.delete(cacheKey);
    addressRetryAt.delete(cacheKey);
    if (!disposed) resolveAddresses();
  }, 15_000);
  addressRetryTimers.set(cacheKey, timer);
}

function handleAddressFailure(cacheKey: string): void {
  addressRetryAt.set(cacheKey, Date.now() + 15_000);
  applyAddressFallback(cacheKey);
  scheduleAddressRetry(cacheKey);
}

// Resolve each coordinate once and retry failed lookups later.
function resolveAddresses(): void {
  for (const device of liveDevices.value) {
    if (!isDeviceOnline(device)) continue;
    const key = deviceKey(device);
    if (addresses.value[key]) continue;
    const position = devicePosition(device);
    const cacheKey = addressCacheKey(device);
    if (!position || !cacheKey) continue;

    const cached = addressCache.get(cacheKey);
    if (cached) {
      applyAddressSuccess(cacheKey, cached);
      continue;
    }
    if (addressInflight.has(cacheKey)) continue;
    const retryAt = addressRetryAt.get(cacheKey);
    if (retryAt != null && Date.now() < retryAt) continue;

    addressInflight.add(cacheKey);
    void reverseGeocode(position)
      .then((info) => {
        addressInflight.delete(cacheKey);
        if (disposed) return;
        if (!info) {
          handleAddressFailure(cacheKey);
          return;
        }
        const retryTimer = addressRetryTimers.get(cacheKey);
        if (retryTimer != null) clearTimeout(retryTimer);
        addressRetryTimers.delete(cacheKey);
        addressRetryAt.delete(cacheKey);
        addressCache.set(cacheKey, info);
        applyAddressSuccess(cacheKey, info);
      })
      .catch(() => {
        addressInflight.delete(cacheKey);
        if (!disposed) handleAddressFailure(cacheKey);
      });
  }
}

function scrollDeviceIntoView(key: string): void {
  if (scrollRaf != null) cancelAnimationFrame(scrollRaf);
  scrollRaf = requestAnimationFrame(() => {
    scrollRaf = null;
    listRef.value?.querySelector(`[data-device-key="${CSS.escape(key)}"]`)?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  });
}

function clearSelection(): void {
  clusterPopup.value = null;
  const previous = selectedKey.value;
  if (previous == null) return;
  selectedKey.value = null;
  syncMarkers();
  scrollDeviceIntoView(previous);
}

// Fit the accuracy circle into the map area beside the floating panel.
function selectedDeviceCamera(device: Device, position: [number, number]): { zoom: number; center: [number, number] } {
  const viewportWidth = Math.max(mapContainer.value?.clientWidth ?? 0, 1);
  const viewportHeight = Math.max(mapContainer.value?.clientHeight ?? 0, 1);
  const panelRight = DEVICE_PANEL_LEFT + DEVICE_PANEL_WIDTH + DEVICE_PANEL_GAP;
  const usableWidth = Math.max(viewportWidth - panelRight - DEVICE_FOCUS_RIGHT_GUTTER, 1);
  const usableHeight = Math.max(viewportHeight - DEVICE_FOCUS_TOP_GUTTER - DEVICE_FOCUS_BOTTOM_GUTTER, 1);

  const accuracyRadius = Math.max(device.location?.horizontalAccuracy ?? 24, 8);
  const accuracyDiameter = Math.max(accuracyRadius * 2, 16);
  const metersPerPixelAtZoom0 = WORLD_METERS_AT_ZOOM_0 * Math.max(Math.cos((position[1] * Math.PI) / 180), 0.2);
  const zoomToFitWidth = Math.log2((metersPerPixelAtZoom0 * usableWidth) / accuracyDiameter);
  const zoomToFitHeight = Math.log2((metersPerPixelAtZoom0 * usableHeight) / accuracyDiameter);
  const zoom = Math.min(DEVICE_FOCUS_MAX_ZOOM, Math.max(DEVICE_FOCUS_MIN_ZOOM, Math.min(zoomToFitWidth, zoomToFitHeight)));

  const targetScreenX = panelRight + usableWidth / 2;
  const offsetPixels = targetScreenX - viewportWidth / 2;
  const degreesPerPixel = 360 / (256 * 2 ** zoom);
  return {
    zoom,
    center: [position[0] - offsetPixels * degreesPerPixel, position[1]],
  };
}

function focusDevice(device: Device): void {
  selectedKey.value = deviceKey(device);
  syncMarkers();

  const position = devicePosition(device);
  const mapInstance = map;
  if (!position || !mapInstance) return;

  const camera = selectedDeviceCamera(device, position);
  mapInstance.setZoomAndCenter(camera.zoom, camera.center);
}

function focusAll(): void {
  clusterPopup.value = null;
  selectedKey.value = null;
  syncMarkers();
  const instances = [...markers.values()];
  if (instances.length === 0) return;

  map?.setFitView(instances, true, devicePanelCollapsed.value ? FIT_VIEW_MARGINS_COLLAPSED : FIT_VIEW_MARGINS);
}

function handlePlaySound(device: Device): void {
  if (!device.deviceId) return;
  emit("play-sound", device.deviceId);
}

function zoomIn(): void {
  map?.zoomIn();
}

function zoomOut(): void {
  const mapInstance = map;
  if (!mapInstance || mapInstance.getZoom() <= minimumWorldZoom()) return;
  mapInstance.zoomOut();
}

function onRotate(): void {
  const mapInstance = map;
  if (!mapInstance) return;
  const rotation = mapInstance.getRotation();
  if (compassRef.value) compassRef.value.style.transform = `rotate(${-rotation}deg)`;
}

function onMapClick(): void {
  if (Date.now() - markerClickAt < MARKER_CLICK_GRACE_MS) return;
  clusterPopup.value = null;
  clearSelection();
}

function onKeyDown(event: KeyboardEvent): void {
  if (event.key === "Escape") clearSelection();
}

function onCompassPointerDown(event: PointerEvent): void {
  const mapInstance = map;
  const button = compassRef.value;
  if (!mapInstance || !button) return;
  compassDrag = { startX: event.clientX, startY: event.clientY, startRot: mapInstance.getRotation(), moved: false };
  button.setPointerCapture(event.pointerId);
}

function onCompassPointerMove(event: PointerEvent): void {
  const drag = compassDrag;
  const button = compassRef.value;
  const mapInstance = map;
  if (!drag || !button || !mapInstance) return;

  const rect = button.getBoundingClientRect();
  const centerX = rect.left + rect.width / 2;
  const centerY = rect.top + rect.height / 2;
  const from = Math.atan2(drag.startY - centerY, drag.startX - centerX);
  const to = Math.atan2(event.clientY - centerY, event.clientX - centerX);
  const delta = ((to - from) * 180) / Math.PI;

  if (Math.abs(event.clientX - drag.startX) + Math.abs(event.clientY - drag.startY) > 4) drag.moved = true;

  if (drag.moved) mapInstance.setRotation(drag.startRot - delta, true);
}

function onCompassPointerUp(): void {
  const drag = compassDrag;
  compassDrag = null;

  if (drag && !drag.moved) map?.setRotation(0, true);
}

function describeError(error: unknown): string {
  if (error instanceof Error && error.message.startsWith("Could not load AMap")) return t("map.loadFailed");
  return error instanceof Error ? error.message : t("map.initFailed");
}

watch(
  () => theme.isDark,
  () => applyMapStyle(),
);

watch(
  () => props.data,
  (next) => {
    liveDevices.value = next.result.devices ?? [];
    lastUpdated.value = new Date();
    clusterPopup.value = null;

    if (selectedKey.value != null && !liveDevices.value.some((device) => deviceKey(device) === selectedKey.value)) {
      selectedKey.value = null;
    }
    resolveAddresses();
    syncMarkers();
  },
);

onMounted(async () => {
  window.addEventListener("keydown", onKeyDown);

  try {
    const AMap: AmapNamespace = await loadAmap();

    if (disposed || !mapContainer.value) return;
    amap = AMap;

    const instance = new AMap.Map(mapContainer.value, {
      zoom: 11,
      center: [120.1, 30.28],
      minZoom: minimumWorldZoom(),

      viewMode: "3D",
      pitch: 0,
      maxZoom: 20,
      rotateEnable: true,
      mapStyle: theme.isDark ? MAP_STYLE_DARK : MAP_STYLE_LIGHT,
    });
    map = instance;
    applyMapStyle();

    instance.on("rotatechange", onRotate);
    instance.on("zoomchange", enforceMapMinZoom);
    instance.on("zoomend", enforceMapMinZoom);
    instance.on("click", onMapClick);
    window.addEventListener("resize", enforceMapMinZoom);
    onRotate();

    syncMarkers();
    focusAll();
  } catch (error) {
    mapError.value = describeError(error);
    toast.error(mapError.value);
  }

  resolveAddresses();

  pollTimer = window.setInterval(requestRefresh, REFRESH_INTERVAL);
});

onUnmounted(() => {
  disposed = true;
  if (pollTimer != null) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
  if (scrollRaf != null) {
    cancelAnimationFrame(scrollRaf);
    scrollRaf = null;
  }
  window.removeEventListener("keydown", onKeyDown);
  window.removeEventListener("resize", enforceMapMinZoom);
  for (const timer of addressRetryTimers.values()) clearTimeout(timer);
  addressRetryTimers.clear();
  markers.clear();
  accuracyCircle = null;

  map?.destroy();
  map = null;
  amap = null;
});
</script>

<template>
  <div class="relative h-full min-h-0 overflow-hidden">
    <aside class="absolute top-4 left-4 z-40 flex max-h-[calc(100%-2rem)] w-[320px] min-h-0 flex-col overflow-hidden rounded-xl border border-border bg-card/95 shadow-2xl backdrop-blur-md">
      <button v-if="devicePanelCollapsed" type="button" class="flex h-11 w-full items-center gap-2 px-4 text-left transition-colors hover:bg-accent" :aria-label="t('map.expandList')" aria-expanded="false" :title="t('map.expandList')" @click="devicePanelCollapsed = false">
        <span class="min-w-0 flex-1 truncate text-md font-semibold">{{ t("map.devices") }}</span>
        <span class="text-xs text-muted-foreground">{{ t("map.deviceCount", { count: deviceRows.length }) }}</span>
        <ChevronDown :size="15" class="shrink-0 text-muted-foreground" />
      </button>

      <template v-if="!devicePanelCollapsed && !selectedDevice">
        <div class="flex shrink-0 items-center gap-1 border-b border-border px-3 py-1.5">
          <span class="flex min-w-0 flex-1 items-baseline gap-1.5">
            <span class="text-lg font-semibold tracking-tight">{{ t("map.devices") }}</span>
            <span class="text-xs text-muted-foreground">{{ t("map.deviceCount", { count: deviceRows.length }) }}</span>
          </span>

          <button type="button" class="grid size-7 shrink-0 place-items-center rounded-md text-primary transition-colors hover:bg-accent" :aria-label="t('map.showAll')" :title="t('map.showAll')" @click="focusAll">
            <Crosshair :size="14" />
          </button>
          <button type="button" class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="t('map.collapseList')" aria-expanded="true" :title="t('map.collapseList')" @click="devicePanelCollapsed = true">
            <ChevronUp :size="15" />
          </button>
        </div>

        <div ref="listRef" class="min-h-0 flex-1 overflow-y-auto px-2 py-1.5">
          <div v-if="deviceRows.length === 0" class="flex flex-col items-center gap-2.5 px-6 py-12 text-center text-muted-foreground">
            <RadarGlyph :size="26" />
            <p class="text-sm">{{ t("map.noDevices") }}</p>
          </div>

          <template v-else>
            <button
              v-for="row in deviceRows"
              :key="row.key"
              :data-device-key="row.key"
              type="button"
              class="flex min-h-[82px] w-full items-start gap-3.5 rounded-lg px-3 py-3 text-left transition-colors"
              :class="row.key === selectedKey ? 'bg-accent' : 'hover:bg-accent/60'"
              @click="focusDevice(row.device)"
            >
              <span class="relative mt-0.5 grid size-10 shrink-0 place-items-center rounded-xl bg-secondary/35">
                <img v-if="showImage(row.device)" :src="row.device.deviceImageUrl ?? ''" alt="" class="size-9 object-contain" @error="markImageFailed(row.device)" />
                <DeviceIcon v-else :device-class="row.device.deviceClass" :size="26" />
                <span v-if="hasLockBadge(row.device)" class="absolute -right-1 -bottom-1">
                  <LockBadge :device="row.device" :size="14" />
                </span>
              </span>

              <span class="min-w-0 flex-1">
                <span class="block truncate text-md font-medium">{{ row.device.name ?? t("map.unnamedDevice") }}</span>
                <span class="mt-0.5 block truncate text-sm text-muted-foreground">{{ row.address }}</span>
                <span class="mt-0.5 block truncate text-sm" :class="row.stale ? 'text-warning' : 'text-muted-foreground'">{{ row.status }}</span>
              </span>
            </button>
          </template>
        </div>
      </template>

      <template v-else-if="!devicePanelCollapsed && selectedDevice">
        <div class="flex h-10 shrink-0 items-center gap-1 border-b border-border px-3">
          <button type="button" class="-ml-0.5 flex h-7 items-center gap-1 rounded-md px-1.5 text-base text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="t('map.backToList')" @click="clearSelection">
            <ArrowLeft :size="14" />
            {{ t("map.devices") }}
          </button>
          <button type="button" class="ml-auto grid size-7 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="t('map.showAll')" :title="t('map.showAll')" @click="focusAll">
            <Crosshair :size="14" />
          </button>
          <button type="button" class="grid size-7 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="t('map.collapseDetails')" aria-expanded="true" :title="t('map.collapseList')" @click="devicePanelCollapsed = true">
            <ChevronUp :size="15" />
          </button>
        </div>

        <div class="min-h-0 flex-1 overflow-y-auto p-4">
          <div class="flex flex-col items-center pb-4 text-center">
            <span class="relative grid size-24 place-items-center">
              <img v-if="showImage(selectedDevice)" :src="selectedDevice.deviceImageUrl ?? ''" alt="" class="size-24 object-contain" @error="markImageFailed(selectedDevice)" />
              <DeviceIcon v-else :device-class="selectedDevice.deviceClass" :size="64" />
              <span v-if="hasLockBadge(selectedDevice)" class="absolute right-0 bottom-0">
                <LockBadge :device="selectedDevice" :size="22" />
              </span>
            </span>
            <p class="mt-2 text-md font-medium">{{ selectedDevice.name ?? t("map.unnamedDevice") }}</p>
            <p class="mt-0.5 text-sm text-muted-foreground">{{ selectedDevice.deviceDisplayName ?? selectedDevice.modelDisplayName ?? t("map.unknownModel") }}</p>
          </div>

          <div class="divide-y divide-border overflow-hidden rounded-lg border border-border">
            <div class="flex items-start gap-3 px-3 py-2">
              <span class="w-14 shrink-0 pt-px text-sm text-muted-foreground">{{ t("map.address") }}</span>
              <span class="min-w-0 flex-1 text-base break-words">{{ detailAddress }}</span>
            </div>
            <div class="flex items-center gap-3 px-3 py-2">
              <span class="w-14 shrink-0 text-sm text-muted-foreground">{{ t("map.locationTime") }}</span>
              <span class="tabular text-base" :class="detailStale ? 'text-warning' : ''">{{ detailTime || "—" }}</span>
            </div>
            <div class="flex items-center gap-3 px-3 py-2">
              <span class="w-14 shrink-0 text-sm text-muted-foreground">{{ t("map.battery") }}</span>
              <BatteryIcon :level="selectedDevice.batteryLevel" :charging="selectedDevice.batteryStatus === 'Charging'" />
              <span v-if="!isDeviceOnline(selectedDevice)" class="text-sm text-muted-foreground">{{ t("map.offline") }}</span>
            </div>
            <div class="flex items-center gap-3 px-3 py-2">
              <span class="w-14 shrink-0 text-sm text-muted-foreground">{{ t("map.location") }}</span>
              <span class="text-base">{{ t(selectedDevice.locationEnabled === false ? "map.off" : "map.on") }}</span>

              <Switch class="ml-auto" :model-value="selectedDevice.locationEnabled !== false" disabled :title="t('map.deviceStatusHint')" :aria-label="t(selectedDevice.locationEnabled === false ? 'map.locationServicesOff' : 'map.locationServicesOn')" />
            </div>
            <div class="flex items-center gap-3 px-3 py-2">
              <span class="w-14 shrink-0 text-sm text-muted-foreground">{{ t("map.security") }}</span>
              <span class="min-w-0 flex-1 text-base">{{ deviceLockLabel(selectedDevice, locale.isEnglish) }}</span>
              <LockBadge :device="selectedDevice" :size="16" />
            </div>
          </div>

          <div class="mt-3">
            <Button class="w-full" :disabled="!selectedDevice.deviceId" @click="handlePlaySound(selectedDevice)">
              <Volume2 :size="14" />
              {{ t("map.playSound") }}
            </Button>
            <p v-if="!selectedDevice.deviceId" class="mt-1.5 text-center text-xs text-muted-foreground">{{ t("map.soundUnsupported") }}</p>
          </div>
        </div>
      </template>
    </aside>

    <section class="absolute inset-0 z-0 min-w-0 overflow-hidden">
      <div class="absolute inset-0 bg-muted" :class="blurMap ? 'opacity-60 blur-[3px]' : ''">
        <div ref="mapContainer" class="h-full w-full" />
      </div>

      <div v-if="clusterPopup" class="absolute top-3 right-3 z-30 w-64 overflow-hidden rounded-xl border border-border bg-card/95 text-card-foreground shadow-xl backdrop-blur">
        <div class="flex items-center justify-between border-b border-border px-3 py-2.5">
          <span class="text-sm font-semibold">{{ t("map.sameLocation", { count: clusterPopup.length }) }}</span>
          <button type="button" class="grid size-6 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="t('map.closeDeviceList')" @click="clusterPopup = null">
            <X :size="13" />
          </button>
        </div>
        <div class="max-h-72 overflow-y-auto p-1.5">
          <button
            v-for="device in clusterPopup"
            :key="deviceKey(device)"
            type="button"
            class="flex w-full items-center gap-2.5 rounded-lg px-2 py-2 text-left transition-colors hover:bg-accent"
            @click="
              clusterPopup = null;
              focusDevice(device);
            "
          >
            <span class="grid size-8 shrink-0 place-items-center rounded-lg bg-secondary">
              <img v-if="showImage(device)" :src="device.deviceImageUrl ?? ''" alt="" class="size-6 object-contain" @error="markImageFailed(device)" />
              <DeviceIcon v-else :device-class="device.deviceClass" :size="19" />
            </span>
            <span class="min-w-0 flex-1">
              <span class="block truncate text-sm font-medium">{{ device.name ?? t("map.unnamedDevice") }}</span>
              <span class="block truncate text-xs text-muted-foreground">{{ device.deviceDisplayName ?? device.modelDisplayName ?? "" }}</span>
            </span>
          </button>
        </div>
      </div>

      <div v-if="mapError" class="absolute inset-0 z-30 flex flex-col items-center justify-center gap-2.5 bg-background/85 px-6 text-center backdrop-blur-sm">
        <CircleAlert class="size-5 text-destructive" />
        <p class="max-w-[280px] text-base">{{ mapError }}</p>
        <p class="max-w-[280px] text-xs text-muted-foreground">{{ t("map.keyHint") }}</p>
        <Button variant="outline" size="sm" class="mt-1" @click="emit('open-settings')">{{ t("map.settings") }}</Button>
      </div>

      <div class="absolute right-3 bottom-3 z-20 flex flex-col items-center gap-2">
        <div class="flex flex-col overflow-hidden rounded-lg border border-border bg-elevated/85 text-elevated-foreground shadow-sm backdrop-blur">
          <button type="button" class="grid size-8 place-items-center transition-colors hover:bg-accent" :aria-label="t('map.zoomIn')" :title="t('map.zoomIn')" @click="zoomIn">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true">
              <path d="M7 2.6v8.8M2.6 7h8.8" />
            </svg>
          </button>
          <span class="h-px w-full bg-border" />
          <button type="button" class="grid size-8 place-items-center transition-colors hover:bg-accent" :aria-label="t('map.zoomOut')" :title="t('map.zoomOut')" @click="zoomOut">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true">
              <path d="M2.6 7h8.8" />
            </svg>
          </button>
        </div>

        <button
          ref="compassRef"
          type="button"
          class="grid size-9 cursor-grab place-items-center rounded-full border border-border bg-elevated/85 text-elevated-foreground shadow-sm backdrop-blur transition-colors select-none hover:bg-accent active:cursor-grabbing"
          :aria-label="t('map.compassAria')"
          :title="t('map.compassTitle')"
          @pointerdown="onCompassPointerDown"
          @pointermove="onCompassPointerMove"
          @pointerup="onCompassPointerUp"
          @pointercancel="onCompassPointerUp"
        >
          <svg width="18" height="18" viewBox="0 0 18 18" aria-hidden="true">
            <polygon points="9,1.5 13,9.5 5,9.5" fill="var(--danger-solid)" />
            <polygon points="9,16.5 13,9.5 5,9.5" fill="currentColor" opacity="0.28" />
          </svg>
        </button>
      </div>

      <div class="pointer-events-none absolute bottom-3 left-1/2 z-20 flex -translate-x-1/2 items-center gap-2 rounded-md border border-border bg-elevated/85 px-2.5 py-1.5 text-xs whitespace-nowrap text-muted-foreground shadow-sm backdrop-blur">
        <span>{{ t("map.updatesEvery") }}</span>
        <i class="size-[3px] rounded-full bg-muted-foreground/40" />
        <span class="tabular">{{ t("map.lastSync", { time: lastSyncTime }) }}</span>
      </div>
    </section>
  </div>
</template>
