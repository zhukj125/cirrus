<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { ChevronLeft, ChevronRight, Download, FlipHorizontal2, Info as InfoIcon, Maximize2, Pause, Play, RotateCcw, RotateCw, X, ZoomIn, ZoomOut } from "@lucide/vue";

import { downloadPhoto, photosList } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import Spinner from "@/components/ui/Spinner.vue";
import PhotoInfoPanel from "@/components/photos/PhotoInfoPanel.vue";
import { Button } from "@/components/ui/button";
import { useLocaleStore } from "@/stores/locale";
import type { PhotoItem } from "@/types";

const props = defineProps<{
  photos: PhotoItem[];
  index: number;

  hasMore?: boolean;
  baseRank?: number;
  totalCount?: number | null;
  sessionId?: string;
}>();
const emit = defineEmits<{ "update:index": [value: number]; close: []; needMore: []; seekRank: [rank: number] }>();
const locale = useLocaleStore();
const t = locale.t;

function formatTime(takenAt: number | null | undefined): string {
  if (!takenAt) return "";
  try {
    return new Date(takenAt).toLocaleString(locale.locale, {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}

const loaded = ref(false);

const playingLive = ref(false);

const liveVideo = ref<HTMLVideoElement | null>(null);

const rotation = ref(0);
const flipped = ref(false);

const stage = ref<HTMLElement | null>(null);
const stageSize = ref({ width: 0, height: 0 });
const filmstrip = ref<HTMLElement | null>(null);

const FILMSTRIP_SETTLE_MS = 140;
const FILMSTRIP_PREFETCH_PX = 600;
const FILMSTRIP_OVERSCAN_PX = 700;
const PHOTO_PREFETCH_COUNT = 24;
const WARM_IMAGE_LIMIT = 18;
const NEED_MORE_COOLDOWN_MS = 1_000;
const FILMSTRIP_PAGE_SIZE = 60;
const FILMSTRIP_PREFETCH_DELAY_MS = 160;
const FILMSTRIP_GAP_PX = 5;
const FILMSTRIP_ITEM_WIDTH = 32;

let filmstripFrame: number | null = null;
let filmstripSettleTimer: number | null = null;
let filmstripProgrammaticTimer: number | null = null;
let filmstripUserScrolling = false;
let filmstripProgrammaticScroll = false;
let needMoreTimer: number | null = null;
let filmstripPrefetchTimer: number | null = null;
let displayLoadToken = 0;
let displayLoadTimer: number | null = null;
let activeDisplayPreloader: HTMLImageElement | null = null;
const warmedImages = new Map<string, HTMLImageElement>();
const filmstripDragging = ref(false);
const filmstripFailed = reactive(new Set<string>());
const filmstripScrollLeft = ref(0);
const filmstripTargetRank = ref(0);
const knownFilmstripPhotos = reactive(new Map<number, PhotoItem>());
const knownFilmstripPages = reactive(new Set<number>());
const pendingFilmstripPages = new Set<number>();
const filmstripViewportWidth = ref(0);
let filmstripDrag: { pointerId: number; originX: number; lastX: number; moved: boolean } | null = null;
let suppressFilmstripClick = false;

function filmstripImageUrl(photo: PhotoItem): string {
  return photo.thumbUrl || photo.displayUrl || "";
}

function filmstripItemStyle(item: { rank: number; left: number; width: number }): Record<string, string> {
  const inset = item.rank === filmstripTargetRank.value ? -3 : 1;
  return { left: `${item.left + inset}px`, width: `${item.width - inset * 2}px` };
}

function onFilmstripImageError(event: Event, photo: PhotoItem): void {
  const image = event.currentTarget;
  if (!(image instanceof HTMLImageElement)) return;
  const fallback = photo.displayUrl;
  if (fallback && image.dataset.usedFallback !== "true") {
    image.dataset.usedFallback = "true";
    image.src = fallback;
    return;
  }
  filmstripFailed.add(photo.id);
}

const baseRank = computed(() => props.baseRank ?? 0);
const currentGlobalRank = computed(() => baseRank.value + props.index);
const filmstripCount = computed(() => props.totalCount ?? props.photos.length);
const atFirstPhoto = computed(() => (props.totalCount != null ? currentGlobalRank.value <= 0 : props.index <= 0));
const atLastPhoto = computed(() => (props.totalCount != null ? currentGlobalRank.value >= props.totalCount - 1 : props.index >= props.photos.length - 1 && !props.hasMore));

const filmstripLayout = computed(() => {
  const count = filmstripCount.value;
  if (count === 0) return [];
  let left = Math.max(0, (filmstripViewportWidth.value - FILMSTRIP_ITEM_WIDTH) / 2);
  return Array.from({ length: count }, (_, rank) => {
    const photo = knownFilmstripPhotos.get(rank);
    const width = FILMSTRIP_ITEM_WIDTH;
    const item = { photo, rank, left, width };
    left += width + FILMSTRIP_GAP_PX;
    return item;
  });
});

const filmstripTotalWidth = computed(() => {
  const layout = filmstripLayout.value;
  const last = layout[layout.length - 1];
  if (!last) return filmstripViewportWidth.value;
  const rightPadding = Math.max(0, (filmstripViewportWidth.value - last.width) / 2);
  return Math.max(filmstripViewportWidth.value, last.left + last.width + rightPadding);
});

const filmstripItems = computed(() => {
  const layout = filmstripLayout.value;
  if (layout.length === 0) return [];
  const from = Math.max(0, filmstripScrollLeft.value - FILMSTRIP_OVERSCAN_PX);
  const to = filmstripScrollLeft.value + filmstripViewportWidth.value + FILMSTRIP_OVERSCAN_PX;
  let low = 0;
  let high = layout.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    const item = layout[middle]!;
    if (item.left + item.width < from) low = middle + 1;
    else high = middle;
  }
  let end = low;
  while (end < layout.length && layout[end]!.left <= to) end += 1;
  return layout.slice(low, Math.min(layout.length, end + 1));
});

watch(
  [() => props.photos, () => props.baseRank],
  ([items, base]) => {
    const startRank = base ?? 0;
    items.forEach((item, index) => {
      const rank = startRank + index;
      knownFilmstripPhotos.set(rank, item);
      knownFilmstripPages.add(Math.floor(rank / FILMSTRIP_PAGE_SIZE) * FILMSTRIP_PAGE_SIZE);
    });
    if (!filmstripUserScrolling) filmstripTargetRank.value = Math.min(Math.max(0, currentGlobalRank.value), Math.max(0, filmstripCount.value - 1));
  },
  { immediate: true },
);

function updateFilmstripViewport(): void {
  filmstripViewportWidth.value = filmstrip.value?.clientWidth ?? 0;
  if (filmstrip.value) filmstripScrollLeft.value = filmstrip.value.scrollLeft;
}
async function prefetchFilmstripPage(rank: number): Promise<void> {
  const sessionId = props.sessionId?.trim();
  if (!sessionId || props.totalCount == null) return;
  const pageStart = Math.floor(rank / FILMSTRIP_PAGE_SIZE) * FILMSTRIP_PAGE_SIZE;
  if (knownFilmstripPages.has(pageStart) || pendingFilmstripPages.has(pageStart)) return;

  pendingFilmstripPages.add(pageStart);
  try {
    const page = await photosList({ sessionId, startRank: pageStart, limit: FILMSTRIP_PAGE_SIZE });
    if (page.status === "ERROR" || page.photos.length === 0) return;
    knownFilmstripPages.add(pageStart);
    page.photos.forEach((photo, offset) => {
      const globalRank = pageStart + offset;
      if (globalRank < filmstripCount.value) knownFilmstripPhotos.set(globalRank, photo);
    });
  } finally {
    pendingFilmstripPages.delete(pageStart);
  }
}

function scheduleFilmstripPrefetch(rank: number): void {
  if (!props.sessionId || props.totalCount == null) return;
  if (filmstripPrefetchTimer !== null) window.clearTimeout(filmstripPrefetchTimer);
  filmstripPrefetchTimer = window.setTimeout(() => {
    filmstripPrefetchTimer = null;
    void prefetchFilmstripPage(rank);
  }, FILMSTRIP_PREFETCH_DELAY_MS);
}

const saving = ref(false);
const infoOpen = ref(false);

const photo = computed<PhotoItem | null>(() => {
  const item: PhotoItem | undefined = props.photos[props.index];
  return item ?? null;
});

const renderedFilmstripItems = computed(() => filmstripItems.value.map((item) => (item.rank === currentGlobalRank.value && photo.value ? { ...item, photo: photo.value } : item)));

const isVideo = computed(() => photo.value?.isVideo === true);
const isLivePhoto = computed(() => photo.value?.isLivePhoto === true && !isVideo.value);
const canPlayLive = computed(() => isLivePhoto.value && Boolean(photo.value?.videoUrl));
const displayUrl = computed(() => photo.value?.displayUrl || photo.value?.thumbUrl || "");
const videoUrl = computed(() => photo.value?.videoUrl ?? "");

const originalUrl = computed(() => photo.value?.originalUrl || photo.value?.displayUrl || photo.value?.thumbUrl || null);
const fileName = computed(() => photo.value?.fileName ?? "");
const time = computed(() => formatTime(photo.value?.takenAt));

const MIN_SCALE = 1;
const MAX_SCALE = 8;

const DOUBLE_CLICK_SCALE = 2;

const scale = ref(1);
const offset = ref({ x: 0, y: 0 });
const dragging = ref(false);

let dragOrigin = { pointerX: 0, pointerY: 0, x: 0, y: 0 };

function zoomTo(next: number): void {
  scale.value = Math.min(MAX_SCALE, Math.max(MIN_SCALE, next));

  if (scale.value === MIN_SCALE) offset.value = { x: 0, y: 0 };
}

function resetZoom(): void {
  scale.value = MIN_SCALE;
  offset.value = { x: 0, y: 0 };
}

function resetImageTransform(): void {
  resetZoom();
  rotation.value = 0;
  flipped.value = false;
}

function rotateBy(delta: number): void {
  rotation.value = (rotation.value + delta + 360) % 360;
  offset.value = { x: 0, y: 0 };
}

function toggleFlip(): void {
  flipped.value = !flipped.value;
  offset.value = { x: 0, y: 0 };
}

function updateStageSize(): void {
  const element = stage.value;
  if (element) {
    stageSize.value = {
      width: Math.max(0, element.clientWidth - 32),
      height: Math.max(0, element.clientHeight - 32),
    };
  }
  updateFilmstripViewport();
}

const quarterTurn = computed(() => rotation.value % 180 !== 0);
const imageTransform = computed(() => {
  const parts = [`translate(${offset.value.x}px, ${offset.value.y}px)`, `scale(${scale.value})`];
  if (rotation.value !== 0) parts.push(`rotate(${rotation.value}deg)`);
  if (flipped.value) parts.push("scaleX(-1)");
  return parts.join(" ");
});
const imageStyle = computed(() => ({
  transform: imageTransform.value,
  transformOrigin: "center center",
  maxWidth: stageSize.value.width > 0 ? `${quarterTurn.value ? stageSize.value.height : stageSize.value.width}px` : undefined,
  maxHeight: stageSize.value.height > 0 ? `${quarterTurn.value ? stageSize.value.width : stageSize.value.height}px` : undefined,
}));

function scheduleCurrentPhotoLoad(): void {
  const token = ++displayLoadToken;
  if (activeDisplayPreloader !== null) {
    activeDisplayPreloader.onload = null;
    activeDisplayPreloader.onerror = null;
    activeDisplayPreloader.src = "";
    activeDisplayPreloader = null;
  }
  if (displayLoadTimer !== null) {
    window.clearTimeout(displayLoadTimer);
    displayLoadTimer = null;
  }
  loaded.value = false;
  displayLoadTimer = window.setTimeout(() => {
    displayLoadTimer = null;
    if (token !== displayLoadToken) return;
    const item = photo.value;
    if (!item || item.isVideo) {
      loaded.value = true;
      warmPhotoNeighborhood();
      return;
    }

    const url = displayUrl.value;
    if (!url) {
      warmPhotoNeighborhood();
      return;
    }
    const image = new Image();
    image.decoding = "async";
    image.fetchPriority = "high";
    activeDisplayPreloader = image;
    image.onload = () => {
      if (token !== displayLoadToken) return;
      warmedImages.set(url, image);
      activeDisplayPreloader = null;
      loaded.value = true;
      warmPhotoNeighborhood();
    };
    image.onerror = () => {
      if (token !== displayLoadToken) return;
      activeDisplayPreloader = null;
      warmPhotoNeighborhood();
    };
    image.src = url;
  }, 50);
}

watch(
  () => photo.value?.id,
  async () => {
    scheduleCurrentPhotoLoad();
    playingLive.value = canPlayLive.value;
    resetImageTransform();
    if (!playingLive.value) return;
    await nextTick();
    try {
      await liveVideo.value?.play();
    } catch {}
  },
  { immediate: true },
);

function zoomStep(direction: 1 | -1): void {
  zoomTo(scale.value * (direction === 1 ? 1.25 : 1 / 1.25));
}

function onWheel(event: WheelEvent): void {
  zoomTo(scale.value * Math.exp(-event.deltaY * 0.0015));
}

function toggleZoom(): void {
  if (scale.value > MIN_SCALE) resetZoom();
  else zoomTo(DOUBLE_CLICK_SCALE);
}

function onDragMove(event: MouseEvent): void {
  if (!dragging.value) return;
  offset.value = { x: dragOrigin.x + (event.clientX - dragOrigin.pointerX), y: dragOrigin.y + (event.clientY - dragOrigin.pointerY) };
}

function endDrag(): void {
  dragging.value = false;
  window.removeEventListener("mousemove", onDragMove);
  window.removeEventListener("mouseup", endDrag);
}

function startDrag(event: MouseEvent): void {
  if (scale.value <= MIN_SCALE) return;
  dragging.value = true;
  dragOrigin = { pointerX: event.clientX, pointerY: event.clientY, x: offset.value.x, y: offset.value.y };

  window.addEventListener("mousemove", onDragMove);
  window.addEventListener("mouseup", endDrag);
}

function clearFilmstripSettle(): void {
  if (filmstripSettleTimer !== null) {
    window.clearTimeout(filmstripSettleTimer);
    filmstripSettleTimer = null;
  }
}

function prepareFilmstripUserScroll(): void {
  filmstripProgrammaticScroll = false;
  if (filmstripProgrammaticTimer !== null) {
    window.clearTimeout(filmstripProgrammaticTimer);
    filmstripProgrammaticTimer = null;
  }
  clearFilmstripSettle();
}

function beginFilmstripUserScroll(): void {
  filmstripUserScrolling = true;
  prepareFilmstripUserScroll();
}

function nearestFilmstripIndex(strip: HTMLElement): number {
  const layout = filmstripLayout.value;
  if (layout.length === 0) return currentGlobalRank.value;
  const center = strip.scrollLeft + strip.clientWidth / 2;
  let low = 0;
  let high = layout.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    const item = layout[middle]!;
    if (item.left + item.width / 2 < center) low = middle + 1;
    else high = middle;
  }
  const candidates = [layout[low - 1], layout[low]].filter((item): item is (typeof layout)[number] => Boolean(item));
  let nearest = currentGlobalRank.value;
  let nearestDistance = Number.POSITIVE_INFINITY;
  for (const item of candidates) {
    const distance = Math.abs(item.left + item.width / 2 - center);
    if (distance >= nearestDistance) continue;
    nearest = item.rank;
    nearestDistance = distance;
  }
  return nearest;
}

function requestMorePhotos(): void {
  if (!props.hasMore || needMoreTimer !== null) return;
  emit("needMore");
  needMoreTimer = window.setTimeout(() => {
    needMoreTimer = null;
  }, NEED_MORE_COOLDOWN_MS);
}

function requestMoreFromFilmstrip(strip: HTMLElement): void {
  if (!props.hasMore || props.totalCount != null) return;
  const remaining = strip.scrollWidth - strip.clientWidth - strip.scrollLeft;
  if (remaining <= FILMSTRIP_PREFETCH_PX) requestMorePhotos();
}

async function scrollFilmstrip(behavior: ScrollBehavior = "smooth"): Promise<void> {
  await nextTick();
  const strip = filmstrip.value;
  const item = filmstripLayout.value[filmstripTargetRank.value];
  if (!strip || !item || filmstripUserScrolling || typeof strip.scrollTo !== "function") return;

  const centeredLeft = item.left + item.width / 2 - strip.clientWidth / 2;
  const maxScroll = Math.max(0, strip.scrollWidth - strip.clientWidth);
  filmstripProgrammaticScroll = true;
  if (filmstripProgrammaticTimer !== null) window.clearTimeout(filmstripProgrammaticTimer);
  filmstripProgrammaticTimer = window.setTimeout(
    () => {
      filmstripProgrammaticScroll = false;
      filmstripProgrammaticTimer = null;
    },
    behavior === "smooth" ? 420 : 80,
  );
  strip.scrollTo({ left: Math.min(maxScroll, Math.max(0, centeredLeft)), behavior });
}

function finishFilmstripScroll(): void {
  filmstripSettleTimer = null;
  if (!filmstripUserScrolling) return;
  filmstripUserScrolling = false;
  const targetRank = filmstripTargetRank.value;
  const localIndex = targetRank - baseRank.value;
  if (localIndex >= 0 && localIndex < props.photos.length) {
    if (localIndex !== props.index) selectFilmstripPhoto(localIndex);
  } else {
    emit("seekRank", targetRank);
  }
  void scrollFilmstrip("smooth");
}

function onFilmstripScroll(): void {
  const strip = filmstrip.value;
  if (!strip) return;
  filmstripScrollLeft.value = strip.scrollLeft;
  requestMoreFromFilmstrip(strip);
  if (filmstripProgrammaticScroll) return;

  beginFilmstripUserScroll();
  clearFilmstripSettle();
  filmstripSettleTimer = window.setTimeout(finishFilmstripScroll, FILMSTRIP_SETTLE_MS);
  if (filmstripFrame !== null) return;
  filmstripFrame = window.requestAnimationFrame(() => {
    filmstripFrame = null;
    const current = filmstrip.value;
    if (!current) return;
    filmstripTargetRank.value = nearestFilmstripIndex(current);
    scheduleFilmstripPrefetch(filmstripTargetRank.value);
  });
}
function onFilmstripWheel(event: WheelEvent): void {
  if (Math.abs(event.deltaX) > Math.abs(event.deltaY)) return;
  if (event.deltaY === 0) return;
  event.preventDefault();
  const strip = filmstrip.value;
  if (!strip) return;
  prepareFilmstripUserScroll();
  strip.scrollBy({ left: event.deltaY, behavior: "auto" });
  onFilmstripScroll();
}

function onFilmstripPointerDown(event: PointerEvent): void {
  const strip = filmstrip.value;
  if (!strip || event.button !== 0) return;
  prepareFilmstripUserScroll();
  filmstripDrag = { pointerId: event.pointerId, originX: event.clientX, lastX: event.clientX, moved: false };
}

function onFilmstripPointerMove(event: PointerEvent): void {
  const strip = filmstrip.value;
  const drag = filmstripDrag;
  if (!strip || !drag || drag.pointerId !== event.pointerId) return;
  if (!drag.moved && Math.abs(event.clientX - drag.originX) > 3) {
    drag.moved = true;
    filmstripDragging.value = true;
    strip.setPointerCapture?.(event.pointerId);
  }
  if (!drag.moved) return;

  event.preventDefault();
  const delta = event.clientX - drag.lastX;
  drag.lastX = event.clientX;
  if (delta !== 0) strip.scrollLeft -= delta;
  onFilmstripScroll();
}

function finishFilmstripPointer(event: PointerEvent): void {
  const strip = filmstrip.value;
  const drag = filmstripDrag;
  if (!strip || !drag || drag.pointerId !== event.pointerId) return;
  filmstripDrag = null;
  filmstripDragging.value = false;
  if (strip.hasPointerCapture?.(event.pointerId)) strip.releasePointerCapture(event.pointerId);
  if (!drag.moved) return;

  suppressFilmstripClick = true;
  window.setTimeout(() => {
    suppressFilmstripClick = false;
  }, 0);
  if (filmstripUserScrolling) {
    clearFilmstripSettle();
    filmstripSettleTimer = window.setTimeout(finishFilmstripScroll, FILMSTRIP_SETTLE_MS);
  }
}

function onFilmstripClick(event: MouseEvent, rank: number): void {
  if (suppressFilmstripClick) {
    event.preventDefault();
    return;
  }
  filmstripTargetRank.value = rank;
  const localIndex = rank - baseRank.value;
  if (localIndex >= 0 && localIndex < props.photos.length) selectFilmstripPhoto(localIndex);
  else emit("seekRank", rank);
  void scrollFilmstrip("smooth");
}

function selectFilmstripPhoto(next: number): void {
  if (next < 0 || next >= props.photos.length || next === props.index) return;
  loaded.value = false;
  playingLive.value = false;
  resetImageTransform();
  filmstripTargetRank.value = baseRank.value + next;
  emit("update:index", next);
  if (props.totalCount == null && next >= props.photos.length - PHOTO_PREFETCH_COUNT) requestMorePhotos();
}

function go(delta: number): void {
  const next = props.index + delta;
  const globalTarget = currentGlobalRank.value + delta;
  if (next < 0 || next >= props.photos.length) {
    if (props.totalCount != null && globalTarget >= 0 && globalTarget < props.totalCount) {
      filmstripTargetRank.value = globalTarget;
      emit("seekRank", globalTarget);
      void scrollFilmstrip("smooth");
    } else if (delta > 0) {
      requestMorePhotos();
    }
    return;
  }

  loaded.value = false;
  playingLive.value = false;
  resetImageTransform();
  filmstripTargetRank.value = baseRank.value + next;
  emit("update:index", next);
}

function warmPhotoNeighborhood(): void {
  for (const offset of [-1, 1, 2, 3, 4]) {
    const item = props.photos[props.index + offset];
    if (!item) continue;
    const url = item.isVideo ? item.thumbUrl : item.displayUrl || item.thumbUrl;
    if (!url || warmedImages.has(url)) continue;
    const image = new Image();
    image.decoding = "async";
    image.fetchPriority = "low";
    image.src = url;
    warmedImages.set(url, image);
  }
  while (warmedImages.size > WARM_IMAGE_LIMIT) {
    const oldest = warmedImages.keys().next().value;
    if (oldest === undefined) break;
    warmedImages.delete(oldest);
  }
}
function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    if (infoOpen.value) infoOpen.value = false;
    else emit("close");
  } else if (event.key === "ArrowLeft") go(-1);
  else if (event.key === "ArrowRight") go(1);
  else if (event.key === "+" || event.key === "=") zoomStep(1);
  else if (event.key === "-" || event.key === "_") zoomStep(-1);
  else if (event.key === "0") resetImageTransform();
  else if (event.key === "[") rotateBy(-90);
  else if (event.key === "]") rotateBy(90);
  else if (event.key.toLowerCase() === "f") toggleFlip();
  else if (event.key.toLowerCase() === "i") infoOpen.value = !infoOpen.value;
}

watch(
  () => props.index,
  (next) => {
    if (next >= props.photos.length - PHOTO_PREFETCH_COUNT) requestMorePhotos();
    void scrollFilmstrip();
  },
);

watch(
  () => props.photos.length,
  () => {
    warmPhotoNeighborhood();
  },
);

watch(
  () => filmstripViewportWidth.value,
  () => {
    if (!filmstripUserScrolling) void scrollFilmstrip("auto");
  },
);

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  window.addEventListener("resize", updateStageSize);
  updateStageSize();
  void scrollFilmstrip("auto");
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKeydown);
  window.removeEventListener("resize", updateStageSize);
  endDrag();
  if (filmstripFrame !== null) window.cancelAnimationFrame(filmstripFrame);
  clearFilmstripSettle();
  if (filmstripProgrammaticTimer !== null) window.clearTimeout(filmstripProgrammaticTimer);
  if (needMoreTimer !== null) window.clearTimeout(needMoreTimer);
  if (filmstripPrefetchTimer !== null) window.clearTimeout(filmstripPrefetchTimer);
  if (displayLoadTimer !== null) window.clearTimeout(displayLoadTimer);
  displayLoadToken += 1;
  if (activeDisplayPreloader !== null) {
    activeDisplayPreloader.onload = null;
    activeDisplayPreloader.onerror = null;
    activeDisplayPreloader.src = "";
  }
  warmedImages.clear();
});

async function handleDownload(): Promise<void> {
  const target = photo.value;
  const url = originalUrl.value;
  if (!target || !url || saving.value) return;
  const name = target.fileName || `${target.id}${isVideo.value ? ".mp4" : ".jpeg"}`;
  saving.value = true;
  try {
    const savedPath = await downloadPhoto(url, name, t("photos.saveTo"));

    if (savedPath) toast.success(t("photos.savedTo", { path: savedPath }));
  } catch (caught) {
    toast.error(t("photos.downloadFailed", { error: caught instanceof Error ? caught.message : String(caught) }));
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div v-if="photo" class="fixed top-8 right-0 bottom-0 left-0 z-[2000] flex flex-col bg-background/95 backdrop-blur-sm">
    <header class="flex h-11 shrink-0 items-center gap-3 border-b border-border px-4 select-none">
      <div class="flex min-w-0 items-center gap-2.5 text-sm text-muted-foreground">
        <span class="tabular shrink-0">{{ filmstripTargetRank + 1 }}/{{ filmstripCount }}</span>
        <span v-if="time" class="truncate">{{ time }}</span>
        <span v-if="fileName" class="truncate">{{ fileName }}</span>
        <span v-if="isVideo" class="shrink-0">{{ t("common.video") }}</span>
        <span v-else-if="isLivePhoto" class="shrink-0">{{ t("common.live") }}</span>
      </div>

      <div class="ml-auto flex shrink-0 items-center gap-1">
        <template v-if="!isVideo && !playingLive">
          <Button variant="ghost" size="icon" class="size-8" :title="t('viewer.zoomOut')" :aria-label="t('viewer.zoomOut')" :disabled="scale <= MIN_SCALE" @click="zoomStep(-1)">
            <ZoomOut :size="15" />
          </Button>
          <button type="button" class="tabular h-7 min-w-12 rounded-md px-1 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50" :title="t('viewer.clickReset')" :disabled="scale <= MIN_SCALE" @click="resetZoom">
            {{ Math.round(scale * 100) }}%
          </button>
          <Button variant="ghost" size="icon" class="size-8" :title="t('viewer.zoomIn')" :aria-label="t('viewer.zoomIn')" :disabled="scale >= MAX_SCALE" @click="zoomStep(1)">
            <ZoomIn :size="15" />
          </Button>
          <Button variant="ghost" size="icon" class="size-8" :title="t('viewer.resetView')" :aria-label="t('viewer.resetView')" :disabled="scale <= MIN_SCALE && rotation === 0 && !flipped" @click="resetImageTransform">
            <Maximize2 :size="15" />
          </Button>
          <span class="mx-1 h-4 w-px bg-border" />
          <Button variant="ghost" size="icon" class="size-8" :title="t('viewer.rotateLeft')" :aria-label="t('viewer.rotateLeft')" @click="rotateBy(-90)">
            <RotateCcw :size="15" />
          </Button>
          <Button variant="ghost" size="icon" class="size-8" :title="t('viewer.rotateRight')" :aria-label="t('viewer.rotateRight')" @click="rotateBy(90)">
            <RotateCw :size="15" />
          </Button>
          <Button variant="ghost" size="icon" class="size-8" :class="flipped ? 'bg-accent text-foreground' : ''" :title="t('viewer.flip')" :aria-label="t('viewer.flip')" @click="toggleFlip">
            <FlipHorizontal2 :size="15" />
          </Button>
          <span class="mx-1 h-4 w-px bg-border" />
        </template>

        <Button v-if="canPlayLive" variant="ghost" size="icon" :title="t(playingLive ? 'viewer.pauseLive' : 'viewer.playLive')" :aria-label="t(playingLive ? 'viewer.pauseLive' : 'viewer.playLive')" @click="playingLive = !playingLive">
          <Pause v-if="playingLive" :size="15" />
          <Play v-else :size="15" />
        </Button>

        <Button variant="ghost" size="icon" class="size-8" :class="infoOpen ? 'bg-accent text-foreground' : ''" :title="t('viewer.info')" :aria-label="t('viewer.info')" :aria-pressed="infoOpen" @click="infoOpen = !infoOpen">
          <InfoIcon :size="15" />
        </Button>

        <Button v-if="originalUrl" variant="ghost" size="icon" :disabled="saving" :title="t(isVideo ? 'photos.downloadVideo' : 'photos.downloadOriginal')" :aria-label="t(isVideo ? 'photos.downloadVideo' : 'photos.downloadOriginal')" @click="handleDownload">
          <Spinner v-if="saving" :size="14" />
          <Download v-else :size="15" />
        </Button>

        <Button variant="ghost" size="icon" :title="t('common.close')" :aria-label="t('common.close')" @click="emit('close')">
          <X :size="15" />
        </Button>
      </div>
    </header>

    <div class="relative flex min-h-0 flex-1 flex-col">
      <div class="relative min-h-0 flex-1">
        <div ref="stage" class="absolute inset-0 flex items-center justify-center overflow-hidden p-4" :class="dragging ? 'cursor-grabbing' : scale > MIN_SCALE ? 'cursor-grab' : ''" @click.self="emit('close')" @wheel.prevent="onWheel">
          <img class="pointer-events-none absolute inset-0 m-auto max-h-full max-w-full scale-105 object-contain opacity-40 blur-2xl" :src="photo.thumbUrl" alt="" aria-hidden="true" />

          <video
            v-if="isVideo && videoUrl"
            class="relative max-h-full max-w-full rounded-md object-contain"
            :class="photo.width && photo.height ? '' : 'h-full w-full'"
            :src="videoUrl"
            :poster="photo.thumbUrl"
            :width="photo.width ?? undefined"
            :height="photo.height ?? undefined"
            controls
            autoplay
            playsinline
            preload="metadata"
          />
          <video
            v-else-if="playingLive && videoUrl"
            ref="liveVideo"
            class="relative max-h-full max-w-full rounded-md object-contain"
            :src="videoUrl"
            :poster="photo.thumbUrl"
            :width="photo.width ?? undefined"
            :height="photo.height ?? undefined"
            controls
            autoplay
            playsinline
            preload="auto"
            @ended="playingLive = false"
          />
          <template v-else>
            <img
              v-if="!loaded"
              class="relative max-h-full max-w-full rounded-md object-contain opacity-80"
              :class="dragging ? '' : 'transition-transform duration-100'"
              :style="imageStyle"
              :src="photo.thumbUrl"
              :alt="fileName || t('photos.photoNameFallback')"
              decoding="async"
              draggable="false"
              @mousedown.prevent="startDrag"
              @dblclick="toggleZoom"
            />
            <img
              v-else
              class="relative max-h-full max-w-full rounded-md object-contain transition-opacity duration-150"
              :class="dragging ? '' : 'transition-transform duration-100'"
              :style="imageStyle"
              :src="displayUrl"
              :alt="fileName || t('photos.photoNameFallback')"
              decoding="async"
              fetchpriority="high"
              draggable="false"
              @mousedown.prevent="startDrag"
              @dblclick="toggleZoom"
            />
          </template>
        </div>

        <button
          type="button"
          class="absolute top-1/2 left-4 z-10 grid size-10 -translate-y-1/2 place-items-center rounded-full border border-border bg-elevated/85 text-elevated-foreground backdrop-blur transition-colors enabled:hover:bg-elevated disabled:opacity-30"
          :aria-label="t('viewer.previousPhoto')"
          :disabled="atFirstPhoto"
          @click="go(-1)"
        >
          <ChevronLeft :size="20" />
        </button>

        <button
          type="button"
          class="absolute top-1/2 right-4 z-10 grid size-10 -translate-y-1/2 place-items-center rounded-full border border-border bg-elevated/85 text-elevated-foreground backdrop-blur transition-colors enabled:hover:bg-elevated disabled:opacity-30"
          :aria-label="t('viewer.nextPhoto')"
          :disabled="atLastPhoto"
          @click="go(1)"
        >
          <ChevronRight :size="20" />
        </button>
      </div>

      <div class="relative h-[62px] shrink-0 border-t border-border bg-elevated/95 shadow-[0_-6px_18px_rgba(0,0,0,0.07)]">
        <div
          ref="filmstrip"
          class="relative block h-full select-none overflow-x-auto overscroll-x-contain py-1 [scrollbar-width:none] [touch-action:pan-y] [&::-webkit-scrollbar]:hidden"
          :class="filmstripDragging ? 'cursor-grabbing' : 'cursor-grab'"
          role="group"
          :aria-label="t('viewer.thumbnailStrip')"
          @scroll.passive="onFilmstripScroll"
          @wheel="onFilmstripWheel"
          @pointerdown="onFilmstripPointerDown"
          @pointermove="onFilmstripPointerMove"
          @pointerup="finishFilmstripPointer"
          @pointercancel="finishFilmstripPointer"
          @dragstart.prevent
        >
          <div class="relative h-full" :style="{ width: `${filmstripTotalWidth}px` }">
            <button
              v-for="item in renderedFilmstripItems"
              :key="item.rank"
              type="button"
              class="group absolute top-1/2 h-[46px] -translate-y-1/2 overflow-hidden border bg-muted transition-[border-color,opacity,box-shadow] duration-200"
              :class="item.rank === filmstripTargetRank ? 'rounded-sm border-foreground/30 opacity-100 shadow-sm' : 'rounded-[2px] border-border/70 opacity-65 hover:border-border hover:opacity-100'"
              :style="filmstripItemStyle(item)"
              :data-filmstrip-index="item.rank"
              :aria-label="t('viewer.selectPhoto', { index: item.rank + 1 })"
              :aria-current="item.rank === filmstripTargetRank ? 'true' : undefined"
              :title="item.photo?.fileName || `#${item.rank + 1}`"
              @click="onFilmstripClick($event, item.rank)"
            >
              <img
                v-if="item.photo && !filmstripFailed.has(item.photo.id) && filmstripImageUrl(item.photo)"
                :src="filmstripImageUrl(item.photo)"
                alt=""
                class="h-full w-full object-cover"
                :loading="Math.abs(item.rank - filmstripTargetRank) <= 12 ? 'eager' : 'lazy'"
                :fetchpriority="item.rank === filmstripTargetRank ? 'high' : Math.abs(item.rank - filmstripTargetRank) <= 12 ? 'auto' : 'low'"
                decoding="async"
                draggable="false"
                @error="onFilmstripImageError($event, item.photo)"
              />
              <div v-else class="grid h-full w-full place-items-center bg-muted">
                <span class="size-1.5 rounded-full bg-muted-foreground/30" />
              </div>
              <span v-if="item.photo?.isVideo || item.photo?.isLivePhoto" class="absolute right-0.5 bottom-0.5 grid size-3 place-items-center rounded-full bg-black/65 text-white shadow-sm">
                <Play :size="7" fill="currentColor" />
              </span>
            </button>
          </div>
        </div>
        <div class="pointer-events-none absolute inset-y-0 left-0 z-10 w-10 bg-gradient-to-r from-elevated via-elevated/75 to-transparent" aria-hidden="true"></div>
        <div class="pointer-events-none absolute inset-y-0 right-0 z-10 w-10 bg-gradient-to-l from-elevated via-elevated/75 to-transparent" aria-hidden="true"></div>
      </div>

      <Transition name="photo-info">
        <PhotoInfoPanel v-if="infoOpen" :photo="photo" :position="currentGlobalRank + 1" :total="filmstripCount" @close="infoOpen = false" />
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.photo-info-enter-active,
.photo-info-leave-active {
  transition:
    opacity 150ms ease,
    transform 180ms cubic-bezier(0.22, 0.8, 0.2, 1);
}

.photo-info-enter-from,
.photo-info-leave-to {
  opacity: 0;
  transform: translateX(12px);
}
</style>
