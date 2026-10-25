<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { AlertTriangle, ArrowLeft, Check, Download, FolderOpen, ListChecks, Pause, Play, RotateCcw, Settings2, Trash, X } from "@lucide/vue";

import { Status, StorageKey } from "@/constants";
import { openDownloadFolder, photosAssetDistribution, photosList, photosTotal, pickDownloadDirectory } from "@/lib/api";
import { readStored, writeStored } from "@/lib/common/storage";
import { toast } from "@/lib/common/toast";
import { isSessionGoneMessage } from "@/lib/session-errors";
import { useAppStore } from "@/stores/app";
import { CONCURRENCY_OPTIONS, formatBytes, formatSpeed, percentOf, queueView, totalOf, useDownloadsStore, type DownloadTask } from "@/stores/downloads";
import { RANGE_OPTIONS, UNBOUNDED, addDays, formatDateInput, presetRange, type DateBounds, type RangeKey } from "@/lib/photos/dates";
import { segmentLanding, type SegmentLanding } from "@/lib/photos/distribution";
import { createExactRangeLoader, createExactRangeStore, type ExactRangeResult } from "@/lib/photos/exact";
import { scanRangePage } from "@/lib/photos/range";
import { DateRangePicker } from "@/components/ui/date-range-picker";
import { Button } from "@/components/ui/button";
import Spinner from "@/components/ui/Spinner.vue";
import DownloadEmptyState from "@/components/photos/DownloadEmptyState.vue";
import DownloadFileList from "@/components/photos/DownloadFileList.vue";
import type { DownloadFileRow } from "@/lib/downloads/file-row";
import type { AssetDistribution, LibraryTotal, PhotoItem } from "@/types";

const downloads = useDownloadsStore();
const app = useAppStore();

const canResumeTasks = computed(() => Boolean(app.photosSession && app.activeAccountKey && downloads.activeAccountKey === app.activeAccountKey));

const settingsOpen = ref(false);
const speedLimitMb = ref("");
const downloadDirectory = ref(readStored(StorageKey.downloadDirectory) ?? "");
const headerTitle = computed(() => (settingsOpen.value ? "Download Settings" : "Download Photos"));
const headerSubtitle = computed(() => (settingsOpen.value ? "Bandwidth limits for all active downloads" : "Choose a photo range and destination"));
const taskOverview = computed(() => {
  let running = 0;
  let paused = 0;
  let completed = 0;
  for (const task of downloads.tasks) {
    if (task.status === "running") running += 1;
    else if (task.status === "paused") paused += 1;
    else if (task.status === "done") completed += 1;
  }
  return { running, paused, completed };
});

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
const queueTab = ref<"active" | "completed">("active");

const detailsTaskId = ref<string | null>(null);
const detailsTab = ref<"downloading" | "queued" | "completed">("downloading");

const activeTasks = computed(() => downloads.tasks.filter((task) => task.status !== "done"));
const completedTasks = computed(() => downloads.tasks.filter((task) => task.status === "done"));
const visibleTasks = computed(() => (queueTab.value === "active" ? activeTasks.value : completedTasks.value));
const totalPages = computed(() => Math.max(1, Math.ceil(visibleTasks.value.length / TASK_PAGE_SIZE)));
const pagedTasks = computed(() => visibleTasks.value.slice((taskPage.value - 1) * TASK_PAGE_SIZE, taskPage.value * TASK_PAGE_SIZE));

function selectQueueTab(tab: "active" | "completed"): void {
  queueTab.value = tab;
  taskPage.value = 1;
}
const detailsTask = computed(() => downloads.tasks.find((task) => task.id === detailsTaskId.value) ?? null);
const detailsQueue = computed(() => (detailsTask.value ? queueView(detailsTask.value, Number.MAX_SAFE_INTEGER) : null));

const detailsActiveRows = computed<DownloadFileRow[]>(() =>
  (detailsQueue.value?.active ?? []).map((item) => ({
    key: item.key,
    name: item.name,
    sizeBytes: item.sizeBytes,
    downloadedBytes: item.downloadedBytes,
    totalBytes: item.totalBytes,
    status: "active",
    photoId: item.photoId ?? undefined,
  })),
);
const detailsPausedRows = computed<DownloadFileRow[]>(() =>
  (detailsQueue.value?.paused ?? []).map((item, index) => ({
    key: item.key,
    name: item.name,
    sizeBytes: item.sizeBytes,
    downloadedBytes: item.downloadedBytes,
    totalBytes: item.totalBytes,
    status: "paused",
    ordinal: index + 1,
    photoId: item.photoId,
  })),
);
const detailsDownloadingRows = computed<DownloadFileRow[]>(() => [...detailsActiveRows.value, ...detailsPausedRows.value]);
const detailsPendingRows = computed<DownloadFileRow[]>(() => (detailsQueue.value?.pending ?? []).map((item, index) => ({ key: item.key, name: item.name, sizeBytes: item.sizeBytes, status: "pending", ordinal: index + 1, photoId: item.photoId })));
const detailsCompletedFiles = computed(() => {
  const task = detailsTask.value;
  if (!task) return [];
  const recorded = task.completedFiles ?? [];
  if (recorded.length > 0) return recorded;
  return task.failed.length === 0 ? task.usedNames.slice(0, task.done).map((name) => ({ name, sizeBytes: null })) : [];
});
const detailsCompletedRows = computed<DownloadFileRow[]>(() => detailsCompletedFiles.value.map((file, index) => ({ key: `completed:${index}:${file.name}`, name: file.name, sizeBytes: file.sizeBytes, status: "completed" })));
const detailsFailedRows = computed<DownloadFileRow[]>(() => (detailsQueue.value?.failed ?? []).map((name, index) => ({ key: `failed:${index}:${name}`, name, sizeBytes: null, status: "failed" })));
const detailsCompletedBytes = computed<number | null>(() => {
  const sizes = detailsCompletedFiles.value.flatMap((file) => (file.sizeBytes === null ? [] : [file.sizeBytes]));
  return sizes.length > 0 ? sizes.reduce((sum, size) => sum + size, 0) : null;
});
const detailsTransferredBytes = computed<number | null>(() => {
  const activeBytes = detailsActiveRows.value.reduce((sum, row) => sum + (row.downloadedBytes ?? 0), 0);
  const pausedBytes = detailsPausedRows.value.reduce((sum, row) => sum + (row.downloadedBytes ?? 0), 0);
  if (detailsCompletedBytes.value === null && activeBytes + pausedBytes === 0) return null;
  return (detailsCompletedBytes.value ?? 0) + activeBytes + pausedBytes;
});

function openTaskDetails(task: DownloadTask): void {
  detailsTaskId.value = task.id;
  const queue = queueView(task, Number.MAX_SAFE_INTEGER);
  if (queue.active.length > 0 || queue.paused.length > 0 || task.status === "running") detailsTab.value = "downloading";
  else if (queue.pendingTotal > 0 || queue.paused.length > 0 || (queue.unfetched ?? 0) > 0) detailsTab.value = "queued";
  else detailsTab.value = "completed";
}

function folderLabel(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

function formatTaskCreatedAt(createdAt: number): string {
  return new Date(createdAt).toLocaleString("en-US", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function taskStatusLabel(task: DownloadTask): string {
  if (task.status === "running") return "Downloading";
  if (task.status === "paused") return "Paused";
  if (task.status === "done") {
    if (task.error) return "Failed";
    return task.failed.length > 0 ? "Completed with errors" : "Completed";
  }
  return task.error ? "Failed" : "Cancelled";
}

watch(totalPages, (pages) => {
  if (taskPage.value > pages) taskPage.value = pages;
});

watch(queueTab, () => {
  taskPage.value = 1;
});

watch(
  () => downloads.dialogOpen,
  (open) => {
    if (!open) return;
    queueTab.value = activeTasks.value.length > 0 || completedTasks.value.length === 0 ? "active" : "completed";
    taskPage.value = 1;
  },
);

watch(
  () => downloads.tasks[0]?.id,
  () => {
    queueTab.value = "active";
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

async function chooseDownloadDirectory(): Promise<string | null> {
  try {
    const picked = await pickDownloadDirectory();
    if (!picked) return null;
    downloadDirectory.value = picked;
    writeStored(StorageKey.downloadDirectory, picked);
    return picked;
  } catch (caught) {
    toast.error(caught instanceof Error ? caught.message : "Could not select a folder");
    return null;
  }
}

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

const rangeStartRank = computed<number | null>(() => {
  if (range.value === "all") return null;
  return rangeExact.value?.oldestRank ?? rangeLanding.value?.startRank ?? segmentLanding(distribution.value, bounds.value)?.startRank ?? null;
});

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
const createLabel = computed(() => (selectionMode.value ? "Download selected" : "Download"));

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
  if (!app.photosSession) return false;
  if (selectionMode.value) return selectedItems.value.length > 0;
  if (range.value === "all") return true;
  if (range.value === "custom" && !hasCustomRange.value) return false;
  if (rangeExact.value !== null) return rangeExact.value.count > 0;
  if (rangeStartRank.value !== null) return true;
  return !searching.value && !handedToTask.value && found.value.length > 0;
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

  let targetDirectory = downloadDirectory.value.trim();
  if (!targetDirectory) targetDirectory = (await chooseDownloadDirectory()) ?? "";
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
    const startRank = rangeStartRank.value;

    if (startRank === null) {
      const scannedItems = found.value.slice();
      if (scannedItems.length === 0) {
        toast.error("No photos in this date range");
        return;
      }
      downloads.create({
        accountKey,
        title: taskTitle.value,
        directory: targetDirectory,
        concurrency,
        photos: scannedItems,
        subfolder,
      });
    } else {
      downloads.create({
        accountKey,
        range: {
          sessionId,
          bounds: { ...bounds.value },
          total: exactTotal,
          startRank,
        },
        title: taskTitle.value,
        directory: targetDirectory,
        concurrency,
        subfolder,
      });
    }

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
  <div class="fixed inset-0 z-[1900] grid place-items-center bg-black/40 p-5 backdrop-blur-[1px]">
    <div class="flex h-[min(760px,calc(100vh-40px))] w-[min(1000px,calc(100vw-40px))] flex-col overflow-hidden rounded-2xl border border-border bg-card shadow-2xl">
      <header class="flex h-14 shrink-0 items-center gap-3 border-b border-border bg-card px-5 select-none">
        <span class="grid size-8 shrink-0 place-items-center rounded-lg bg-primary/10 text-primary">
          <Download :size="16" />
        </span>
        <div class="min-w-0">
          <h2 class="truncate text-sm font-semibold tracking-tight">{{ headerTitle }}</h2>
          <p class="truncate text-2xs text-muted-foreground">{{ headerSubtitle }}</p>
        </div>
        <div class="ml-auto flex items-center gap-1.5">
          <span v-if="!settingsOpen && downloads.downloadSpeed > 0" class="tabular mr-1 flex items-center gap-1 rounded-full bg-primary/10 px-2.5 py-1 text-2xs font-medium text-primary">↓ {{ formatSpeed(downloads.downloadSpeed) }}</span>
          <button v-if="!settingsOpen" type="button" class="grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" title="Download settings" aria-label="Download settings" @click="openSettings">
            <Settings2 :size="15" />
          </button>
          <button v-else type="button" class="grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" title="Back to downloads" aria-label="Back to downloads" @click="settingsOpen = false">
            <ArrowLeft :size="15" />
          </button>
          <button type="button" class="grid size-8 place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive" title="Close" aria-label="Close" @click="close">
            <X :size="16" />
          </button>
        </div>
      </header>

      <template v-if="!settingsOpen">
        <div class="shrink-0 border-b border-border bg-muted/20 p-3">
          <div v-if="selectionMode" class="rounded-xl border border-border bg-card p-3 shadow-sm">
            <div class="flex min-w-0 items-center gap-3">
              <span class="flex shrink-0 items-center gap-1.5 text-sm font-medium"><Check :size="14" class="text-primary" />{{ selectedItems.length }} selected</span>
              <div class="flex min-w-0 flex-1 items-center gap-1 overflow-hidden">
                <img v-for="item in selectedItems.slice(0, 8)" :key="item.id" :src="item.thumbUrl" alt="" class="size-8 shrink-0 rounded-md border border-border/60 object-cover" draggable="false" />
                <span v-if="selectedItems.length > 8" class="shrink-0 text-xs text-muted-foreground">+{{ selectedItems.length - 8 }}</span>
              </div>
              <button
                type="button"
                class="flex h-8 min-w-0 max-w-56 shrink-0 items-center gap-2 rounded-lg border border-border bg-background px-2.5 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                :title="downloadDirectory || 'Choose download folder'"
                @click="chooseDownloadDirectory"
              >
                <FolderOpen :size="13" class="shrink-0" />
                <span class="truncate">{{ downloadDirectory ? folderLabel(downloadDirectory) : "Choose folder" }}</span>
              </button>
              <Button class="h-8 shrink-0 px-4 text-xs" :disabled="!canCreate" @click="createTask">{{ createLabel }}</Button>
            </div>
          </div>

          <div v-else class="rounded-xl border border-border bg-card p-3 shadow-sm">
            <div class="flex min-w-0 flex-wrap items-center gap-2">
              <span class="shrink-0 text-2xs font-medium uppercase tracking-wide text-muted-foreground">Range</span>
              <div class="flex shrink-0 items-center gap-0.5 rounded-lg bg-secondary/80 p-1">
                <button v-for="option in RANGE_OPTIONS" :key="option.key" type="button" class="h-7 rounded-md px-2.5 text-xs transition-colors" :class="range === option.key ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'" @click="selectRange(option.key)">
                  {{ option.label }}
                </button>
              </div>
              <DateRangePicker v-if="range === 'custom'" :model-value="customBounds" :show-clear="false" @update:model-value="onCustomChange" />
              <span v-if="showFoundStatus" class="flex min-w-28 flex-1 items-center gap-1.5 truncate text-xs text-muted-foreground">
                <Spinner v-if="searching" :size="12" />
                <span class="truncate">{{ foundLabel }}</span>
                <AlertTriangle v-if="searchSlow || truncated" :size="12" class="shrink-0 text-warning" :title="searchSlow ? 'Search is slow; the task will continue while downloading' : 'Only recent photos were scanned'" />
              </span>
              <div v-else class="flex-1" />
              <button
                type="button"
                class="flex h-8 min-w-0 max-w-56 shrink-0 items-center gap-2 rounded-lg border border-border bg-background px-2.5 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                :title="downloadDirectory || 'Choose download folder'"
                @click="chooseDownloadDirectory"
              >
                <FolderOpen :size="13" class="shrink-0" />
                <span class="truncate">{{ downloadDirectory ? folderLabel(downloadDirectory) : "Choose folder" }}</span>
              </button>
              <Button class="h-8 shrink-0 px-4 text-xs" :disabled="!canCreate" @click="createTask">{{ createLabel }}</Button>
            </div>
          </div>

          <div v-if="downloads.persistenceWarning" class="mt-2 flex items-start gap-2 rounded-lg border border-warning/30 bg-warning/10 px-3 py-2 text-xs text-warning">
            <AlertTriangle :size="13" class="mt-0.5 shrink-0" />
            <span>{{ downloads.persistenceWarning }}</span>
          </div>
        </div>
        <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
          <div class="flex h-12 shrink-0 items-center gap-3 border-b border-border bg-card px-4">
            <div class="grid w-64 grid-cols-2 rounded-lg bg-secondary/80 p-1" role="tablist" aria-label="Download tasks">
              <button
                type="button"
                class="flex h-8 items-center justify-center gap-1.5 rounded-md text-xs transition-colors"
                :class="queueTab === 'active' ? 'bg-card font-medium text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'"
                role="tab"
                :aria-selected="queueTab === 'active'"
                @click="selectQueueTab('active')"
              >
                Active <span class="tabular text-2xs">{{ activeTasks.length }}</span>
              </button>
              <button
                type="button"
                class="flex h-8 items-center justify-center gap-1.5 rounded-md text-xs transition-colors"
                :class="queueTab === 'completed' ? 'bg-card font-medium text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'"
                role="tab"
                :aria-selected="queueTab === 'completed'"
                @click="selectQueueTab('completed')"
              >
                Completed <span class="tabular text-2xs">{{ completedTasks.length }}</span>
              </button>
            </div>
            <div class="ml-auto flex items-center gap-3 text-2xs">
              <template v-if="queueTab === 'active'">
                <span v-if="taskOverview.running" class="font-medium text-primary">{{ taskOverview.running }} running</span>
                <span v-if="taskOverview.paused" class="font-medium text-warning">{{ taskOverview.paused }} paused</span>
              </template>
              <button v-else-if="completedTasks.length" type="button" class="rounded-md px-2 py-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" @click="downloads.clearFinished()">Clear finished</button>
            </div>
          </div>

          <DownloadEmptyState v-if="downloads.tasks.length === 0" />
          <div v-else-if="visibleTasks.length === 0" class="grid min-h-0 flex-1 place-items-center p-6 text-center">
            <div>
              <span class="mx-auto grid size-10 place-items-center rounded-full bg-muted text-muted-foreground">
                <Check v-if="queueTab === 'completed'" :size="17" />
                <Download v-else :size="17" />
              </span>
              <p class="mt-3 text-sm font-medium">{{ queueTab === "active" ? "No active downloads" : "No completed downloads" }}</p>
              <p class="mt-1 text-xs text-muted-foreground">{{ queueTab === "active" ? "New tasks will appear here." : "Finished downloads will appear here." }}</p>
            </div>
          </div>
          <div v-else class="min-h-0 flex-1 overflow-y-auto px-3 py-3">
            <div class="flex flex-col gap-2">
              <article v-for="task in pagedTasks" :key="task.id" class="group relative cursor-pointer overflow-hidden rounded-xl border border-border/80 bg-card transition-all hover:border-primary/40 hover:shadow-sm" :title="`${task.title} · View task details`" @click="openTaskDetails(task)">
                <span class="absolute inset-y-3 left-0 w-0.5 rounded-r-full" :class="task.status === 'done' && !task.error && task.failed.length === 0 ? 'bg-success' : task.status === 'paused' || task.status === 'cancelled' || task.error || task.failed.length > 0 ? 'bg-warning' : 'bg-primary'" />
                <div class="p-3.5 pl-4">
                  <div class="flex min-w-0 items-start gap-3">
                    <span
                      class="mt-0.5 grid size-8 shrink-0 place-items-center rounded-full"
                      :class="task.status === 'done' && !task.error && task.failed.length === 0 ? 'bg-success/10 text-success' : task.status === 'paused' || task.status === 'cancelled' || task.error || task.failed.length > 0 ? 'bg-warning/10 text-warning' : 'bg-primary/10 text-primary'"
                    >
                      <Check v-if="task.status === 'done' && !task.error && task.failed.length === 0" :size="14" :stroke-width="3" />
                      <AlertTriangle v-else-if="task.status === 'cancelled' || task.error || task.failed.length > 0" :size="14" />
                      <Pause v-else-if="task.status === 'paused'" :size="14" />
                      <span v-else class="size-2 animate-pulse rounded-full bg-primary" />
                    </span>

                    <div class="min-w-0 flex-1">
                      <div class="flex min-w-0 items-center gap-2">
                        <h4 class="truncate text-sm font-semibold">{{ task.title }}</h4>
                        <span class="shrink-0 rounded-full px-2 py-0.5 text-2xs font-medium" :class="task.status === 'running' ? 'bg-primary/10 text-primary' : task.status === 'done' && !task.error && task.failed.length === 0 ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning'">{{
                          taskStatusLabel(task)
                        }}</span>
                        <span v-if="task.speedBytesPerSec" class="tabular shrink-0 text-2xs font-medium text-primary">{{ formatSpeed(task.speedBytesPerSec) }}</span>
                      </div>
                      <p class="mt-0.5 flex min-w-0 items-center gap-1.5 text-2xs text-muted-foreground">
                        <span
                          >{{ task.done + task.failed.length }}<template v-if="totalOf(task) !== null"> / {{ totalOf(task) }}</template> files</span
                        >
                        <span>·</span>
                        <span>{{ percentOf(task) ?? 0 }}%</span>
                        <span>·</span>
                        <span class="truncate">Created {{ formatTaskCreatedAt(task.createdAt) }}</span>
                        <button type="button" class="ml-auto flex min-w-0 max-w-64 shrink items-center gap-1 rounded px-1.5 py-0.5 text-muted-foreground transition-colors hover:bg-accent hover:text-primary" :title="task.directory" @click.stop="openTaskFolder(task.directory)">
                          <FolderOpen :size="11" class="shrink-0" />
                          <span class="shrink-0">Open folder</span>
                          <span class="text-muted-foreground/60">·</span>
                          <span class="truncate">{{ folderLabel(task.directory) }}</span>
                        </button>
                      </p>
                    </div>

                    <div class="flex shrink-0 items-center gap-0.5" @click.stop>
                      <Button v-if="(task.failedItems?.length ?? 0) > 0 && task.status !== 'running'" variant="ghost" size="icon" class="size-7 rounded-full" title="Retry failed files" aria-label="Retry failed files" @click="downloads.retryFailed(task.id)">
                        <RotateCcw :size="13" />
                      </Button>
                      <Button v-if="task.status === 'running'" variant="ghost" size="icon" class="size-7 rounded-full" title="Pause task" aria-label="Pause task" @click="downloads.pause(task.id)">
                        <Pause :size="14" />
                      </Button>
                      <Button
                        v-else-if="task.status === 'paused' || task.status === 'cancelled' || task.error"
                        variant="ghost"
                        size="icon"
                        class="size-7 rounded-full"
                        :disabled="!canResumeTasks"
                        :title="canResumeTasks ? (task.status === 'cancelled' || task.error ? 'Restore task' : 'Resume task') : 'Sign in again to resume'"
                        :aria-label="task.status === 'cancelled' || task.error ? 'Restore task' : 'Resume task'"
                        @click="downloads.resume(task.id)"
                      >
                        <Play :size="14" />
                      </Button>
                      <Button variant="ghost" size="icon" class="size-7 rounded-full text-muted-foreground hover:text-destructive" title="Remove from list (keeps files)" aria-label="Remove from list" @click="downloads.remove(task.id)">
                        <Trash :size="13" />
                      </Button>
                    </div>
                  </div>

                  <div class="mt-3 flex items-center gap-3">
                    <div class="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-secondary">
                      <div class="h-full rounded-full transition-[width] duration-200" :class="task.status === 'cancelled' || task.error || task.failed.length > 0 ? 'bg-warning' : task.status === 'done' ? 'bg-success' : 'bg-primary'" :style="{ width: `${percentOf(task) ?? 0}%` }" />
                    </div>
                    <span class="tabular w-9 shrink-0 text-right text-2xs text-muted-foreground">{{ percentOf(task) ?? 0 }}%</span>
                  </div>

                  <div v-if="task.activeFiles?.length || task.lastFile" class="mt-2 flex min-w-0 items-center text-2xs text-muted-foreground">
                    <span v-if="task.activeFiles?.length" class="flex min-w-0 flex-1 items-center gap-1.5 text-primary">
                      <Spinner :size="11" class="shrink-0" />
                      <span class="truncate" :title="task.activeFiles[0]">{{ task.activeFiles[0] }}</span>
                      <span v-if="task.activeFiles.length > 1" class="shrink-0">+{{ task.activeFiles.length - 1 }}</span>
                    </span>
                    <span v-else-if="task.lastFile" class="min-w-0 flex-1 truncate" :title="task.lastFile">Last: {{ task.lastFile }}</span>
                  </div>
                  <p v-if="task.status === 'paused' && !canResumeTasks" class="mt-1.5 text-2xs text-muted-foreground">Sign in to this account again to resume this task</p>
                  <p v-if="task.error" class="mt-1.5 line-clamp-2 text-2xs text-warning">{{ task.error }}</p>
                </div>
              </article>
            </div>
          </div>

          <div v-if="totalPages > 1" class="flex h-10 shrink-0 items-center justify-center gap-2 border-t border-border bg-card px-4 text-2xs text-muted-foreground">
            <button type="button" class="rounded-md px-2 py-1 transition-colors enabled:hover:bg-accent enabled:hover:text-foreground disabled:opacity-40" :disabled="taskPage <= 1" @click="goToPage(taskPage - 1)">Previous</button>
            <span class="tabular">Page {{ taskPage }} / {{ totalPages }}</span>
            <button type="button" class="rounded-md px-2 py-1 transition-colors enabled:hover:bg-accent enabled:hover:text-foreground disabled:opacity-40" :disabled="taskPage >= totalPages" @click="goToPage(taskPage + 1)">Next</button>
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
      <div class="flex h-[min(680px,calc(100vh-48px))] w-[min(760px,calc(100vw-48px))] flex-col overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-2xl" role="dialog" aria-modal="true" aria-labelledby="task-details-title">
        <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
          <ListChecks :size="16" class="text-primary" />
          <h2 id="task-details-title" class="text-sm font-semibold">Task Details</h2>
          <span class="rounded-full px-2 py-0.5 text-2xs font-medium" :class="detailsTask.status === 'running' ? 'bg-primary/10 text-primary' : detailsTask.status === 'done' && !detailsTask.error && detailsTask.failed.length === 0 ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning'">{{
            taskStatusLabel(detailsTask)
          }}</span>
          <button type="button" class="ml-auto grid size-8 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" aria-label="Close task details" @click="detailsTaskId = null">
            <X :size="16" />
          </button>
        </header>

        <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
          <section class="shrink-0 border-b border-border p-4">
            <div class="flex items-start gap-3">
              <div class="min-w-0 flex-1">
                <h3 class="break-words text-base font-semibold">{{ detailsTask.title }}</h3>
                <p class="mt-1 text-xs text-muted-foreground">Created {{ formatTaskCreatedAt(detailsTask.createdAt) }}</p>
                <p class="mt-1 truncate text-xs text-muted-foreground" :title="detailsTask.directory">{{ detailsTask.directory }}</p>
              </div>
              <div class="flex shrink-0 items-center gap-2">
                <Button v-if="detailsTask.status === 'running'" variant="outline" size="sm" class="h-7 gap-1 px-2 text-xs" @click="downloads.pause(detailsTask.id)">
                  <Pause :size="12" />
                  Pause
                </Button>
                <Button v-else-if="detailsTask.status === 'paused' || detailsTask.status === 'cancelled' || detailsTask.error" variant="outline" size="sm" class="h-7 gap-1 px-2 text-xs" :disabled="!canResumeTasks" @click="downloads.resume(detailsTask.id)">
                  <Play :size="12" />
                  Restore
                </Button>
                <button type="button" class="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-primary" @click="openTaskFolder(detailsTask.directory)">
                  <FolderOpen :size="13" />
                  Open folder
                </button>
              </div>
            </div>

            <div class="mt-4 flex items-center gap-3">
              <div class="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-secondary">
                <div class="h-full rounded-full bg-primary transition-[width] duration-200" :class="detailsTask.status === 'cancelled' || detailsTask.error ? '!bg-warning' : ''" :style="{ width: `${percentOf(detailsTask) ?? 0}%` }" />
              </div>
              <span class="tabular shrink-0 text-xs text-muted-foreground"
                >{{ detailsTask.done + detailsTask.failed.length }}<template v-if="totalOf(detailsTask) !== null"> / {{ totalOf(detailsTask) }}</template></span
              >
              <span class="tabular shrink-0 text-xs text-muted-foreground">Transferred {{ detailsTransferredBytes === null ? "—" : formatBytes(detailsTransferredBytes) }}</span>
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

          <section class="flex min-h-0 flex-1 flex-col p-4">
            <div class="grid grid-cols-3 rounded-lg bg-secondary p-1" role="tablist" aria-label="Download task details">
              <button
                type="button"
                class="flex h-8 items-center justify-center gap-1.5 rounded-md text-xs transition-colors"
                :class="detailsTab === 'downloading' ? 'bg-card font-medium text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'"
                role="tab"
                :aria-selected="detailsTab === 'downloading'"
                @click="detailsTab = 'downloading'"
              >
                Downloading <span class="tabular text-2xs">{{ detailsDownloadingRows.length }}</span>
              </button>
              <button
                type="button"
                class="flex h-8 items-center justify-center gap-1.5 rounded-md text-xs transition-colors"
                :class="detailsTab === 'queued' ? 'bg-card font-medium text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'"
                role="tab"
                :aria-selected="detailsTab === 'queued'"
                @click="detailsTab = 'queued'"
              >
                Queued <span class="tabular text-2xs">{{ detailsQueue.pendingTotal + (detailsQueue.unfetched ?? 0) }}</span>
              </button>
              <button
                type="button"
                class="flex h-8 items-center justify-center gap-1.5 rounded-md text-xs transition-colors"
                :class="detailsTab === 'completed' ? 'bg-card font-medium text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'"
                role="tab"
                :aria-selected="detailsTab === 'completed'"
                @click="detailsTab = 'completed'"
              >
                Completed <span class="tabular text-2xs">{{ detailsCompletedFiles.length + detailsQueue.failed.length }}</span>
              </button>
            </div>

            <div v-if="detailsTab === 'downloading'" class="mt-4 flex min-h-0 flex-1 flex-col">
              <DownloadFileList v-if="detailsDownloadingRows.length" :rows="detailsDownloadingRows" fill @pause="downloads.pauseItem(detailsTask.id, $event)" @resume="downloads.resumeItem(detailsTask.id, $event)" class="mt-2" />
              <div v-else class="mt-2 flex min-h-0 flex-1 items-center justify-center gap-2 rounded-md border border-dashed border-border text-xs text-muted-foreground">
                <Spinner v-if="detailsTask.status === 'running'" :size="13" />
                {{ detailsTask.status === "running" ? "Preparing the next file…" : "No active downloads" }}
              </div>
            </div>

            <div v-else-if="detailsTab === 'queued'" class="mt-4 flex min-h-0 flex-1 flex-col">
              <div v-if="detailsQueue.pendingTotal > 0" class="flex min-h-0 flex-1 flex-col">
                <h4 class="text-xs font-medium text-muted-foreground">
                  Pending <span class="tabular">{{ detailsQueue.pendingTotal }}</span>
                </h4>
                <DownloadFileList :rows="detailsPendingRows" fill empty-label="No pending files" class="mt-2" @pause="downloads.pauseItem(detailsTask.id, $event)" />
              </div>
              <p v-if="detailsQueue.unfetched !== null && detailsQueue.unfetched > 0" class="mt-3 rounded-md border border-border bg-muted/35 px-3 py-2 text-xs text-muted-foreground">
                <span class="tabular font-medium text-foreground">{{ detailsQueue.unfetched }}</span> more waiting to be retrieved from iCloud.
              </p>
              <div v-if="detailsQueue.pendingTotal === 0 && (detailsQueue.unfetched ?? 0) === 0" class="mt-2 flex min-h-0 flex-1 items-center justify-center rounded-md border border-dashed border-border text-xs text-muted-foreground">No queued files</div>
            </div>

            <div v-else class="mt-4 flex min-h-0 flex-1 flex-col">
              <div v-if="detailsCompletedFiles.length" class="flex min-h-0 flex-1 flex-col">
                <div class="flex items-center gap-2">
                  <h4 class="flex items-center gap-1.5 text-xs font-medium text-success">
                    <Check :size="12" />Downloaded files <span class="tabular">{{ detailsCompletedFiles.length }}</span>
                  </h4>
                  <span class="tabular ml-auto text-2xs text-muted-foreground">{{ detailsCompletedBytes === null ? "Size unavailable" : formatBytes(detailsCompletedBytes) }}</span>
                </div>
                <DownloadFileList :rows="detailsCompletedRows" fill empty-label="No completed files" class="mt-2" />
              </div>
              <div v-if="detailsQueue.failed.length" class="mt-4">
                <div class="flex items-center gap-2">
                  <h4 class="flex items-center gap-1.5 text-xs font-medium text-warning">
                    <AlertTriangle :size="12" />Failed files <span class="tabular">{{ detailsQueue.failed.length }}</span>
                  </h4>
                  <Button v-if="(detailsTask.failedItems?.length ?? 0) > 0 && detailsTask.status !== 'running'" variant="outline" size="sm" class="ml-auto h-7 gap-1 px-2 text-xs" @click="downloads.retryFailed(detailsTask.id)">
                    <RotateCcw :size="12" />
                    Retry failed
                  </Button>
                </div>
                <DownloadFileList :rows="detailsFailedRows" :height="180" empty-label="No failed files" class="mt-2" />
              </div>
              <div v-if="!detailsCompletedFiles.length && !detailsQueue.failed.length" class="mt-2 flex min-h-0 flex-1 items-center justify-center rounded-md border border-dashed border-border text-xs text-muted-foreground">Nothing completed yet</div>
            </div>
          </section>
        </div>
      </div>
    </div>
  </div>
</template>
