<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { ChevronLeft, ChevronRight, Download, FlipHorizontal2, Info as InfoIcon, Maximize2, Pause, Play, RotateCcw, RotateCw, X, ZoomIn, ZoomOut } from "@lucide/vue";

import { downloadPhoto } from "@/lib/api";
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
}>();
const emit = defineEmits<{ "update:index": [value: number]; close: []; needMore: [] }>();
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

const FILMSTRIP_RADIUS = 60;
const FILMSTRIP_SIZE = FILMSTRIP_RADIUS * 2 + 1;

const filmstripItems = computed(() => {
  const maxStart = Math.max(0, props.photos.length - FILMSTRIP_SIZE);
  const start = Math.min(Math.max(0, props.index - FILMSTRIP_RADIUS), maxStart);
  const end = Math.min(props.photos.length, start + FILMSTRIP_SIZE);
  return props.photos.slice(start, end).map((item, offset) => ({ photo: item, index: start + offset }));
});

function filmstripItemWidth(photo: PhotoItem): string {
  const ratio = photo.width && photo.height ? photo.width / photo.height : 0.65;
  return `${Math.round(Math.min(72, Math.max(20, 46 * ratio)))}px`;
}

const saving = ref(false);
const infoOpen = ref(false);

const photo = computed<PhotoItem | null>(() => {
  const item: PhotoItem | undefined = props.photos[props.index];
  return item ?? null;
});

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
  if (!element) return;
  stageSize.value = {
    width: Math.max(0, element.clientWidth - 32),
    height: Math.max(0, element.clientHeight - 32),
  };
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

watch(
  () => photo.value?.id,
  async () => {
    loaded.value = false;
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

async function scrollFilmstrip(behavior: ScrollBehavior = "smooth"): Promise<void> {
  await nextTick();
  const strip = filmstrip.value;
  const item = strip?.querySelector<HTMLElement>(`[data-filmstrip-index="${props.index}"]`);
  if (!strip || !item || typeof strip.scrollTo !== "function") return;
  const stripRect = strip.getBoundingClientRect();
  const itemRect = item.getBoundingClientRect();
  const centeredLeft = strip.scrollLeft + itemRect.left - stripRect.left - (stripRect.width - itemRect.width) / 2;
  strip.scrollTo({ left: Math.max(0, centeredLeft), behavior });
}

function selectFilmstripPhoto(next: number): void {
  if (next < 0 || next >= props.photos.length || next === props.index) return;
  loaded.value = false;
  playingLive.value = false;
  resetImageTransform();
  emit("update:index", next);
  if (props.hasMore && next >= props.photos.length - 12) emit("needMore");
}

function go(delta: number): void {
  const next = props.index + delta;
  if (next < 0) return;
  if (next >= props.photos.length) {
    if (delta > 0 && props.hasMore) emit("needMore");
    return;
  }

  loaded.value = false;
  playingLive.value = false;
  resetImageTransform();
  emit("update:index", next);
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
    if (props.hasMore && next >= props.photos.length - 12) emit("needMore");
    void scrollFilmstrip();
  },
);

watch(
  () => props.photos.length,
  () => void scrollFilmstrip("auto"),
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
        <span class="tabular shrink-0">{{ index + 1 }}/{{ photos.length }}</span>
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
          <img
            v-else
            class="relative max-h-full max-w-full rounded-md object-contain transition-opacity duration-200"
            :class="[loaded ? 'opacity-100' : 'opacity-0', dragging ? '' : 'transition-transform duration-100']"
            :style="imageStyle"
            :src="displayUrl"
            :alt="fileName || t('photos.photoNameFallback')"
            draggable="false"
            @load="loaded = true"
            @mousedown.prevent="startDrag"
            @dblclick="toggleZoom"
          />
        </div>

        <button
          type="button"
          class="absolute top-1/2 left-4 z-10 grid size-10 -translate-y-1/2 place-items-center rounded-full border border-border bg-elevated/85 text-elevated-foreground backdrop-blur transition-colors enabled:hover:bg-elevated disabled:opacity-30"
          :aria-label="t('viewer.previousPhoto')"
          :disabled="index === 0"
          @click="go(-1)"
        >
          <ChevronLeft :size="20" />
        </button>

        <button
          type="button"
          class="absolute top-1/2 right-4 z-10 grid size-10 -translate-y-1/2 place-items-center rounded-full border border-border bg-elevated/85 text-elevated-foreground backdrop-blur transition-colors enabled:hover:bg-elevated disabled:opacity-30"
          :aria-label="t('viewer.nextPhoto')"
          :disabled="index >= photos.length - 1 && !hasMore"
          @click="go(1)"
        >
          <ChevronRight :size="20" />
        </button>
      </div>

      <div class="relative h-[62px] shrink-0 border-t border-border bg-elevated/95 shadow-[0_-6px_18px_rgba(0,0,0,0.07)]">
        <div ref="filmstrip" class="relative flex h-full items-center gap-px overflow-x-auto px-1.5 py-1 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden" role="group" :aria-label="t('viewer.thumbnailStrip')">
          <button
            v-for="item in filmstripItems"
            :key="item.photo.id"
            type="button"
            class="group relative h-[46px] flex-none overflow-hidden border bg-muted transition-[margin,border-color,opacity,box-shadow] duration-200"
            :class="item.index === index ? 'mx-2 rounded-md border-foreground/30 opacity-100 shadow-sm' : 'rounded-[3px] border-border/70 opacity-65 hover:border-border hover:opacity-100'"
            :style="{ width: filmstripItemWidth(item.photo) }"
            :data-filmstrip-index="item.index"
            :aria-label="t('viewer.selectPhoto', { index: item.index + 1 })"
            :aria-current="item.index === index ? 'true' : undefined"
            :title="item.photo.fileName || t('photos.photoNameFallback')"
            @click="selectFilmstripPhoto(item.index)"
          >
            <img :src="item.photo.thumbUrl" alt="" class="h-full w-full object-cover" loading="lazy" decoding="async" draggable="false" />
            <span v-if="item.photo.isVideo || item.photo.isLivePhoto" class="absolute right-0.5 bottom-0.5 grid size-3 place-items-center rounded-full bg-black/65 text-white shadow-sm">
              <Play :size="7" fill="currentColor" />
            </span>
          </button>
        </div>
        <div class="pointer-events-none absolute inset-y-0 left-0 z-10 w-10 bg-gradient-to-r from-elevated via-elevated/75 to-transparent" aria-hidden="true"></div>
        <div class="pointer-events-none absolute inset-y-0 right-0 z-10 w-10 bg-gradient-to-l from-elevated via-elevated/75 to-transparent" aria-hidden="true"></div>
      </div>

      <Transition name="photo-info">
        <PhotoInfoPanel v-if="infoOpen" :photo="photo" :position="index + 1" :total="photos.length" @close="infoOpen = false" />
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
