<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Aperture, CalendarDays, Camera, ExternalLink, FileImage, HardDrive, Image as ImageIcon, MapPin, PlaySquare, Star, Tags, X } from "@lucide/vue";

import { openExternal } from "@/lib/api";
import { reverseGeocode, type AddressInfo } from "@/lib/geocode";
import { useLocaleStore } from "@/stores/locale";
import type { PhotoItem } from "@/types";

const props = defineProps<{
  photo: PhotoItem;
  position: number;
  total: number;
}>();

const emit = defineEmits<{ close: [] }>();
const locale = useLocaleStore();
const t = locale.t;

const metadata = computed(() => props.photo.metadata ?? null);
const locationAddress = ref<AddressInfo | null>(null);
let locationRequest = 0;

function extensionFrom(value: string | null | undefined): string | null {
  if (!value) return null;
  const clean = value.split(/[?#]/, 1)[0] ?? value;
  const name = clean.split("/").pop() ?? clean;
  const dot = name.lastIndexOf(".");
  return dot > 0 && dot < name.length - 1 ? name.slice(dot + 1).toUpperCase() : null;
}

function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = units[0]!;
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024;
    unit = units[index]!;
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${unit}`;
}

function formatPhotoDate(value: number | null | undefined, options: Intl.DateTimeFormatOptions): string | null {
  if (!value) return null;
  const offset = props.photo.timeZoneOffset;
  const date = offset == null ? new Date(value) : new Date(value + offset * 1_000);
  return date.toLocaleString(locale.locale, offset == null ? options : { ...options, timeZone: "UTC" });
}

const takenAt = computed(() => {
  if (!props.photo.takenAt) return "—";
  try {
    return (
      formatPhotoDate(props.photo.takenAt, {
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        weekday: "short",
        hour: "2-digit",
        minute: "2-digit",
      }) || "—"
    );
  } catch {
    return "—";
  }
});

const fileName = computed(() => props.photo.fileName || props.photo.id || "—");
const kind = computed(() => (props.photo.isVideo ? t("common.video") : props.photo.isLivePhoto ? t("common.live") : t("common.photo")));
const format = computed(() => extensionFrom(props.photo.fileName) || extensionFrom(props.photo.originalUrl) || extensionFrom(props.photo.displayUrl) || "—");
const dimensions = computed(() => (props.photo.width && props.photo.height ? `${props.photo.width} × ${props.photo.height}` : "—"));
const fileSize = computed(() => formatBytes(props.photo.sizeBytes));
const originalAvailable = computed(() => Boolean(props.photo.originalUrl));
const liveAvailable = computed(() => Boolean(props.photo.isLivePhoto && props.photo.videoUrl));

const cameraName = computed(() => {
  const make = metadata.value?.cameraMake?.trim();
  const model = metadata.value?.cameraModel?.trim();
  if (!make) return model || null;
  if (!model) return make;
  return model.toLowerCase().startsWith(make.toLowerCase()) ? model : `${make} ${model}`;
});

const cameraDetails = computed(() => {
  const value = metadata.value;
  if (!value) return [];
  const focalLength = [value.focalLength, value.focalLength35mm && t("viewer.equivalentFocalLength", { value: value.focalLength35mm })].filter(Boolean).join(" · ");
  return [
    { label: t("viewer.lens"), value: value.lensModel || value.lensSpecification || (props.photo.isVideo ? t("viewer.noLensInfo") : null) },
    { label: t("viewer.focalLength"), value: focalLength },
    { label: t("viewer.aperture"), value: value.fNumber },
    { label: t("viewer.shutterSpeed"), value: value.exposureTime },
    { label: t("viewer.iso"), value: value.iso },
    { label: t("viewer.exposureBias"), value: value.exposureBias },
    { label: t("viewer.flash"), value: value.flash },
    { label: t("viewer.whiteBalance"), value: value.whiteBalance },
    { label: t("viewer.meteringMode"), value: value.meteringMode },
    { label: t("viewer.colorSpace"), value: value.colorSpace },
    { label: t("viewer.software"), value: value.software },
  ].filter((detail): detail is { label: string; value: string } => Boolean(detail.value?.trim()));
});

const coordinates = computed<[number, number] | null>(() => {
  const value = metadata.value;
  if (value?.gpsLatitude == null || value.gpsLongitude == null) return null;
  return [value.gpsLongitude, value.gpsLatitude];
});

const location = computed(() => {
  const value = coordinates.value;
  if (!value) return null;
  return `${value[1].toFixed(6)}, ${value[0].toFixed(6)}`;
});

watch(
  coordinates,
  async (value) => {
    const request = ++locationRequest;
    locationAddress.value = null;
    if (!value) return;
    const address = await reverseGeocode(value);
    if (request === locationRequest) locationAddress.value = address;
  },
  { immediate: true },
);

const displayLocation = computed(() => locationAddress.value?.full || location.value);
const addedAt = computed(() =>
  formatPhotoDate(props.photo.addedAt, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }),
);

function openLocationMap(): void {
  const value = coordinates.value;
  if (!value) return;
  const name = encodeURIComponent(t("viewer.photoLocation"));
  void openExternal(`https://uri.amap.com/marker?position=${value[0]},${value[1]}&name=${name}`);
}

const altitude = computed(() => {
  const value = metadata.value?.gpsAltitude;
  if (value == null || !Number.isFinite(value)) return null;
  return t("viewer.altitude", { value: value.toFixed(1) });
});

function formatDuration(seconds: number | null | undefined): string | null {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return null;
  const total = Math.round(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const remainder = total % 60;
  const pad = (value: number) => String(value).padStart(2, "0");
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(remainder)}` : `${pad(minutes)}:${pad(remainder)}`;
}

const videoDetails = computed(() => {
  if (!props.photo.isVideo) return [];
  const addedAt = formatPhotoDate(props.photo.addedAt, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
  return [
    { label: t("viewer.duration"), value: formatDuration(props.photo.durationSeconds) },
    { label: t("viewer.frameRate"), value: props.photo.frameRate ? `${props.photo.frameRate.toFixed(props.photo.frameRate % 1 ? 1 : 0)} fps` : null },
    { label: t("viewer.codec"), value: props.photo.codec },
    { label: t("viewer.addedAt"), value: addedAt },
  ].filter((detail): detail is { label: string; value: string } => Boolean(detail.value));
});
</script>

<template>
  <aside class="absolute top-3 right-3 z-30 flex max-h-[calc(100%-86px)] w-[320px] max-w-[calc(100vw-24px)] flex-col overflow-hidden rounded-xl border border-border bg-card/98 text-card-foreground shadow-2xl backdrop-blur-xl" role="dialog" :aria-label="t('viewer.info')">
    <header class="flex h-12 shrink-0 items-center border-b border-border px-2">
      <button type="button" class="grid size-8 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="t('common.close')" @click="emit('close')">
        <X :size="16" />
      </button>
      <h2 class="flex-1 text-center text-sm font-semibold">{{ t("viewer.info") }}</h2>
      <span class="size-8" aria-hidden="true"></span>
    </header>

    <div class="min-h-0 space-y-2 overflow-y-auto bg-muted/25 p-3">
      <section class="overflow-hidden rounded-lg border border-border bg-card">
        <div class="flex items-start gap-3 p-3.5">
          <CalendarDays :size="16" class="mt-0.5 shrink-0 text-muted-foreground" />
          <div class="min-w-0">
            <p class="text-sm font-medium">{{ takenAt }}</p>
            <p class="mt-1 text-xs text-muted-foreground">{{ t("viewer.photoPosition", { index: position, total }) }}</p>
          </div>
        </div>
      </section>

      <section v-if="videoDetails.length" class="divide-y divide-border overflow-hidden rounded-lg border border-border bg-card">
        <div v-for="detail in videoDetails" :key="detail.label" class="flex items-center gap-4 px-3.5 py-2.5">
          <span class="w-[88px] shrink-0 text-xs text-muted-foreground">{{ detail.label }}</span>
          <span class="min-w-0 flex-1 text-right text-xs font-medium">{{ detail.value }}</span>
        </div>
      </section>

      <section v-if="metadata" class="overflow-hidden rounded-lg border border-border bg-card">
        <div v-if="cameraName" class="flex items-start gap-3 border-b border-border px-3.5 py-3">
          <Camera :size="16" class="mt-0.5 shrink-0 text-muted-foreground" />
          <div class="min-w-0 flex-1">
            <p class="text-xs text-muted-foreground">{{ t("viewer.camera") }}</p>
            <p class="mt-0.5 text-sm font-medium">{{ cameraName }}</p>
          </div>
          <Star v-if="photo.isFavorite" :size="14" class="mt-1 shrink-0 fill-current text-warning" />
        </div>
        <dl v-if="cameraDetails.length" class="divide-y divide-border">
          <div v-for="detail in cameraDetails" :key="detail.label" class="flex items-start gap-4 px-3.5 py-2.5">
            <dt class="w-[88px] shrink-0 text-xs text-muted-foreground">{{ detail.label }}</dt>
            <dd class="min-w-0 flex-1 text-right text-xs font-medium break-words">{{ detail.value }}</dd>
          </div>
        </dl>
        <div v-if="displayLocation" class="flex items-start gap-3 border-t border-border px-3.5 py-3">
          <MapPin :size="15" class="mt-0.5 shrink-0 text-muted-foreground" />
          <div class="min-w-0 flex-1">
            <p class="text-xs text-muted-foreground">{{ t("viewer.location") }}</p>
            <div class="mt-0.5 flex items-start gap-2">
              <p class="min-w-0 flex-1 text-xs font-medium">{{ displayLocation }}</p>
              <button type="button" class="shrink-0 rounded p-0.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :title="t('viewer.openLocation')" :aria-label="t('viewer.openLocation')" @click="openLocationMap">
                <ExternalLink :size="13" />
              </button>
            </div>
            <p class="tabular mt-1 text-xs text-muted-foreground">{{ location }}</p>
            <p v-if="altitude" class="mt-0.5 text-xs text-muted-foreground">{{ altitude }}</p>
          </div>
        </div>
        <div v-if="photo.keywords?.length" class="flex items-start gap-3 border-t border-border px-3.5 py-3">
          <Tags :size="15" class="mt-0.5 shrink-0 text-muted-foreground" />
          <div class="min-w-0 flex-1">
            <p class="text-xs text-muted-foreground">{{ t("viewer.keywords") }}</p>
            <div class="mt-1.5 flex flex-wrap gap-1">
              <span v-for="keyword in photo.keywords" :key="keyword" class="rounded-full bg-muted px-2 py-0.5 text-2xs font-medium">{{ keyword }}</span>
            </div>
          </div>
        </div>
      </section>
      <p v-else-if="!photo.isVideo" class="flex items-center gap-2 rounded-lg border border-border bg-card px-3.5 py-3 text-xs text-muted-foreground">
        <Aperture :size="14" />
        {{ t("viewer.noMetadata") }}
      </p>

      <section class="divide-y divide-border overflow-hidden rounded-lg border border-border bg-card">
        <div class="flex items-start gap-3 px-3.5 py-3">
          <FileImage :size="16" class="mt-0.5 shrink-0 text-muted-foreground" />
          <div class="min-w-0 flex-1">
            <p class="text-xs text-muted-foreground">{{ t("viewer.fileName") }}</p>
            <p class="mt-0.5 text-sm font-medium break-all">{{ fileName }}</p>
          </div>
        </div>
        <div class="flex items-start gap-3 px-3.5 py-3">
          <ImageIcon :size="16" class="mt-0.5 shrink-0 text-muted-foreground" />
          <div class="min-w-0 flex-1">
            <p class="text-xs text-muted-foreground">{{ t("photos.type") }}</p>
            <p class="mt-0.5 text-sm font-medium">{{ kind }}</p>
          </div>
          <span class="rounded-full bg-muted px-2 py-0.5 text-2xs font-medium text-muted-foreground">{{ format }}</span>
        </div>
        <div v-if="addedAt" class="flex items-start gap-3 px-3.5 py-3">
          <CalendarDays :size="16" class="mt-0.5 shrink-0 text-muted-foreground" />
          <div class="min-w-0 flex-1">
            <p class="text-xs text-muted-foreground">{{ t("viewer.addedAt") }}</p>
            <p class="mt-0.5 text-sm font-medium">{{ addedAt }}</p>
          </div>
        </div>
      </section>

      <section class="grid grid-cols-2 divide-x divide-border overflow-hidden rounded-lg border border-border bg-card">
        <div class="p-3.5">
          <p class="text-xs text-muted-foreground">{{ t("photos.dimensions") }}</p>
          <p class="tabular mt-1 text-sm font-medium">{{ dimensions }}</p>
        </div>
        <div class="p-3.5">
          <div class="flex items-center gap-1.5 text-xs text-muted-foreground"><HardDrive :size="13" />{{ t("viewer.fileSize") }}</div>
          <p class="tabular mt-1 text-sm font-medium">{{ fileSize }}</p>
        </div>
      </section>

      <section class="divide-y divide-border overflow-hidden rounded-lg border border-border bg-card">
        <div class="flex items-center gap-3 px-3.5 py-3">
          <FileImage :size="15" class="shrink-0 text-muted-foreground" />
          <span class="flex-1 text-sm">{{ t("photos.original") }}</span>
          <span class="text-xs" :class="originalAvailable ? 'text-success' : 'text-muted-foreground'">{{ t(originalAvailable ? "common.available" : "common.unavailable") }}</span>
        </div>
        <div class="flex items-center gap-3 px-3.5 py-3">
          <PlaySquare :size="15" class="shrink-0 text-muted-foreground" />
          <span class="flex-1 text-sm">{{ t("viewer.livePhotoVideo") }}</span>
          <span class="text-xs" :class="liveAvailable ? 'text-success' : 'text-muted-foreground'">{{ t(liveAvailable ? "common.available" : "common.unavailable") }}</span>
        </div>
      </section>
    </div>
  </aside>
</template>
