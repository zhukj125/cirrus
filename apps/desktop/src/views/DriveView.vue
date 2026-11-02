<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { save } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, ChevronRight, Download, File, Folder, HardDrive, RefreshCw } from "@lucide/vue";

import Spinner from "@/components/ui/Spinner.vue";
import { Button } from "@/components/ui/button";
import { driveDownload, driveList } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { useLocaleStore } from "@/stores/locale";
import type { DriveItem } from "@/types";

const props = defineProps<{ sessionId: string }>();

const ITEM_ROW_HEIGHT = 56;
const VIRTUAL_OVERSCAN = 6;

const locale = useLocaleStore();
const items = ref<DriveItem[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const downloadingId = ref<string | null>(null);
const path = ref<Array<{ id: string; name: string }>>([{ id: "root", name: locale.t("drive.root") }]);
const scrollContainer = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewportHeight = ref(0);

const currentFolder = computed(() => path.value[path.value.length - 1]!);
const canGoBack = computed(() => path.value.length > 1);
const virtualWindow = computed(() => {
  const total = items.value.length;
  const viewport = viewportHeight.value || ITEM_ROW_HEIGHT * 10;
  const firstVisible = Math.floor(scrollTop.value / ITEM_ROW_HEIGHT);
  const start = Math.max(0, firstVisible - VIRTUAL_OVERSCAN);
  const end = Math.min(total, firstVisible + Math.ceil(viewport / ITEM_ROW_HEIGHT) + VIRTUAL_OVERSCAN * 2);
  return { start, end, totalHeight: total * ITEM_ROW_HEIGHT };
});
const visibleRows = computed(() => items.value.slice(virtualWindow.value.start, virtualWindow.value.end).map((item, offset) => ({ item, index: virtualWindow.value.start + offset })));

function isFolder(item: DriveItem): boolean {
  return item.kind === "folder" || item.kind === "app-library";
}

function itemName(item: DriveItem): string {
  return item.extension && !item.name.toLowerCase().endsWith(`.${item.extension.toLowerCase()}`) ? `${item.name}.${item.extension}` : item.name;
}

function formatSize(bytes: number | null): string {
  if (bytes == null || !Number.isFinite(bytes)) return "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 10 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

function formatModified(value: string | null): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat(locale.formatLocale, { dateStyle: "medium" }).format(date);
}

function measureViewport(): void {
  const container = scrollContainer.value;
  if (!container) return;
  viewportHeight.value = container.clientHeight;
  scrollTop.value = container.scrollTop;
}

function handleScroll(event: Event): void {
  scrollTop.value = (event.currentTarget as HTMLElement).scrollTop;
}

function resetScroll(): void {
  scrollTop.value = 0;
  if (scrollContainer.value) scrollContainer.value.scrollTop = 0;
}

async function load(folderId = currentFolder.value.id): Promise<void> {
  if (loading.value) return;
  loading.value = true;
  error.value = null;
  try {
    const result = await driveList({ sessionId: props.sessionId, folderId });
    if (result.status !== "OK") throw new Error(result.message || locale.t("drive.loadFailed"));
    items.value = result.items;
  } catch (cause) {
    items.value = [];
    error.value = cause instanceof Error ? cause.message : locale.t("drive.loadFailed");
  } finally {
    loading.value = false;
  }
}

async function openItem(item: DriveItem): Promise<void> {
  if (isFolder(item)) {
    resetScroll();
    path.value.push({ id: item.id, name: item.name });
    await load(item.id);
    return;
  }
  await downloadItem(item);
}

async function downloadItem(item: DriveItem): Promise<void> {
  if (downloadingId.value) return;
  if (!item.downloadUrl) {
    toast.error(locale.t("drive.noDownload"));
    return;
  }
  const name = itemName(item);
  const targetPath = await save({ title: locale.t("drive.saveFile"), defaultPath: name });
  if (!targetPath) return;
  downloadingId.value = item.id;
  try {
    const result = await driveDownload({ sessionId: props.sessionId, url: item.downloadUrl, name, targetPath });
    toast.success(locale.t("drive.downloadComplete", { file: result.fileName }));
  } catch (cause) {
    toast.error(locale.t("drive.downloadFailed", { error: cause instanceof Error ? cause.message : locale.t("app.unknownError") }));
  } finally {
    downloadingId.value = null;
  }
}

async function goBack(): Promise<void> {
  if (!canGoBack.value) return;
  resetScroll();
  path.value.pop();
  await load();
}

async function goTo(index: number): Promise<void> {
  if (index < 0 || index >= path.value.length - 1) return;
  resetScroll();
  path.value = path.value.slice(0, index + 1);
  await load();
}

let resizeObserver: ResizeObserver | null = null;

onMounted(() => {
  measureViewport();
  if (typeof ResizeObserver !== "undefined" && scrollContainer.value) {
    resizeObserver = new ResizeObserver(measureViewport);
    resizeObserver.observe(scrollContainer.value);
  }
  void load();
});

onBeforeUnmount(() => resizeObserver?.disconnect());
</script>

<template>
  <section class="flex h-full min-h-0 flex-col bg-background">
    <header class="flex min-h-12 shrink-0 flex-wrap items-center gap-2 border-b border-border px-4 py-2">
      <Button v-if="canGoBack" variant="ghost" size="sm" class="h-8 px-2" :aria-label="locale.t('drive.back')" @click="goBack">
        <ArrowLeft :size="15" />
      </Button>
      <HardDrive :size="17" class="shrink-0 text-primary" />
      <nav class="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto" :aria-label="locale.t('nav.drive')">
        <template v-for="(segment, index) in path" :key="segment.id">
          <ChevronRight v-if="index > 0" :size="13" class="shrink-0 text-muted-foreground/50" />
          <button type="button" class="max-w-48 truncate rounded px-1.5 py-1 text-sm transition-colors hover:bg-accent" :class="index === path.length - 1 ? 'font-medium text-foreground' : 'text-muted-foreground'" @click="goTo(index)">
            {{ segment.name }}
          </button>
        </template>
      </nav>
      <span class="text-xs text-muted-foreground">{{ locale.t("drive.items", { count: items.length }) }}</span>
      <Button variant="ghost" size="sm" class="h-8 px-2" :disabled="loading" :aria-label="locale.t('common.retry')" @click="load()">
        <RefreshCw :size="15" :class="loading ? 'animate-spin' : ''" />
      </Button>
    </header>

    <div ref="scrollContainer" class="min-h-0 flex-1 overflow-y-auto p-3" @scroll.passive="handleScroll">
      <div v-if="loading && items.length === 0" class="grid h-full place-items-center">
        <Spinner :size="22" />
      </div>

      <div v-else-if="error" class="flex h-full flex-col items-center justify-center gap-3 text-center">
        <HardDrive :size="30" class="text-muted-foreground/60" />
        <p class="max-w-md text-sm text-muted-foreground">{{ error }}</p>
        <Button variant="outline" size="sm" @click="load()">{{ locale.t("common.retry") }}</Button>
      </div>

      <div v-else-if="items.length === 0" class="flex h-full flex-col items-center justify-center gap-2 text-center text-muted-foreground">
        <Folder :size="32" class="text-muted-foreground/50" />
        <p class="text-sm">{{ locale.t("drive.empty") }}</p>
      </div>

      <div v-else class="relative mx-auto max-w-5xl overflow-hidden rounded-lg border border-border bg-card" :style="{ height: `${virtualWindow.totalHeight}px` }">
        <button
          v-for="{ item, index } in visibleRows"
          :key="item.id"
          type="button"
          data-testid="drive-item"
          class="absolute inset-x-0 top-0 flex h-14 items-center gap-3 px-4 text-left transition-colors hover:bg-accent/60 disabled:opacity-60"
          :class="index === items.length - 1 ? 'border-b-0' : 'border-b border-border'"
          :style="{ transform: `translateY(${index * ITEM_ROW_HEIGHT}px)` }"
          :disabled="!isFolder(item) && (!item.downloadUrl || downloadingId !== null)"
          @click="openItem(item)"
        >
          <span class="grid size-9 shrink-0 place-items-center rounded-lg" :class="isFolder(item) ? 'bg-primary/10 text-primary' : 'bg-secondary text-muted-foreground'">
            <Folder v-if="isFolder(item)" :size="19" />
            <File v-else :size="18" />
          </span>
          <span class="min-w-0 flex-1">
            <span class="block truncate text-sm font-medium">{{ itemName(item) }}</span>
            <span class="mt-0.5 flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
              <span v-if="isFolder(item) && item.childCount != null">{{ locale.t("drive.items", { count: item.childCount }) }}</span>
              <span v-else-if="formatSize(item.sizeBytes)">{{ formatSize(item.sizeBytes) }}</span>
              <span v-if="formatModified(item.dateModified)" class="truncate">{{ formatModified(item.dateModified) }}</span>
            </span>
          </span>
          <Spinner v-if="downloadingId === item.id" :size="16" />
          <Download v-else-if="!isFolder(item) && item.downloadUrl" :size="16" class="shrink-0 text-muted-foreground" />
          <ChevronRight v-else-if="isFolder(item)" :size="16" class="shrink-0 text-muted-foreground" />
        </button>
      </div>
    </div>
  </section>
</template>
