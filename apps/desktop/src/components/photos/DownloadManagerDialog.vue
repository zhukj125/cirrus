<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { AlertTriangle, ArrowLeft, Check, Download, FolderOpen, ListChecks, Pause, Play, Settings2, Trash, X } from "@lucide/vue";

import { Status, StorageKey } from "@/constants";
import { openDownloadFolder, photosAssetDistribution, photosList, photosTotal, pickDownloadDirectory } from "@/lib/api";
import { readStored } from "@/lib/common/storage";
import { toast } from "@/lib/common/toast";
import { isSessionGoneMessage } from "@/lib/session-errors";
import { useAppStore } from "@/stores/app";
import { CONCURRENCY_OPTIONS, formatSpeed, percentOf, queueView, totalOf, useDownloadsStore, type DownloadTask } from "@/stores/downloads";
import { RANGE_OPTIONS, UNBOUNDED, addDays, formatDateInput, presetRange, type DateBounds, type RangeKey } from "@/lib/photos/dates";
import { segmentLanding, type SegmentLanding } from "@/lib/photos/distribution";
import { createExactRangeLoader, createExactRangeStore, type ExactRangeResult } from "@/lib/photos/exact";
import { scanRangePage } from "@/lib/photos/range";
import { DateRangePicker } from "@/components/ui/date-range-picker";
import { Button } from "@/components/ui/button";
import Spinner from "@/components/ui/Spinner.vue";
import DownloadEmptyState from "@/components/photos/DownloadEmptyState.vue";
import type { AssetDistribution, LibraryTotal, PhotoItem } from "@/types";

const downloads = useDownloadsStore();
const app = useAppStore();

const canResumeTasks = computed(() => Boolean(app.photosSession && app.activeAccountKey && downloads.activeAccountKey === app.activeAccountKey));

const settingsOpen = ref(false);
const speedLimitMb = ref("");
const headerTitle = computed(() => (settingsOpen.value ? "Download Settings" : "Download Photos"));

function openSettings(): void {
  speedLimitMb.value = downloads.speedLimitBytesPerSec > 0 ? String(Number((downloads.speedLimitBytesPerSec / (1024 * 1024)).toFixed(2))) : "";
  settingsOpen.value = true;
}

function saveSettings(): void {
  const mb = speedLimitMb.value.trim() === "" ? 0 : Number(speedLimitMb.value);
  if (!Number.isFinite(mb) || mb < 0) {
    toast.error("Enter a speed limit of 0 or greater");
    return;
  }
  downloads.setSpeedLimit(Math.round(mb * 1024 * 1024));
  toast.success(mb > 0 ? `Global speed limit set to ${mb} MB/s` : "Global speed limit removed");
  settingsOpen.value = false;
}

const PAGE_SIZE = 60;

const MAX_PAGES = 100;

const SLOW_SEARCH_MS = 5000;

const TASK_PAGE_SIZE = 10;
const taskPage = ref(1);
const QUEUE_PREVIEW_LIMIT = 200;
const detailsTaskId = ref<string | null>(null);

const totalPages = computed(() => Math.max(1, Math.ceil(downloads.tasks.length / TASK_PAGE_SIZE)));

const pagedTasks = computed(() => downloads.tasks.slice((taskPage.value - 1) * TASK_PAGE_SIZE, taskPage.value * TASK_PAGE_SIZE));
const detailsTask = computed(() => downloads.tasks.find((task) => task.id === detailsTaskId.value) ?? null);
const detailsQueue = computed(() => (detailsTask.value ? queueView(detailsTask.value, QUEUE_PREVIEW_LIMIT) : null));
const detailsPendingPreview = computed(() => detailsQueue.value?.pending ?? []);
const detailsPendingHidden = computed(() => Math.max(0, (detailsQueue.value?.pendingTotal ?? 0) - detailsPendingPreview.value.length));

function openTaskDetails(task: DownloadTask): void {
  detailsTaskId.value = task.id;
}

function taskStatusLabel(task: DownloadTask): string {
  if (task.status === "running") return "Downloading";
  if (task.status === "paused") return "Paused";
  if (task.status === "done") return "Completed";
  return task.error ? "Failed" : "Cancelled";
}

watch(totalPages, (pages) => {
  if (taskPage.value > pages) taskPage.value = pages;
});

watch(
  () => downloads.tasks[0]?.id,
  () => {
    taskPage.value = 1;
  },
);

function goToPage(next: number): void {
  taskPage.value = Math.min(totalPages.value, Math.max(1, next));
}

const requestedPhotos = downloads.consumeRequestedPhotos();
const requestedBounds = requestedPhotos ? null : downloads.consumeRequestedBounds();
const selectionMode = ref(Boolean(requestedPhotos?.length));
const selectedItems = ref<PhotoItem[]>(requestedPhotos ? [...requestedPhotos] : []);
const range = ref<RangeKey>(requestedBounds ? "custom" : "all");
const customBounds = ref<DateBounds>(requestedBounds ? { ...requestedBounds } : UNBOUNDED);
const bounds = ref<DateBounds>(requestedBounds ? { ...requestedBounds } : UNBOUNDED);
const hasCustomRange = computed(() => range.value === "custom" && customBounds.value.after !== null && customBounds.value.before !== null);
const showFoundStatus = computed(() => range.value !== "custom" || hasCustomRange.value || searching.value);

const storedConcurrency = Number(readStored(StorageKey.downloadConcurrency));
const concurrency = CONCURRENCY_OPTIONS.includes(storedConcurrency as (typeof CONCURRENCY_OPTIONS)[number]) ? storedConcurrency : 4;

const found = ref<PhotoItem[]>([]);
const searching = ref(false);

const searchSlow = ref(false);
const truncated = ref(false);

const handedToTask = ref(false);

const total = ref<LibraryTotal | null>(null);
const totalLoading = ref(false);

const distribution = ref<AssetDistribution | null>(null);

const rangeLanding = ref<SegmentLanding | null>(null);

async function loadExactDates(startRank: number, limit: number): Promise<readonly (number | null)[]> {
  const sessionId = app.photosSession;
  if (!sessionId) throw new Error("Photo session is unavailable");
  const page = await photosList({ sessionId, startRank, limit });
  if (page.status === Status.error) throw new Error(page.message ?? "Could not load photos by rank");
  return page.photos.map((photo) => photo.takenAt);
}

let exactReaderDistribution: AssetDistribution | null = null;
let exactReader: ((bounds: DateBounds) => Promise<ExactRangeResult>) | null = null;

function readExactRange(bounds: DateBounds): Promise<ExactRangeResult> {
  const dist = distribution.value;
  if (!dist) return Promise.reject(new Error("Photo timeline is unavailable"));
  if (exactReaderDistribution !== dist || exactReader === null) {
    exactReaderDistribution = dist;
    exactReader = createExactRangeLoader(dist, loadExactDates);
  }
  return exactReader(bounds);
}

const exactRanges = createExactRangeStore(readExactRange);

const rangeExact = computed(() => (range.value === "all" ? null : exactRanges.cached(bounds.value)));

const exactPending = computed(() => range.value !== "all" && exactRanges.isPending(bounds.value));

let searchEpoch = 0;
let slowTimer: ReturnType<typeof setTimeout> | null = null;

function clearSlowTimer(): void {
  if (slowTimer === null) return;
  clearTimeout(slowTimer);
  slowTimer = null;
}

const rangeLabel = computed(() => {
  const { after, before } = bounds.value;
  if (after === null || before === null) return null;
  return `${formatDateInput(after)} to ${formatDateInput(addDays(before, -1))}`;
});
const scopeLabel = computed(() => (selectionMode.value ? `${selectedItems.value.length} selected photos` : rangeLabel.value ? `Range ${rangeLabel.value}` : "All photos"));

const plannedTotal = computed<number | null>(() => {
  if (selectionMode.value) return selectedItems.value.length;
  if (range.value === "all") return total.value?.assets ?? null;
  if (rangeExact.value !== null) return rangeExact.value.count;
  if (searching.value || handedToTask.value) return null;
  return found.value.length;
});

const plannedLabel = computed(() => (plannedTotal.value === null ? null : `${plannedTotal.value} photos`));
const taskTitle = computed(() => (plannedLabel.value === null ? scopeLabel.value : `${scopeLabel.value} · ${plannedLabel.value}`));
const createLabel = computed(() => (selectionMode.value ? "Download Selected" : "Create Task"));

const foundLabel = computed(() => {
  if (selectionMode.value) return `${selectedItems.value.length} photos selected and ready to download`;
  if (range.value === "all") {
    if (totalLoading.value) return "Loading total…";
    return total.value === null ? "Total unknown; scanning while downloading" : `${total.value.assets} photos`;
  }

  if (rangeExact.value !== null) return `${rangeExact.value.count} photos`;
  if (exactPending.value) return "Calculating exact count…";
  if (handedToTask.value) return "The task will continue scanning this range";
  if (searching.value) return `Searching… ${found.value.length} found`;
  return truncated.value ? `${found.value.length} found (range not fully scanned)` : `${found.value.length} photos`;
});

async function loadTotal(): Promise<void> {
  const sessionId = app.photosSession;
  if (!sessionId) return;
  totalLoading.value = true;
  try {
    const result = await photosTotal(sessionId);

    if (sessionId !== app.photosSession) return;
    total.value = result;
  } catch {
    if (sessionId !== app.photosSession) return;
    total.value = null;
  } finally {
    if (sessionId === app.photosSession) totalLoading.value = false;
  }
}

async function loadDistribution(): Promise<void> {
  const sessionId = app.photosSession;
  if (!sessionId) return;
  try {
    const result = await photosAssetDistribution(sessionId);

    if (sessionId !== app.photosSession) return;
    const usable = Array.isArray(result?.distribution?.buckets) && result.distribution.buckets.length > 0 && result.distribution.bucketWidthMs > 0;
    distribution.value = usable ? result.distribution : null;

    exactRanges.clear();
    exactReader = null;
    exactReaderDistribution = null;
  } catch {
    if (sessionId !== app.photosSession) return;
    distribution.value = null;
  }

  if (distribution.value && range.value !== "all") void fetchRange();
}

function resetSearch(): void {
  searchEpoch += 1;
  searching.value = false;
  searchSlow.value = false;
  handedToTask.value = false;
  truncated.value = false;
  rangeLanding.value = null;
  found.value = [];
  clearSlowTimer();
}

function selectRange(key: RangeKey): void {
  range.value = key;
  resetSearch();
  if (key === "all") {
    bounds.value = UNBOUNDED;

    if (total.value === null) void loadTotal();
    return;
  }
  if (key === "custom") return;
  bounds.value = presetRange(key);
  void fetchRange();
}

function onCustomChange(value: DateBounds): void {
  range.value = "custom";
  customBounds.value = value;
  if (value.after === null || value.before === null) return;
  bounds.value = value;
  resetSearch();
  void fetchRange();
}

async function fetchRange(): Promise<void> {
  const sessionId = app.photosSession;
  if (!sessionId) {
    toast.error("Photo session is unavailable. Return to Photos and sign in again.");
    return;
  }
  const epoch = ++searchEpoch;
  searching.value = true;
  searchSlow.value = false;
  handedToTask.value = false;
  truncated.value = false;
  rangeLanding.value = null;
  clearSlowTimer();

  if (range.value === "all") {
    searching.value = false;
    return;
  }

  slowTimer = setTimeout(() => {
    slowTimer = null;
    if (epoch === searchEpoch) searchSlow.value = true;
  }, SLOW_SEARCH_MS);

  const target = bounds.value;
  if (target.after !== null || target.before !== null) {
    rangeLanding.value = segmentLanding(distribution.value, target);
    if (distribution.value) {
      const exact = await exactRanges.ensure(target);
      if (epoch !== searchEpoch) return;
      if (exact !== null) {
        searching.value = false;
        searchSlow.value = false;
        clearSlowTimer();
        return;
      }
    }
  }

  const collected: PhotoItem[] = [];
  const seen = new Set<string>();
  let cursor: string | null = null;
  let oldest: number | null = null;
  let pages = 0;

  try {
    while (pages < MAX_PAGES) {
      const page = await photosList({ sessionId, cursor, limit: PAGE_SIZE });
      if (epoch !== searchEpoch) return;
      if (page.status === Status.error) {
        if (isSessionGoneMessage(page.message)) toast.error(page.message ?? "Session expired. Please sign in again.");
        else toast.error(page.message ?? "Could not find photos");
        break;
      }

      const fresh = new Set<string>();
      for (const item of page.photos) {
        if (seen.has(item.id)) continue;
        seen.add(item.id);
        fresh.add(item.id);
      }
      const scan = scanRangePage(page.photos, bounds.value, oldest);
      oldest = scan.oldestTakenAt;
      for (const item of scan.matched) {
        if (fresh.has(item.id)) collected.push(item);
      }
      cursor = page.nextCursor;
      pages += 1;

      found.value = collected.slice();

      if (scan.passedLowerBound) break;
      if (cursor === null || fresh.size === 0) break;
    }
    if (epoch !== searchEpoch) return;
    if (pages >= MAX_PAGES && cursor !== null) truncated.value = true;
    found.value = collected.slice();
  } catch (caught) {
    if (epoch !== searchEpoch) return;
    toast.error(caught instanceof Error ? caught.message : "Could not find photos");
  } finally {
    if (epoch === searchEpoch) {
      searching.value = false;
      searchSlow.value = false;
      clearSlowTimer();
    }
  }
}

const canCreate = computed(() => {
  if (selectionMode.value) return selectedItems.value.length > 0;
  if (range.value === "all") return Boolean(app.photosSession);
  if (rangeExact.value !== null) return Boolean(app.photosSession) && rangeExact.value.count > 0;
  if (!searching.value && !handedToTask.value && !truncated.value) return true;
  return searching.value && searchSlow.value;
});

async function createTask(): Promise<void> {
  const sessionId = app.photosSession;
  const accountKey = app.activeAccountKey;
  if (!sessionId || !accountKey) {
    toast.error("Photo session is unavailable. Return to Photos and sign in again.");
    return;
  }

  const items = selectionMode.value ? selectedItems.value.slice() : [];
  if (selectionMode.value && items.length === 0) {
    toast.error("No photos selected");
    return;
  }
  if (!selectionMode.value && range.value !== "all") {
    if (rangeExact.value?.count === 0) {
      toast.error("No photos in this date range");
      return;
    }
    if (rangeExact.value === null && !searching.value && !truncated.value && found.value.length === 0) {
      toast.error("No photos in this date range");
      return;
    }
  }

  let targetDirectory: string | null = null;
  try {
    targetDirectory = await pickDownloadDirectory();
  } catch (caught) {
    toast.error(caught instanceof Error ? caught.message : "Could not select a folder");
    return;
  }
  if (!targetDirectory) return;

  const subfolder = readStored(StorageKey.downloadSubfolder) === "true";
  if (selectionMode.value) {
    downloads.create({
      accountKey,
      title: `${items.length} selected photos`,
      directory: targetDirectory,
      concurrency,
      photos: items,
      subfolder,
    });
    selectionMode.value = false;
    selectedItems.value = [];
    return;
  }

  if (range.value !== "all") {
    const exactTotal = rangeExact.value?.count ?? (!searching.value && !truncated.value && !handedToTask.value ? found.value.length : null);
    downloads.create({
      accountKey,
      range: {
        sessionId,
        bounds: { ...bounds.value },
        total: exactTotal,
        startRank: rangeExact.value?.oldestRank ?? rangeLanding.value?.startRank ?? null,
      },
      title: taskTitle.value,
      directory: targetDirectory,
      concurrency,
      subfolder,
    });

    searchEpoch += 1;
    searching.value = false;
    searchSlow.value = false;
    handedToTask.value = true;
    clearSlowTimer();
    return;
  }

  downloads.create({
    accountKey,
    range: { sessionId, bounds: UNBOUNDED, total: total.value?.assets ?? null },
    title: taskTitle.value,
    directory: targetDirectory,
    concurrency,
    subfolder,
  });
}
async function openTaskFolder(path: string): Promise<void> {
  try {
    await openDownloadFolder(path);
  } catch (caught) {
    toast.error(caught instanceof Error ? caught.message : "Could not open the folder");
  }
}

function close(): void {
  searchEpoch += 1;
  clearSlowTimer();
  downloads.closeDialog();
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape" && detailsTaskId.value !== null) detailsTaskId.value = null;
}

onMounted(() => window.addEventListener("keydown", onKeydown));
onUnmounted(() => window.removeEventListener("keydown", onKeydown));

void fetchRange();

void loadTotal();
void loadDistribution();

watch(
  () => app.photosSession,
  () => {
    total.value = null;
    distribution.value = null;
    exactRanges.clear();
    exactReader = null;
    exactReaderDistribution = null;
    resetSearch();
    void loadTotal();
    void loadDistribution();
    if (range.value !== "all") void fetchRange();
  },
);
</script>

<template>
  <div class="fixed inset-0 z-[1900] grid place-items-center bg-black/35 p-6">
    <div class="flex h-[min(720px,calc(100vh-48px))] w-[min(920px,calc(100vw-48px))] flex-col overflow-hidden rounded-xl border border-border bg-card shadow-2xl">
      <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4 select-none">
        <Download :size="16" class="text-primary" />
        <h2 class="text-sm font-semibold tracking-tight">{{ headerTitle }}</h2>
        <div class="ml-auto flex items-center gap-2">
          <span v-if="!settingsOpen && downloads.downloadSpeed > 0" class="tabular flex items-center gap-1 text-xs font-medium text-primary">↓ {{ formatSpeed(downloads.downloadSpeed) }}</span>
          <button v-if="!settingsOpen" type="button" class="grid size-8 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" title="Download settings" aria-label="Download settings" @click="openSettings">
            <Settings2 :size="15" />
          </button>
          <button v-else type="button" class="grid size-8 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" title="Back to downloads" aria-label="Back to downloads" @click="settingsOpen = false">
            <ArrowLeft :size="15" />
          </button>
          <button type="button" class="grid size-8 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" title="Close" aria-label="Close" @click="close">
            <X :size="16" />
          </button>
        </div>
      </header>

      <template v-if="!settingsOpen">
        <div class="shrink-0 border-b border-border bg-muted/20 px-4 py-3">
          <div v-if="selectionMode" class="flex items-center gap-3">
            <div class="flex min-w-0 flex-1 items-center gap-3">
              <span class="flex shrink-0 items-center gap-1.5 text-sm font-medium"><Check :size="14" class="text-primary" />{{ selectedItems.length }} selected</span>
              <div class="flex min-w-0 items-center gap-1 overflow-hidden">
                <img v-for="item in selectedItems.slice(0, 6)" :key="item.id" :src="item.thumbUrl" alt="" class="size-7 shrink-0 rounded object-cover" draggable="false" />
                <span v-if="selectedItems.length > 6" class="shrink-0 text-xs text-muted-foreground">+{{ selectedItems.length - 6 }}</span>
              </div>
            </div>
            <Button class="h-8 shrink-0 px-4" :disabled="!canCreate" @click="createTask">{{ createLabel }}</Button>
          </div>

          <div v-else class="flex flex-wrap items-center gap-x-3 gap-y-2">
            <span class="shrink-0 text-xs text-muted-foreground">Range</span>
            <div class="flex shrink-0 items-center gap-0.5 rounded-md bg-secondary p-0.5">
              <button v-for="option in RANGE_OPTIONS" :key="option.key" type="button" class="h-7 rounded-[5px] px-2.5 text-sm transition-colors" :class="range === option.key ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'" @click="selectRange(option.key)">
                {{ option.label }}
              </button>
            </div>
            <DateRangePicker v-if="range === 'custom'" :model-value="customBounds" :show-clear="false" @update:model-value="onCustomChange" />
            <div class="ml-auto flex shrink-0 items-center gap-3">
              <span v-if="showFoundStatus" class="flex items-center gap-1.5 text-xs text-muted-foreground">
                <Spinner v-if="searching" :size="12" />
                {{ foundLabel }}
                <AlertTriangle v-if="searchSlow || truncated" :size="12" class="text-warning" :title="searchSlow ? 'Search is slow; the task will continue while downloading' : 'Only recent photos were scanned'" />
              </span>
              <Button class="h-8 px-4" :disabled="!canCreate" @click="createTask">{{ createLabel }}</Button>
            </div>
          </div>

          <div v-if="downloads.persistenceWarning" class="mt-2 flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 px-2.5 py-2 text-xs text-warning">
            <AlertTriangle :size="13" class="mt-0.5 shrink-0" />
            <span>{{ downloads.persistenceWarning }}</span>
          </div>
        </div>

        <div class="min-h-0 flex-1 overflow-y-auto">
          <div class="sticky top-0 z-10 flex items-center gap-3 border-b border-border bg-card/95 px-4 py-2.5 backdrop-blur">
            <h3 class="text-sm font-medium">Download history</h3>
            <span class="rounded-full bg-secondary px-2 py-0.5 text-2xs tabular text-muted-foreground">{{ downloads.tasks.length }} tasks</span>
            <button v-if="downloads.tasks.some((task) => task.status === 'done' || task.status === 'cancelled')" type="button" class="ml-auto rounded-md px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" @click="downloads.clearFinished()">
              Clear completed
            </button>
          </div>

          <DownloadEmptyState v-if="downloads.tasks.length === 0" />
          <div v-else class="flex flex-col gap-2 p-3">
            <article v-for="task in pagedTasks" :key="task.id" class="rounded-lg border border-border/80 bg-card p-3 shadow-sm transition-colors hover:border-border">
              <div class="flex items-start gap-3">
                <span class="mt-0.5 grid size-8 shrink-0 place-items-center rounded-full" :class="task.status === 'done' && !task.error ? 'bg-success/10 text-success' : task.status === 'cancelled' || task.error ? 'bg-warning/10 text-warning' : 'bg-primary/10 text-primary'">
                  <Check v-if="task.status === 'done' && !task.error" :size="14" :stroke-width="3" />
                  <AlertTriangle v-else-if="task.status === 'cancelled' || task.error" :size="14" />
                  <span v-else class="size-2 rounded-full bg-primary" :class="task.status === 'running' ? 'animate-pulse' : ''" />
                </span>

                <div class="min-w-0 flex-1">
                  <div class="flex items-start gap-2">
                    <div class="min-w-0 flex-1">
                      <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                        <button type="button" class="min-w-0 truncate text-left text-sm font-medium transition-colors hover:text-primary" :title="`${task.title} · View task details`" @click="openTaskDetails(task)">{{ task.title }}</button>
                        <span v-if="task.speedBytesPerSec" class="tabular text-2xs font-medium text-primary">{{ formatSpeed(task.speedBytesPerSec) }}</span>
                        <span v-if="task.failed.length" class="flex items-center gap-1 text-2xs text-warning" :title="task.failed.join('\n')"><AlertTriangle :size="11" />{{ task.failed.length }} failed</span>
                      </div>
                    </div>

                    <div class="flex shrink-0 items-center gap-1">
                      <Button size="icon" variant="outline" class="size-7" title="View task details" aria-label="View task details" @click="openTaskDetails(task)">
                        <ListChecks :size="14" />
                      </Button>
                      <Button v-if="task.status === 'running'" size="icon" variant="outline" class="size-7" title="Pause task" aria-label="Pause task" @click="downloads.pause(task.id)">
                        <Pause :size="14" />
                      </Button>
                      <Button
                        v-else-if="task.status === 'paused' || task.status === 'cancelled'"
                        size="icon"
                        variant="outline"
                        class="size-7"
                        :disabled="!canResumeTasks"
                        :title="canResumeTasks ? (task.status === 'cancelled' ? 'Restore task' : 'Resume task') : 'Sign in again to resume'"
                        :aria-label="task.status === 'cancelled' ? 'Restore task' : 'Resume task'"
                        @click="downloads.resume(task.id)"
                      >
                        <Play :size="14" />
                      </Button>
                      <Button size="icon" variant="outline" class="size-7 hover:text-destructive" title="Remove from list (keeps files)" aria-label="Remove from list" @click="downloads.remove(task.id)">
                        <Trash :size="14" />
                      </Button>
                    </div>
                  </div>

                  <div class="mt-2 flex items-center gap-3">
                    <div class="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-secondary">
                      <div class="h-full rounded-full transition-[width] duration-200" :class="[task.status === 'cancelled' || task.error ? 'bg-warning' : 'bg-primary', totalOf(task) === null ? 'animate-pulse' : '']" :style="{ width: `${percentOf(task) ?? 0}%` }" />
                    </div>
                    <span class="tabular min-w-[72px] shrink-0 text-right text-xs text-muted-foreground">
                      {{ task.done + task.failed.length }}<template v-if="totalOf(task) !== null"> / {{ totalOf(task) }}</template>
                    </span>
                  </div>

                  <div class="mt-2 flex min-w-0 items-center justify-between gap-3">
                    <div class="min-w-0 flex-1">
                      <p v-if="task.activeFiles?.length" class="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                        <span class="shrink-0 text-primary">Downloading</span>
                        <span class="truncate font-medium text-foreground" :title="task.activeFiles.join('\n')">{{ task.activeFiles[0] }}</span>
                        <span v-if="task.activeFiles.length > 1" class="shrink-0">and {{ task.activeFiles.length }} files</span>
                      </p>
                      <p v-else-if="task.lastFile" class="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                        <span class="shrink-0">Recent files</span>
                        <span class="truncate" :title="task.lastFile">{{ task.lastFile }}</span>
                      </p>
                      <p class="mt-0.5 truncate text-2xs text-muted-foreground" :title="task.directory">{{ task.directory }}</p>
                    </div>
                    <button type="button" class="flex shrink-0 items-center gap-1 rounded-md px-1.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-primary" :title="task.directory" @click="openTaskFolder(task.directory)">
                      <FolderOpen :size="12" />
                      Open folder
                    </button>
                  </div>
                  <p v-if="task.status === 'paused' && !canResumeTasks" class="mt-1 text-xs text-muted-foreground">Sign in to this account again to resume this task</p>
                  <p v-if="task.error" class="mt-1 text-xs text-warning">{{ task.error }}</p>
                </div>
              </div>
            </article>
          </div>

          <div v-if="totalPages > 1" class="sticky bottom-0 flex items-center justify-center gap-2 border-t border-border bg-card px-4 py-2 text-xs text-muted-foreground">
            <button type="button" class="rounded-md border border-border px-2 py-1 transition-colors enabled:hover:bg-accent enabled:hover:text-foreground disabled:opacity-40" :disabled="taskPage <= 1" @click="goToPage(taskPage - 1)">Previous</button>
            <span class="tabular">Page {{ taskPage }} / {{ totalPages }}</span>
            <button type="button" class="rounded-md border border-border px-2 py-1 transition-colors enabled:hover:bg-accent enabled:hover:text-foreground disabled:opacity-40" :disabled="taskPage >= totalPages" @click="goToPage(taskPage + 1)">Next</button>
          </div>
        </div>
      </template>

      <div v-else class="min-h-0 flex-1 overflow-y-auto p-6">
        <div class="mx-auto max-w-xl">
          <h3 class="text-base font-semibold tracking-tight">Global Speed Limit</h3>
          <p class="mt-1 text-sm leading-6 text-muted-foreground">Limit the combined bandwidth used by all downloads. Leave blank or enter 0 for no limit.</p>
          <label class="mt-5 block text-sm font-medium" for="download-speed-limit">Speed limit (MB/s)</label>
          <div class="mt-2 flex items-center gap-2">
            <input
              id="download-speed-limit"
              v-model="speedLimitMb"
              type="number"
              min="0"
              step="0.5"
              inputmode="decimal"
              placeholder="0 (unlimited)"
              class="h-9 w-52 rounded-md border border-input bg-background px-3 text-sm outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring/25"
            />
            <span class="text-sm text-muted-foreground">MB/s</span>
          </div>
          <p class="mt-2 text-xs text-muted-foreground">For example, 10 means all active downloads combined stay below 10 MB/s. Applies immediately and is restored on the next launch.</p>
          <div class="mt-6 flex items-center gap-2">
            <Button class="h-8 px-4" @click="saveSettings">Save Settings</Button>
            <Button variant="outline" class="h-8 px-4" @click="settingsOpen = false">Cancel</Button>
          </div>
        </div>
      </div>
    </div>

    <div v-if="detailsTask && detailsQueue" class="fixed inset-0 z-[2100] grid place-items-center bg-black/40 p-6 backdrop-blur-[1px]" @click.self="detailsTaskId = null">
      <div class="flex max-h-[min(780px,calc(100vh-48px))] w-[min(760px,calc(100vw-48px))] flex-col overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-2xl" role="dialog" aria-modal="true" aria-labelledby="task-details-title">
        <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
          <ListChecks :size="16" class="text-primary" />
          <h2 id="task-details-title" class="text-sm font-semibold">Task Details</h2>
          <span class="rounded-full px-2 py-0.5 text-2xs font-medium" :class="detailsTask.status === 'running' ? 'bg-primary/10 text-primary' : detailsTask.status === 'done' && !detailsTask.error ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning'">{{ taskStatusLabel(detailsTask) }}</span>
          <button type="button" class="ml-auto grid size-8 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" aria-label="Close task details" @click="detailsTaskId = null">
            <X :size="16" />
          </button>
        </header>

        <div class="min-h-0 flex-1 overflow-y-auto">
          <section class="border-b border-border p-4">
            <div class="flex items-start gap-3">
              <div class="min-w-0 flex-1">
                <h3 class="break-words text-base font-semibold">{{ detailsTask.title }}</h3>
                <p class="mt-1 truncate text-xs text-muted-foreground" :title="detailsTask.directory">{{ detailsTask.directory }}</p>
              </div>
              <button type="button" class="flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-primary" @click="openTaskFolder(detailsTask.directory)">
                <FolderOpen :size="13" />
                Open folder
              </button>
            </div>

            <div class="mt-4 flex items-center gap-3">
              <div class="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-secondary">
                <div class="h-full rounded-full bg-primary transition-[width] duration-200" :class="detailsTask.status === 'cancelled' || detailsTask.error ? '!bg-warning' : ''" :style="{ width: `${percentOf(detailsTask) ?? 0}%` }" />
              </div>
              <span class="tabular shrink-0 text-xs text-muted-foreground"
                >{{ detailsTask.done + detailsTask.failed.length }}<template v-if="totalOf(detailsTask) !== null"> / {{ totalOf(detailsTask) }}</template></span
              >
              <span v-if="detailsTask.speedBytesPerSec" class="tabular shrink-0 text-xs font-medium text-primary">{{ formatSpeed(detailsTask.speedBytesPerSec) }}</span>
            </div>

            <dl class="mt-4 grid grid-cols-3 gap-2 text-center">
              <div class="rounded-md border border-border bg-background px-2 py-2">
                <dt class="text-2xs text-muted-foreground">Completed</dt>
                <dd class="tabular mt-0.5 text-sm font-semibold">{{ detailsTask.done }}</dd>
              </div>
              <div class="rounded-md border border-border bg-background px-2 py-2">
                <dt class="text-2xs text-muted-foreground">Failed</dt>
                <dd class="tabular mt-0.5 text-sm font-semibold" :class="detailsTask.failed.length ? 'text-warning' : ''">{{ detailsTask.failed.length }}</dd>
              </div>
              <div class="rounded-md border border-border bg-background px-2 py-2">
                <dt class="text-2xs text-muted-foreground">Remaining</dt>
                <dd class="tabular mt-0.5 text-sm font-semibold">{{ detailsQueue.remaining ?? "Unknown" }}</dd>
              </div>
            </dl>
          </section>

          <section class="p-4">
            <div class="flex items-center gap-2">
              <h3 class="text-sm font-semibold">Download Queue</h3>
              <span class="rounded-full bg-secondary px-2 py-0.5 text-2xs tabular text-muted-foreground">{{ detailsQueue.active.length + detailsQueue.pendingTotal }} items retrieved</span>
            </div>

            <div v-if="detailsQueue.active.length" class="mt-4">
              <h4 class="flex items-center gap-1.5 text-xs font-medium text-primary"><span class="size-1.5 rounded-full bg-primary" />Downloading</h4>
              <ul class="mt-2 divide-y divide-border rounded-md border border-border bg-background">
                <li v-for="item in detailsQueue.active" :key="item.key" class="flex min-w-0 items-center gap-2 px-3 py-2 text-xs">
                  <Spinner :size="12" class="shrink-0 text-primary" />
                  <span class="truncate" :title="item.name">{{ item.name }}</span>
                </li>
              </ul>
            </div>

            <div v-if="detailsQueue.pendingTotal > 0" class="mt-4">
              <h4 class="text-xs font-medium text-muted-foreground">
                Pending <span class="tabular">{{ detailsQueue.pendingTotal }}</span>
              </h4>
              <ol class="mt-2 max-h-64 divide-y divide-border overflow-y-auto rounded-md border border-border bg-background">
                <li v-for="(item, index) in detailsPendingPreview" :key="item.key" class="flex min-w-0 items-center gap-2 px-3 py-2 text-xs">
                  <span class="tabular w-7 shrink-0 text-right text-2xs text-muted-foreground">{{ index + 1 }}</span>
                  <span class="truncate" :title="item.name">{{ item.name }}</span>
                </li>
              </ol>
              <p v-if="detailsPendingHidden > 0" class="mt-1.5 text-2xs text-muted-foreground">Showing the first {{ QUEUE_PREVIEW_LIMIT }} items; {{ detailsPendingHidden }} more are queued.</p>
            </div>

            <p v-if="detailsQueue.unfetched !== null && detailsQueue.unfetched > 0" class="mt-3 rounded-md border border-border bg-muted/35 px-3 py-2 text-xs text-muted-foreground">
              <span class="tabular font-medium text-foreground">{{ detailsQueue.unfetched }}</span> more waiting to be retrieved from iCloud.
            </p>

            <div v-if="detailsQueue.failed.length" class="mt-4">
              <h4 class="flex items-center gap-1.5 text-xs font-medium text-warning">
                <AlertTriangle :size="12" />Failed files <span class="tabular">{{ detailsQueue.failed.length }}</span>
              </h4>
              <ul class="mt-2 max-h-40 divide-y divide-border overflow-y-auto rounded-md border border-warning/20 bg-warning/5">
                <li v-for="(name, index) in detailsQueue.failed" :key="`${name}:${index}`" class="truncate px-3 py-2 text-xs text-warning" :title="name">{{ name }}</li>
              </ul>
            </div>

            <div v-if="detailsQueue.active.length === 0 && detailsQueue.pendingTotal === 0 && (detailsQueue.unfetched ?? 0) === 0" class="mt-4 rounded-md border border-dashed border-border px-4 py-8 text-center text-xs text-muted-foreground">
              {{ detailsQueue.failed.length ? "No pending items. Review failed files above." : detailsTask.status === "running" ? "Retrieving the next queue…" : "Queue is empty" }}
            </div>
          </section>
        </div>
      </div>
    </div>
  </div>
</template>
