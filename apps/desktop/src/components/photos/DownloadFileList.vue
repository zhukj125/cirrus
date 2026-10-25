<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { AlertTriangle, Check, Pause, Play } from "@lucide/vue";

import Spinner from "@/components/ui/Spinner.vue";
import type { DownloadFileRow } from "@/lib/downloads/file-row";
import { formatBytes } from "@/stores/downloads";
import { useLocaleStore } from "@/stores/locale";

const emit = defineEmits<{ pause: [photoId: string]; resume: [photoId: string] }>();
const locale = useLocaleStore();
const t = locale.t;

const props = withDefaults(
  defineProps<{
    rows: DownloadFileRow[];
    height?: number;
    itemHeight?: number;
    emptyLabel?: string;
    fill?: boolean;
  }>(),
  {
    height: 240,
    itemHeight: 36,
    fill: false,
  },
);

const OVERSCAN = 4;
const scroller = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewportHeight = ref(props.height);
let resizeObserver: ResizeObserver | null = null;

function measureViewport(): void {
  viewportHeight.value = scroller.value?.clientHeight || props.height;
}

onMounted(() => {
  measureViewport();
  if (typeof ResizeObserver === "undefined" || !scroller.value) return;
  resizeObserver = new ResizeObserver(measureViewport);
  resizeObserver.observe(scroller.value);
});

onUnmounted(() => resizeObserver?.disconnect());
watch(
  () => props.height,
  () => {
    if (!props.fill) viewportHeight.value = props.height;
  },
);

const visibleCount = computed(() => Math.ceil(viewportHeight.value / props.itemHeight) + OVERSCAN * 2);
const start = computed(() => Math.max(0, Math.floor(scrollTop.value / props.itemHeight) - OVERSCAN));
const end = computed(() => Math.min(props.rows.length, start.value + visibleCount.value));
const visibleRows = computed(() => props.rows.slice(start.value, end.value));
const topSpace = computed(() => start.value * props.itemHeight);
const bottomSpace = computed(() => Math.max(0, (props.rows.length - end.value) * props.itemHeight));

function onScroll(event: Event): void {
  scrollTop.value = (event.currentTarget as HTMLElement).scrollTop;
}

watch(
  () => props.rows,
  () => {
    const maxScroll = Math.max(0, props.rows.length * props.itemHeight - viewportHeight.value);
    if (scrollTop.value <= maxScroll) return;
    scrollTop.value = maxScroll;
    if (scroller.value) scroller.value.scrollTop = maxScroll;
  },
);
</script>

<template>
  <div ref="scroller" data-testid="download-file-list" class="overflow-y-auto rounded-md border border-border bg-background" :style="fill ? undefined : { height: `${height}px` }" @scroll.passive="onScroll">
    <div v-if="rows.length === 0" class="grid h-full place-items-center px-4 text-center text-xs text-muted-foreground">{{ emptyLabel || t("downloads.noFiles") }}</div>
    <template v-else>
      <div :style="{ height: `${topSpace}px` }" aria-hidden="true" />
      <div v-for="row in visibleRows" :key="row.key" data-testid="download-file-row" class="flex min-w-0 items-center gap-2 border-b border-border/70 px-3 text-xs last:border-b-0" :style="{ height: `${itemHeight}px` }">
        <span v-if="row.ordinal !== undefined" class="tabular w-7 shrink-0 text-right text-2xs text-muted-foreground">{{ row.ordinal }}</span>
        <Spinner v-if="row.status === 'active'" :size="12" class="shrink-0 text-primary" />
        <Pause v-else-if="row.status === 'paused'" :size="12" class="shrink-0 text-warning" />
        <Check v-else-if="row.status === 'completed'" :size="12" :stroke-width="3" class="shrink-0 text-success" />
        <AlertTriangle v-else-if="row.status === 'failed'" :size="12" class="shrink-0 text-warning" />
        <span v-else class="size-1.5 shrink-0 rounded-full bg-muted-foreground/45" />
        <span class="min-w-0 flex-1 truncate" :class="row.status === 'failed' ? 'text-warning' : ''" :title="row.name">{{ row.name }}</span>
        <span v-if="row.status === 'active' || row.status === 'paused'" class="tabular shrink-0 text-2xs text-muted-foreground"
          >{{ formatBytes(row.downloadedBytes ?? 0) }}<template v-if="row.totalBytes !== null && row.totalBytes !== undefined"> / {{ formatBytes(row.totalBytes) }}</template></span
        >
        <span v-else class="tabular shrink-0 text-2xs text-muted-foreground">{{ formatBytes(row.sizeBytes) }}</span>
        <button
          v-if="(row.status === 'active' || row.status === 'pending') && row.photoId"
          type="button"
          class="grid size-6 shrink-0 place-items-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
          :title="t('downloads.pauseFile')"
          :aria-label="t('downloads.pauseFile')"
          @click.stop="emit('pause', row.photoId)"
        >
          <Pause :size="12" />
        </button>
        <button
          v-else-if="row.status === 'paused' && row.photoId"
          type="button"
          class="grid size-6 shrink-0 place-items-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
          :title="t('downloads.resumeFile')"
          :aria-label="t('downloads.resumeFile')"
          @click.stop="emit('resume', row.photoId)"
        >
          <Play :size="12" />
        </button>
      </div>
      <div :style="{ height: `${bottomSpace}px` }" aria-hidden="true" />
    </template>
  </div>
</template>
