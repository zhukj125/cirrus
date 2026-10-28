<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch, watchEffect } from "vue";
import { CalendarRange, Check, ChevronDown, Download, ImageOff, Images, Info, LayoutGrid, List, Play, RotateCcw, X } from "@lucide/vue";

import { Status, StorageKey } from "@/constants";
import { downloadPhoto, photosAssetDistribution, photosList, photosTotal } from "@/lib/api";
import { readStored, writeStored } from "@/lib/common/storage";
import { toast } from "@/lib/common/toast";
import { isSessionGoneMessage } from "@/lib/session-errors";
import { countLabel as photoCountLabel } from "@/lib/photos/count";
import { UNBOUNDED, addDays, filterByRange, formatDateInput, groupByDay, presetRange, type DateBounds, type RangeKey } from "@/lib/photos/dates";
import { monthBounds, segmentLanding, yearBounds, type SegmentLanding } from "@/lib/photos/distribution";
import { createExactRangeLoader, createExactRangeStore, type ExactRangeResult } from "@/lib/photos/exact";
import { mapWithConcurrency } from "@/lib/photos/concurrency";
import { photoPageStarts, photoUrlRefreshDelayMs, signedPhotoUrlExpiryMs } from "@/lib/photos/url-refresh";
import { passedLowerBound, passedUpperBound } from "@/lib/photos/range";
import { timelineSegment, timelineYearsDesc, type TimelineSelection } from "@/lib/photos/timeline";
import { PHOTO_VIEW_MODES, galleryGridClass, gallerySkeletonClass, galleryTileClass, isPhotoViewMode, type PhotoViewMode } from "@/lib/photos/view";
import Spinner from "@/components/ui/Spinner.vue";
import { Button } from "@/components/ui/button";
import PhotoDatePicker from "@/components/photos/PhotoDatePicker.vue";
import { useDownloadsStore } from "@/stores/downloads";
import { useLocaleStore } from "@/stores/locale";
import PhotoViewer from "@/components/PhotoViewer.vue";
import type { AssetDistribution, LibraryTotal, PhotoItem, Timeline } from "@/types";

const props = defineProps<{ sessionId: string }>();
const emit = defineEmits<{ logout: [] }>();
const downloads = useDownloadsStore();
const locale = useLocaleStore();
const t = locale.t;

const PAGE_SIZE = 60;
const VIEWER_PAGE_SIZE = 120;

const storedPhotoView = readStored(StorageKey.photosView);
const photoView = ref<PhotoViewMode>(isPhotoViewMode(storedPhotoView) ? storedPhotoView : "comfortable");
watch(photoView, (value) => writeStored(StorageKey.photosView, value));

const photoGridClass = computed(() => galleryGridClass(photoView.value));
const photoTileClass = computed(() => galleryTileClass(photoView.value));
const photoSkeletonClass = computed(() => gallerySkeletonClass(photoView.value));
const photoViewOptions = computed(() =>
  PHOTO_VIEW_MODES.map((mode) => ({
    ...mode,
    label: t(`photos.view${mode.id.charAt(0).toUpperCase()}${mode.id.slice(1)}`),
    title: t(`photos.view${mode.id.charAt(0).toUpperCase()}${mode.id.slice(1)}Title`),
  })),
);

const range = ref<RangeKey>("all");

const customBounds = ref<DateBounds>(UNBOUNDED);

const activeRange = ref<DateBounds>(UNBOUNDED);
function selectRange(key: RangeKey): void {
  timelineSelection.value = null;
  timelinePickerOpen.value = false;
  range.value = key;
  if (key === "all") {
    activeRange.value = UNBOUNDED;
    customBounds.value = UNBOUNDED;
    void reload();
    return;
  }
  if (key === "custom") {
    customBounds.value = activeRange.value;
    return;
  }
  activeRange.value = presetRange(key);
  ensureActiveRangeExact();
  void reload();
}

function onCustomRangeChange(value: DateBounds): void {
  customBounds.value = value;
  timelineSelection.value = null;
  timelinePickerOpen.value = false;
  if (value.after === null || value.before === null) {
    selectRange("all");
    return;
  }
  range.value = "custom";
  activeRange.value = value;
  ensureActiveRangeExact();
  void reload();
}

function selectRangeFromPicker(key: RangeKey): void {
  timelinePickerOpen.value = false;
  selectRange(key);
}

function selectCustomRangeFromPicker(value: DateBounds): void {
  timelinePickerOpen.value = false;
  onCustomRangeChange(value);
}

const rangeLabel = computed(() => {
  const { after, before } = activeRange.value;
  if (after === null || before === null) return null;
  return `${formatDateInput(after)}${locale.isEnglish ? " to " : " 至 "}${formatDateInput(addDays(before, -1))}`;
});

const isUnfiltered = computed(() => range.value === "all");

const photos = ref<PhotoItem[]>([]);

const loadedThumbUrls = reactive(new Set<string>());
const failedThumbUrls = reactive(new Set<string>());
let photoUrlRefreshTimer: ReturnType<typeof setTimeout> | null = null;
let photoUrlRefreshing = false;
const cursor = ref<string | null>(null);
const hasMore = ref(true);
const loading = ref(false);

const reloading = ref(false);
const busy = computed(() => loading.value || reloading.value);
const error = ref<string | null>(null);

const total = ref<LibraryTotal | null>(null);

const viewerIndex = ref<number | null>(null);
const viewerPhotos = ref<PhotoItem[] | null>(null);
const viewerBaseRank = ref(0);
let viewerSeekToken = 0;

const viewerList = computed(() => viewerPhotos.value ?? visiblePhotos.value);
const viewerTotalCount = computed(() => (isUnfiltered.value ? (total.value?.assets ?? null) : null));

const sentinel = ref<HTMLElement | null>(null);

const scroller = ref<HTMLElement | null>(null);

const distribution = ref<AssetDistribution | null>(null);

const timeline = ref<Timeline | null>(null);

const timelineLoading = ref(false);

const timelineSelection = ref<TimelineSelection | null>(null);

const timelinePickerOpen = ref(false);
const timelinePicker = ref<HTMLElement | null>(null);

const timelineYears = computed(() => timelineYearsDesc(timeline.value, locale.locale));
const timelineTriggerLabel = computed(() => {
  const selected = timelineSelection.value;
  if (selected) return selected.month === null ? `${selected.year}` : new Date(selected.year, selected.month - 1, 1).toLocaleDateString(locale.locale, { month: "short", year: "numeric" });
  if (range.value === "all") return t("photos.allTime");
  if (range.value === "today") return t("photos.rangeToday");
  if (range.value === "week") return t("photos.rangeWeek");
  if (range.value === "month") return t("photos.rangeMonth");
  if (range.value === "year") return t("photos.rangeYear");
  return rangeLabel.value ?? t("photos.customRange");
});

async function loadExactRangeDates(startRank: number, limit: number): Promise<readonly (number | null)[]> {
  const page = await photosList({ sessionId: props.sessionId, startRank, limit });
  if (page.status === Status.error) throw new Error(page.message ?? t("photos.loadRankFailed"));
  return page.photos.map((photo) => photo.takenAt);
}

let exactRangeReaderDistribution: AssetDistribution | null = null;
let exactRangeReader: ((bounds: DateBounds) => Promise<ExactRangeResult>) | null = null;

function readExactRange(bounds: DateBounds): Promise<ExactRangeResult> {
  const dist = distribution.value;
  if (!dist) return Promise.reject(new Error(t("photos.timelineUnavailable")));
  if (exactRangeReaderDistribution !== dist || exactRangeReader === null) {
    exactRangeReaderDistribution = dist;
    exactRangeReader = createExactRangeLoader(dist, loadExactRangeDates);
  }
  return exactRangeReader(bounds);
}

const exactRanges = createExactRangeStore(readExactRange);
const activeRangeExact = computed(() => (isUnfiltered.value ? null : exactRanges.cached(activeRange.value)));

const activeExactCount = computed(() => activeRangeExact.value?.count ?? null);

const exactCountLoading = computed(() => !isUnfiltered.value && exactRanges.isPending(activeRange.value));

function ensureActiveRangeExact(): void {
  if (isUnfiltered.value || !distribution.value) return;
  void exactRanges.ensure(activeRange.value);
}

const exactCounts = computed<Record<string, number>>(() => {
  const counts: Record<string, number> = {};
  for (const year of timeline.value?.years ?? []) {
    const yearExact = exactRanges.cached(yearBounds(year.year));
    if (yearExact) counts[String(year.year)] = yearExact.count;
    for (let month = 1; month <= 12; month += 1) {
      const monthExact = exactRanges.cached(monthBounds(year.year, month));
      if (monthExact) counts[`${year.year}-${String(month).padStart(2, "0")}`] = monthExact.count;
    }
  }
  return counts;
});

const exactPendingCounts = computed<Record<string, boolean>>(() => {
  const pending: Record<string, boolean> = {};
  for (const year of timeline.value?.years ?? []) {
    if (exactRanges.isPending(yearBounds(year.year))) pending[String(year.year)] = true;
    for (let month = 1; month <= 12; month += 1) {
      if (exactRanges.isPending(monthBounds(year.year, month))) pending[`${year.year}-${String(month).padStart(2, "0")}`] = true;
    }
  }
  return pending;
});

function ensureYearExact(yearNumber: number): void {
  const entry = timeline.value?.years.find((year) => year.year === yearNumber);
  if (!entry) return;
  const ranges: DateBounds[] = [yearBounds(yearNumber), ...Array.from({ length: 12 }, (_, index) => monthBounds(yearNumber, index + 1))];
  void mapWithConcurrency(ranges, 3, async (bounds) => {
    await exactRanges.ensure(bounds);
  }).catch(() => undefined);
}

let loadEpoch = 0;

const seenIds = new Set<string>();

const sessionExpired = ref(false);

const truncated = ref(false);

const rankCursor = ref<number | null>(null);

const rankDirection = ref<"older" | "newer">("older");

const MAX_CURSOR_PAGES = 40;

const RANK_INITIAL_PAGES = 8;

const RANK_SCROLL_PAGES = 4;

const RANK_REQUEST_CONCURRENCY = 4;

const visiblePhotos = computed(() => filterByRange(photos.value, activeRange.value));

const landing = computed<SegmentLanding | null>(() => {
  if (isUnfiltered.value) return null;
  const selection = timelineSelection.value;
  const entry = selection ? timelineSegment(timeline.value, selection) : null;
  const computedLanding = segmentLanding(distribution.value, activeRange.value, entry?.startRank ?? null);
  if (!computedLanding) return null;

  return entry ? { ...computedLanding, count: entry.count } : computedLanding;
});

const countLabel = computed(() => {
  const shown = isUnfiltered.value ? photos.value.length : visiblePhotos.value.length;
  return photoCountLabel(
    {
      shown,
      total: total.value?.assets ?? null,
      filtered: !isUnfiltered.value,
      hasMore: hasMore.value,
      truncated: truncated.value,
      exact: activeExactCount.value,
      exactLoading: exactCountLoading.value,
    },
    locale.isEnglish,
  );
});

async function loadTotal(sessionId: string): Promise<void> {
  try {
    const result = await photosTotal(sessionId);

    if (sessionId !== props.sessionId) return;
    total.value = result;
  } catch {
    if (sessionId !== props.sessionId) return;
    total.value = null;
  }
}

async function loadTimeline(sessionId: string): Promise<void> {
  timelineLoading.value = true;

  exactRanges.clear();
  exactRangeReader = null;
  exactRangeReaderDistribution = null;
  try {
    const result = await photosAssetDistribution(sessionId);

    if (sessionId !== props.sessionId) return;
    const usable = Array.isArray(result?.distribution?.buckets) && result.distribution.buckets.length > 0 && result.distribution.bucketWidthMs > 0;
    distribution.value = usable ? result.distribution : null;
    timeline.value = result?.timeline ?? null;
    ensureActiveRangeExact();
  } catch {
    if (sessionId !== props.sessionId) return;
    distribution.value = null;
    timeline.value = null;
  } finally {
    if (sessionId === props.sessionId) timelineLoading.value = false;
  }
}

function selectTimeline(selection: TimelineSelection): void {
  timelinePickerOpen.value = false;
  timelineSelection.value = selection;
  const bounds = selection.month === null ? yearBounds(selection.year) : monthBounds(selection.year, selection.month);
  customBounds.value = bounds;
  activeRange.value = bounds;
  range.value = "custom";

  void reload().then(() => {
    scroller.value?.scrollTo({ top: 0 });
    const current = timelineSelection.value;
    if (current?.year !== selection.year || current.month !== selection.month) return;
    ensureActiveRangeExact();
  });
}

function continueLoading(): void {
  requestMore();
}

function requestMore(): void {
  if (busy.value || !hasMore.value) return;
  truncated.value = false;
  void (rankCursor.value !== null ? loadRankPages(loadEpoch, RANK_SCROLL_PAGES) : fillGallery(loadEpoch));
}

let scrollFrame: number | null = null;
function onGalleryScroll(): void {
  if (scrollFrame !== null) return;
  scrollFrame = requestAnimationFrame(() => {
    scrollFrame = null;
    const el = scroller.value;
    if (!el || el.scrollHeight - el.scrollTop - el.clientHeight > 800) return;
    requestMore();
  });
}

const expectedPhotoCount = computed(() => (!isUnfiltered.value ? (activeExactCount.value ?? landing.value?.count ?? null) : null));

const coalescedSkeleton = computed(() => reloading.value && expectedPhotoCount.value !== null && expectedPhotoCount.value <= PAGE_SIZE);
const showInitialSkeleton = computed(() => coalescedSkeleton.value || (busy.value && visiblePhotos.value.length === 0));

const skeletonCount = computed(() => {
  if (!busy.value) return 0;
  const expected = expectedPhotoCount.value;
  if (expected !== null) {
    const remaining = Math.max(0, expected - visiblePhotos.value.length);
    return Math.min(18, remaining === 0 && visiblePhotos.value.length === 0 ? 6 : remaining);
  }
  return visiblePhotos.value.length === 0 ? 12 : hasMore.value || truncated.value ? 4 : 0;
});

const sections = computed(() => groupByDay(visiblePhotos.value, new Date(), locale.locale));

const selectedPhotoIds = reactive(new Set<string>());
const selectionAnchorIndex = ref<number | null>(null);
const contextMenu = ref<{ photo: PhotoItem; x: number; y: number } | null>(null);
const detailsPhoto = ref<PhotoItem | null>(null);
const downloadingPhotoId = ref<string | null>(null);
const selectedCount = computed(() => selectedPhotoIds.size);
const selectedPhotos = computed(() => visiblePhotos.value.filter((photo) => selectedPhotoIds.has(photo.id)));

interface PhotoSelectionDrag {
  startIndex: number;
  lastIndex: number;
  startX: number;
  startY: number;
  active: boolean;
}

let photoSelectionDrag: PhotoSelectionDrag | null = null;
let suppressPhotoClickUntil = 0;

function clearPhotoSelection(): void {
  selectedPhotoIds.clear();
  selectionAnchorIndex.value = null;
}

watch(activeRange, clearPhotoSelection, { deep: true });

function addPhotoIndexRange(from: number, to: number): void {
  const start = Math.min(from, to);
  const end = Math.max(from, to);
  for (let index = start; index <= end; index += 1) {
    const photo = visiblePhotos.value[index];
    if (photo) selectedPhotoIds.add(photo.id);
  }
}

function onSelectionPointerMove(event: PointerEvent): void {
  const drag = photoSelectionDrag;
  if (!drag) return;
  if (!drag.active) {
    if (Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < 5) return;
    drag.active = true;
    selectionAnchorIndex.value = drag.startIndex;
    addPhotoIndexRange(drag.startIndex, drag.startIndex);
  }
  event.preventDefault();

  const tile = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>("[data-photo-index]");
  if (!tile) return;
  const index = Number(tile.dataset.photoIndex);
  if (!Number.isInteger(index) || index < 0 || index >= visiblePhotos.value.length || index === drag.lastIndex) return;
  addPhotoIndexRange(drag.lastIndex, index);
  drag.lastIndex = index;
}

function stopSelectionTracking(): void {
  window.removeEventListener("pointermove", onSelectionPointerMove);
  window.removeEventListener("pointerup", endPhotoSelection);
  window.removeEventListener("pointercancel", endPhotoSelection);
}

function endPhotoSelection(): void {
  if (photoSelectionDrag?.active) suppressPhotoClickUntil = performance.now() + 320;
  stopSelectionTracking();
  photoSelectionDrag = null;
}

function onPhotoPointerDown(entry: { index: number }, event: PointerEvent): void {
  if (event.button !== 0) return;
  contextMenu.value = null;
  photoSelectionDrag = {
    startIndex: entry.index,
    lastIndex: entry.index,
    startX: event.clientX,
    startY: event.clientY,
    active: false,
  };
  window.addEventListener("pointermove", onSelectionPointerMove, { passive: false });
  window.addEventListener("pointerup", endPhotoSelection);
  window.addEventListener("pointercancel", endPhotoSelection);
}

function togglePhotoSelection(index: number): void {
  const photo = visiblePhotos.value[index];
  if (!photo) return;
  if (selectedPhotoIds.has(photo.id)) selectedPhotoIds.delete(photo.id);
  else selectedPhotoIds.add(photo.id);
  selectionAnchorIndex.value = index;
}

function onPhotoClick(entry: { index: number }, event: MouseEvent): void {
  if (performance.now() < suppressPhotoClickUntil) return;
  if (event.shiftKey && selectionAnchorIndex.value !== null) {
    addPhotoIndexRange(selectionAnchorIndex.value, entry.index);
    return;
  }
  if (selectedPhotoIds.size > 0 || event.ctrlKey || event.metaKey) {
    togglePhotoSelection(entry.index);
    return;
  }
  openViewer(entry.index);
}

function onPhotoKeyboard(entry: { index: number }): void {
  if (selectedPhotoIds.size > 0) togglePhotoSelection(entry.index);
  else openViewer(entry.index);
}

function showPhotoContextMenu(entry: { photo: PhotoItem }, event: MouseEvent): void {
  event.preventDefault();
  const width = 208;
  const height = 154;
  contextMenu.value = {
    photo: entry.photo,
    x: Math.max(8, Math.min(event.clientX, window.innerWidth - width - 8)),
    y: Math.max(8, Math.min(event.clientY, window.innerHeight - height - 8)),
  };
}

function openPhotoDetails(photo: PhotoItem): void {
  contextMenu.value = null;
  detailsPhoto.value = photo;
}

function openViewerForPhoto(photo: PhotoItem): void {
  const index = visiblePhotos.value.findIndex((item) => item.id === photo.id);
  if (index < 0) return;
  detailsPhoto.value = null;
  openViewer(index);
}

function selectPhotoFromContext(photo: PhotoItem): void {
  const index = visiblePhotos.value.findIndex((item) => item.id === photo.id);
  if (index >= 0) {
    selectedPhotoIds.add(photo.id);
    selectionAnchorIndex.value = index;
  }
  contextMenu.value = null;
}

function photoType(photo: PhotoItem): string {
  if (photo.isVideo) return t("common.video");
  if (photo.isLivePhoto) return t("photos.typeLivePhoto");
  return t("photos.typePhoto");
}

function formatPhotoTime(takenAt: number | null | undefined): string {
  if (!takenAt) return "—";
  return new Date(takenAt).toLocaleString(locale.locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function photoDimensions(photo: PhotoItem): string {
  return photo.width && photo.height ? `${photo.width} × ${photo.height}` : "—";
}

function photoFileName(photo: PhotoItem): string {
  return photo.fileName || photo.id;
}

function photoDownloadUrl(photo: PhotoItem): string | null {
  return photo.originalUrl || photo.displayUrl || photo.thumbUrl || null;
}

async function downloadSinglePhoto(photo: PhotoItem): Promise<void> {
  if (downloadingPhotoId.value) return;
  const url = photoDownloadUrl(photo);
  if (!url) {
    toast.error(t("photos.noDownloadUrl"));
    return;
  }
  downloadingPhotoId.value = photo.id;
  try {
    const savedPath = await downloadPhoto(url, photoFileName(photo), t("photos.saveTo"));
    if (savedPath) toast.success(t("photos.savedTo", { path: savedPath }));
  } catch (caught) {
    toast.error(t("photos.downloadFailed", { error: caught instanceof Error ? caught.message : String(caught) }));
  } finally {
    downloadingPhotoId.value = null;
  }
}

function downloadSelectedPhotos(): void {
  const items = selectedPhotos.value.slice();
  if (items.length === 0) return;
  downloads.openDialogWithPhotos(items);
  clearPhotoSelection();
}

function closePhotoOverlays(): void {
  contextMenu.value = null;
  detailsPhoto.value = null;
}

function onOverlayKeydown(event: KeyboardEvent): void {
  if (event.key !== "Escape") return;
  if (contextMenu.value || detailsPhoto.value) {
    closePhotoOverlays();
    return;
  }
  if (timelinePickerOpen.value) {
    timelinePickerOpen.value = false;
    return;
  }
  if (selectedCount.value > 0) clearPhotoSelection();
}

function onDocumentPointerDown(event: PointerEvent): void {
  if (!timelinePickerOpen.value || timelinePicker.value?.contains(event.target as Node)) return;
  timelinePickerOpen.value = false;
}

onMounted(() => {
  window.addEventListener("keydown", onOverlayKeydown);
  document.addEventListener("pointerdown", onDocumentPointerDown);
});
onUnmounted(() => {
  window.removeEventListener("keydown", onOverlayKeydown);
  document.removeEventListener("pointerdown", onDocumentPointerDown);
  stopSelectionTracking();
  clearPhotoUrlRefreshTimer();
});

function markThumbnailLoaded(url: string): void {
  failedThumbUrls.delete(url);
  loadedThumbUrls.add(url);
}

function markThumbnailFailed(photo: PhotoItem): void {
  const url = photo.thumbUrl;
  loadedThumbUrls.delete(url);
  failedThumbUrls.add(url);
  const expiry = signedPhotoUrlExpiryMs(url);
  if (expiry !== null && expiry <= Date.now()) void refreshLoadedPhotoUrls();
}

function clearPhotoUrlRefreshTimer(): void {
  if (photoUrlRefreshTimer !== null) {
    clearTimeout(photoUrlRefreshTimer);
    photoUrlRefreshTimer = null;
  }
}

function schedulePhotoUrlRefresh(): void {
  clearPhotoUrlRefreshTimer();
  const delay = photoUrlRefreshDelayMs(photos.value);
  if (delay === null) return;
  photoUrlRefreshTimer = setTimeout(() => {
    photoUrlRefreshTimer = null;
    void refreshLoadedPhotoUrls();
  }, delay);
}

async function refreshLoadedPhotoUrls(): Promise<void> {
  if (photoUrlRefreshing || sessionExpired.value || photos.value.length === 0) return;
  photoUrlRefreshing = true;
  clearPhotoUrlRefreshTimer();
  const epoch = loadEpoch;
  const sessionId = props.sessionId;
  const starts = photoPageStarts(photos.value.length, PAGE_SIZE);

  try {
    const pages = await mapWithConcurrency(starts, 3, (startRank) => photosList({ sessionId, startRank, limit: PAGE_SIZE }));
    if (epoch !== loadEpoch || sessionId !== props.sessionId) return;

    const replacements = new Map<string, PhotoItem>();
    for (const page of pages) {
      if (page.status === Status.error) continue;
      for (const item of page.photos) replacements.set(item.id, item);
    }
    if (replacements.size === 0) return;

    const previous = new Map(photos.value.map((item) => [item.id, item]));
    const applyReplacements = (items: PhotoItem[]): PhotoItem[] =>
      items.map((item) => {
        const next = replacements.get(item.id);
        if (!next) return item;
        const old = previous.get(item.id);
        if (old?.thumbUrl && old.thumbUrl !== next.thumbUrl) {
          loadedThumbUrls.delete(old.thumbUrl);
          failedThumbUrls.delete(old.thumbUrl);
        }
        return next;
      });

    photos.value = applyReplacements(photos.value);
    if (viewerPhotos.value) viewerPhotos.value = applyReplacements(viewerPhotos.value);
  } catch (error) {
    console.warn("[PHOTOS] could not refresh signed image URLs", error);
  } finally {
    photoUrlRefreshing = false;
    schedulePhotoUrlRefresh();
  }
}

watch(photos, () => schedulePhotoUrlRefresh(), { flush: "post" });

function resetThumbnailState(): void {
  loadedThumbUrls.clear();
  failedThumbUrls.clear();
}

function handleExpiredSession(message: string | null): boolean {
  if (!isSessionGoneMessage(message)) return false;
  sessionExpired.value = true;
  toast.error(message ?? t("photos.sessionExpired"));
  emit("logout");
  return true;
}

async function loadPage(pageCursor: string | null, replace: boolean, epoch = loadEpoch): Promise<void> {
  if (epoch !== loadEpoch || sessionExpired.value || loading.value) return;
  const sessionId = props.sessionId;
  loading.value = true;
  error.value = null;
  try {
    const page = await photosList({ sessionId, cursor: pageCursor, limit: PAGE_SIZE });
    if (epoch !== loadEpoch || sessionId !== props.sessionId) return;
    if (page.status === Status.error) {
      if (handleExpiredSession(page.message)) return;
      error.value = page.message ?? t("photos.loadFailed");
      hasMore.value = false;
      return;
    }

    let stalled = false;
    if (replace) {
      seenIds.clear();
      page.photos.forEach((item) => seenIds.add(item.id));
      photos.value = page.photos;
    } else {
      const fresh = page.photos.filter((item) => !seenIds.has(item.id));
      stalled = fresh.length === 0 && page.photos.length > 0;
      fresh.forEach((item) => seenIds.add(item.id));
      photos.value = [...photos.value, ...fresh];
    }
    cursor.value = page.nextCursor;
    hasMore.value = !stalled && page.nextCursor != null && page.photos.length > 0;
  } catch (caught) {
    if (epoch !== loadEpoch) return;
    error.value = caught instanceof Error ? caught.message : t("photos.networkFailed");
    hasMore.value = false;
  } finally {
    if (epoch === loadEpoch) loading.value = false;
  }
}

async function startFromNewest(epoch: number): Promise<void> {
  await loadPage(null, true, epoch);
  if (epoch !== loadEpoch || sessionExpired.value || !hasMore.value || cursor.value === null) return;

  rankDirection.value = "older";
  rankCursor.value = photos.value.length + PAGE_SIZE - 1;
  await loadRankPages(epoch, RANK_INITIAL_PAGES);
}

function resetState(): void {
  loading.value = false;
  seenIds.clear();
  resetThumbnailState();
  photos.value = [];
  cursor.value = null;
  rankCursor.value = null;
  rankDirection.value = "older";
  hasMore.value = true;
  error.value = null;
  viewerIndex.value = null;
  truncated.value = false;
}

function plannedRankPageCount(pageLimit: number): number {
  const expected = expectedPhotoCount.value;
  if (expected === null) return pageLimit;
  const remaining = Math.max(0, expected - visiblePhotos.value.length);
  return Math.min(pageLimit, Math.max(1, Math.ceil(remaining / PAGE_SIZE)));
}

async function loadRankPages(epoch: number, pageLimit = RANK_SCROLL_PAGES): Promise<void> {
  if (epoch !== loadEpoch || sessionExpired.value || loading.value || rankCursor.value === null) return;
  const sessionId = props.sessionId;
  const startRank = rankCursor.value;
  const direction = rankDirection.value;
  const starts = Array.from({ length: plannedRankPageCount(pageLimit) }, (_, index) => (direction === "older" ? startRank + index * PAGE_SIZE : startRank - index * PAGE_SIZE));
  loading.value = true;
  error.value = null;
  truncated.value = false;

  let results: Array<{ rank: number; page: Awaited<ReturnType<typeof photosList>> }> | null = null;
  let requestError: string | null = null;
  try {
    results = await mapWithConcurrency(starts, RANK_REQUEST_CONCURRENCY, async (rank) => {
      const page = await photosList({ sessionId, startRank: rank, limit: PAGE_SIZE });
      if (epoch !== loadEpoch || sessionId !== props.sessionId) throw new Error("stale-photo-request");
      return { rank, page };
    });
  } catch (caught) {
    requestError = caught instanceof Error ? caught.message : t("photos.networkFailed");
  } finally {
    if (epoch === loadEpoch) loading.value = false;
  }

  if (epoch !== loadEpoch || requestError === "stale-photo-request") return;

  const failedPage = results?.find(({ page }) => page.status === Status.error);
  if (requestError !== null || failedPage !== undefined) {
    if (failedPage && handleExpiredSession(failedPage.page.message)) return;
    await fallbackToPaging(epoch);
    return;
  }
  if (results === null) return;

  let nextRank = startRank;
  let finished = false;
  const fresh: PhotoItem[] = [];
  for (const { rank, page } of results) {
    if (rank !== nextRank) {
      finished = true;
      break;
    }
    if (page.photos.length === 0) {
      finished = true;
      break;
    }
    for (const item of page.photos) {
      if (seenIds.has(item.id)) continue;
      seenIds.add(item.id);
      fresh.push(item);
    }
    nextRank = rank + (direction === "older" ? page.photos.length : -page.photos.length);

    if (direction === "newer") {
      if (passedUpperBound(page.photos, activeRange.value) || nextRank < 0) finished = true;
    } else {
      const collectionTotal = distribution.value?.total ?? total.value?.assets ?? null;
      if (passedLowerBound(fresh, activeRange.value) || (collectionTotal !== null && nextRank >= collectionTotal)) finished = true;
    }
    if (finished) break;
  }

  fresh.sort((a, b) => (b.takenAt ?? 0) - (a.takenAt ?? 0));
  photos.value = direction === "newer" ? [...fresh, ...photos.value] : [...photos.value, ...fresh];
  rankCursor.value = nextRank;
  hasMore.value = !finished;
  truncated.value = !finished;
}

async function fallbackToPaging(epoch: number): Promise<void> {
  if (epoch !== loadEpoch) return;
  rankCursor.value = null;
  seenIds.clear();
  resetThumbnailState();
  photos.value = [];
  cursor.value = null;
  hasMore.value = true;
  error.value = null;
  truncated.value = false;
  await loadPage(null, true, epoch);
  if (epoch === loadEpoch) await fillGallery(epoch);
}

async function reload(): Promise<void> {
  const epoch = ++loadEpoch;
  reloading.value = true;
  resetState();
  try {
    const target = landing.value?.startRank ?? null;
    rankDirection.value = activeRange.value.before === null ? "older" : "newer";
    if (target !== null && (rankDirection.value === "newer" || target > 0)) {
      rankCursor.value = target;
      await loadRankPages(epoch, RANK_INITIAL_PAGES);
      return;
    }
    await startFromNewest(epoch);
  } finally {
    if (epoch === loadEpoch) reloading.value = false;
  }
}

function retry(): void {
  if (rankCursor.value !== null) {
    void loadRankPages(loadEpoch, RANK_SCROLL_PAGES);
    return;
  }
  const epoch = loadEpoch;
  void loadPage(photos.value.length > 0 ? cursor.value : null, photos.value.length === 0, epoch).then(() => fillGallery(epoch));
}

async function fillGallery(epoch = loadEpoch): Promise<void> {
  truncated.value = false;
  let pages = 0;
  while (epoch === loadEpoch && !sessionExpired.value && hasMore.value && cursor.value !== null && pages < MAX_CURSOR_PAGES) {
    if (passedLowerBound(photos.value, activeRange.value)) {
      hasMore.value = false;
      return;
    }
    const countBefore = photos.value.length;
    await loadPage(cursor.value, false, epoch);
    if (epoch !== loadEpoch) return;
    if (photos.value.length === countBefore) break;
    pages += 1;
  }
  truncated.value = hasMore.value && pages >= MAX_CURSOR_PAGES;
}

function openViewer(index: number): void {
  viewerSeekToken += 1;
  viewerPhotos.value = [...visiblePhotos.value];
  viewerBaseRank.value = 0;
  viewerIndex.value = index;
}

function closeViewer(): void {
  viewerSeekToken += 1;
  viewerIndex.value = null;
  viewerPhotos.value = null;
  viewerBaseRank.value = 0;
}

async function seekViewerRank(rank: number): Promise<void> {
  const total = viewerTotalCount.value;
  const target = Math.max(0, Math.min(rank, (total ?? 1) - 1));
  const halfPage = Math.floor(VIEWER_PAGE_SIZE / 2);
  const maxStart = Math.max(0, (total ?? VIEWER_PAGE_SIZE) - VIEWER_PAGE_SIZE);
  const pageStart = Math.min(Math.max(0, target - halfPage), maxStart);
  const token = ++viewerSeekToken;
  try {
    const page = await photosList({ sessionId: props.sessionId, startRank: pageStart, limit: VIEWER_PAGE_SIZE });
    if (token !== viewerSeekToken) return;
    if (page.status === Status.error) {
      toast.error(page.message ?? t("photos.loadRankFailed"));
      return;
    }
    if (page.photos.length === 0) {
      toast.error(t("photos.loadRankFailed"));
      return;
    }
    const targetIndex = Math.min(page.photos.length - 1, Math.max(0, target - pageStart));
    viewerPhotos.value = page.photos;
    viewerBaseRank.value = pageStart;
    viewerIndex.value = targetIndex;
  } catch (caught) {
    if (token !== viewerSeekToken) return;
    toast.error(t("photos.loadRankFailed"));
  }
}

void startFromNewest(loadEpoch);

void loadTotal(props.sessionId);
void loadTimeline(props.sessionId);

watchEffect((onCleanup) => {
  const el = sentinel.value;
  if (!el) return;
  const observer = new IntersectionObserver(
    (entries) => {
      if (entries[0]?.isIntersecting !== true) return;

      requestMore();
    },
    { rootMargin: "600px" },
  );
  observer.observe(el);
  onCleanup(() => observer.disconnect());
});

watch(
  () => props.sessionId,
  () => {
    sessionExpired.value = false;
    range.value = "all";
    activeRange.value = { after: null, before: null };
    timelineSelection.value = null;
    timelinePickerOpen.value = false;
    clearPhotoSelection();
    closePhotoOverlays();

    total.value = null;
    distribution.value = null;
    timeline.value = null;
    exactRanges.clear();
    exactRangeReader = null;
    exactRangeReaderDistribution = null;
    void reload();
    void loadTotal(props.sessionId);
    void loadTimeline(props.sessionId);
  },
);
</script>

<template>
  <div class="relative flex h-full min-h-0 flex-col">
    <div class="flex h-11 shrink-0 items-center gap-2 border-b border-border bg-card/55 px-4 text-sm text-muted-foreground">
      <div ref="timelinePicker" class="relative shrink-0">
        <button
          type="button"
          class="flex h-8 max-w-[330px] items-center gap-2 rounded-md border border-border/70 bg-background px-2.5 text-sm text-foreground shadow-sm transition-colors hover:bg-accent focus-visible:border-primary focus-visible:ring-2 focus-visible:ring-ring/25"
          :aria-expanded="timelinePickerOpen"
          aria-haspopup="dialog"
          :title="t('photos.selectDateRange')"
          @click="timelinePickerOpen = !timelinePickerOpen"
        >
          <CalendarRange :size="14" class="shrink-0 text-muted-foreground" />
          <span class="tabular min-w-0 truncate">{{ timelineTriggerLabel }}</span>
          <ChevronDown :size="13" class="shrink-0 text-muted-foreground transition-transform" :class="timelinePickerOpen ? 'rotate-180' : ''" />
        </button>

        <div v-if="timelinePickerOpen" class="absolute top-full left-0 z-[1200] mt-2" role="dialog" :aria-label="t('photos.selectDateRange')">
          <PhotoDatePicker
            :active-range="range"
            :custom-value="customBounds"
            :custom-label="rangeLabel"
            :years="timelineYears"
            :selected="timelineSelection"
            :loading="timelineLoading"
            :exact-counts="exactCounts"
            :pending-counts="exactPendingCounts"
            @select-range="selectRangeFromPicker"
            @select-custom-range="selectCustomRangeFromPicker"
            @select-timeline="selectTimeline"
            @ensure-year="ensureYearExact"
          />
        </div>
      </div>

      <div class="ml-auto flex min-w-0 items-center gap-2">
        <div class="relative shrink-0">
          <LayoutGrid v-if="photoView !== 'list'" :size="13" class="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-muted-foreground" />
          <List v-else :size="13" class="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-muted-foreground" />
          <select
            v-model="photoView"
            class="h-7 appearance-none rounded-md border border-border/70 bg-background pr-7 pl-7 text-xs text-muted-foreground shadow-sm outline-none transition-colors hover:bg-accent hover:text-foreground focus:border-primary"
            :title="t('photos.layout')"
            :aria-label="t('photos.layout')"
          >
            <option v-for="mode in photoViewOptions" :key="mode.id" :value="mode.id" :title="mode.title">{{ mode.label }}</option>
          </select>
          <ChevronDown :size="12" class="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 text-muted-foreground" />
        </div>
        <button
          type="button"
          class="flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-border/70 bg-background px-2.5 text-sm text-muted-foreground shadow-sm transition-colors hover:bg-accent hover:text-foreground"
          :class="downloads.dialogOpen ? 'bg-accent text-foreground' : ''"
          :title="isUnfiltered ? t('photos.downloadAll') : t('photos.downloadRange')"
          @click="downloads.openDialog(isUnfiltered ? null : activeRange)"
        >
          <Download :size="13" />
          {{ t("photos.download") }}
        </button>
        <span class="h-4 w-px shrink-0 bg-border" />
        <span class="tabular max-w-[280px] shrink-0 truncate rounded-md bg-secondary/70 px-2.5 py-1 text-xs" :title="countLabel">{{ countLabel }}</span>

        <span v-if="error" class="flex min-w-0 items-center gap-2">
          <span class="truncate text-destructive" :title="error">{{ error }}</span>
          <button type="button" class="shrink-0 rounded-md px-1.5 py-0.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" @click="retry">{{ t("common.retry") }}</button>
        </span>

        <span v-if="busy" class="flex shrink-0 items-center gap-1.5">
          <Spinner :size="12" />
          {{ t("photos.loading") }}
        </span>
      </div>
    </div>

    <div class="flex min-h-0 flex-1">
      <div ref="scroller" class="min-h-0 flex-1 overflow-y-auto px-4 pt-1 pb-4" @scroll.passive="onGalleryScroll">
        <div v-if="error && photos.length === 0" class="grid h-full place-items-center">
          <div class="flex flex-col items-center gap-2.5 text-sm text-muted-foreground">
            <ImageOff :size="22" />
            <p>{{ error }}</p>
            <Button size="sm" variant="outline" @click="retry">{{ t("common.retry") }}</Button>
          </div>
        </div>

        <div v-else-if="showInitialSkeleton" class="pb-3">
          <div class="flex items-center gap-2.5 py-3">
            <div class="h-6 w-32 animate-pulse rounded bg-muted" aria-hidden="true" />
            <div class="h-4 w-10 animate-pulse rounded bg-muted" aria-hidden="true" />
          </div>
          <div class="mb-5 grid" :class="photoGridClass">
            <div v-for="index in skeletonCount" :key="`skeleton-empty-${index}`" data-testid="photo-skeleton" class="animate-pulse overflow-hidden bg-muted" :class="photoSkeletonClass" :style="{ animationDelay: `${(index % 6) * 70}ms` }" aria-hidden="true">
              <div class="size-full bg-gradient-to-br from-muted via-secondary to-muted" />
            </div>
          </div>
        </div>

        <div v-else-if="!busy && visiblePhotos.length === 0" class="grid h-full place-items-center">
          <div class="flex flex-col items-center gap-2.5 text-sm text-muted-foreground">
            <Images :size="22" />
            <p>{{ isUnfiltered ? t("photos.emptyLibrary") : t("photos.emptyRange") }}</p>

            <p v-if="truncated" class="max-w-[280px] text-xs text-warning">
              {{ isUnfiltered ? t("photos.largeLibraryHint") : t("photos.olderRangeHint") }}
            </p>
            <Button v-if="!isUnfiltered" size="sm" variant="outline" @click="selectRange('all')">
              <RotateCcw :size="13" />
              {{ t("photos.clearFilters") }}
            </Button>
          </div>
        </div>

        <template v-else>
          <section v-for="section in sections" :key="section.key">
            <div class="sticky top-0 z-10 -mx-4 flex items-center gap-2.5 border-b border-border/40 bg-background/92 px-4 py-2.5 backdrop-blur-sm">
              <h2 class="shrink-0 text-sm font-semibold">{{ section.label }}</h2>
              <span class="tabular shrink-0 rounded-full bg-secondary px-2 py-0.5 text-2xs text-muted-foreground">{{ t("photos.photoCount", { count: section.items.length }) }}</span>
              <span class="h-px flex-1 bg-border/60" aria-hidden="true" />
            </div>

            <div class="mb-5 grid" :class="photoGridClass">
              <figure
                v-for="entry in section.items"
                :key="entry.photo.id"
                :data-photo-id="entry.photo.id"
                :data-photo-index="entry.index"
                class="group relative cursor-zoom-in touch-pan-y overflow-hidden border border-border/50 shadow-sm select-none transition-[border-color,box-shadow,transform] duration-200 hover:border-primary/30 hover:shadow-md [content-visibility:auto]"
                :class="[photoTileClass, selectedPhotoIds.has(entry.photo.id) ? 'border-primary/40' : '']"
                role="button"
                tabindex="0"
                :aria-busy="Boolean(entry.photo.thumbUrl) && !loadedThumbUrls.has(entry.photo.thumbUrl) && !failedThumbUrls.has(entry.photo.thumbUrl)"
                :aria-label="t('photos.photoAria', { name: entry.photo.fileName ?? t('photos.photoNameFallback') })"
                @pointerdown="onPhotoPointerDown(entry, $event)"
                @click="onPhotoClick(entry, $event)"
                @keydown.enter.prevent="onPhotoKeyboard(entry)"
                @keydown.space.prevent="onPhotoKeyboard(entry)"
                @contextmenu="showPhotoContextMenu(entry, $event)"
                @dragstart.prevent
              >
                <div class="overflow-hidden bg-muted" :class="photoView === 'list' ? 'relative h-14 w-20 shrink-0 rounded-md' : 'absolute inset-0 rounded-[inherit]'">
                  <div
                    v-if="entry.photo.thumbUrl && !loadedThumbUrls.has(entry.photo.thumbUrl)"
                    data-testid="photo-thumb-placeholder"
                    class="absolute inset-0 grid place-items-center bg-gradient-to-br from-muted via-secondary to-muted text-muted-foreground"
                    :class="failedThumbUrls.has(entry.photo.thumbUrl) ? '' : 'animate-pulse'"
                    aria-hidden="true"
                  >
                    <ImageOff v-if="failedThumbUrls.has(entry.photo.thumbUrl)" :size="20" />
                  </div>

                  <img
                    v-if="entry.photo.thumbUrl"
                    class="absolute inset-0 size-full object-cover transition-transform duration-300 group-hover:scale-[1.025]"
                    :class="loadedThumbUrls.has(entry.photo.thumbUrl) ? 'opacity-100' : 'opacity-0'"
                    :src="entry.photo.thumbUrl"
                    :alt="entry.photo.fileName ?? t('photos.photoNameFallback')"
                    :loading="entry.index < 24 ? 'eager' : 'lazy'"
                    :fetchpriority="entry.index < 12 ? 'high' : 'auto'"
                    decoding="async"
                    draggable="false"
                    @load="markThumbnailLoaded(entry.photo.thumbUrl)"
                    @error="markThumbnailFailed(entry.photo)"
                  />

                  <span v-else class="grid size-full place-items-center text-muted-foreground">
                    <ImageOff :size="20" />
                  </span>

                  <template v-if="entry.photo.thumbUrl && loadedThumbUrls.has(entry.photo.thumbUrl)">
                    <span class="pointer-events-none absolute inset-0 bg-black/15 opacity-0 transition-opacity group-hover:opacity-100" />

                    <span v-if="entry.photo.isLivePhoto && !entry.photo.isVideo" class="absolute top-2 left-2 rounded-md bg-black/60 px-1.5 py-0.5 text-2xs font-medium tracking-wide text-white backdrop-blur-sm">{{ t("photos.liveBadge") }}</span>

                    <span v-if="entry.photo.isVideo" class="absolute right-2 bottom-2 flex items-center gap-1 rounded-md bg-black/60 px-1.5 py-0.5 text-2xs text-white backdrop-blur-sm">
                      <Play :size="9" class="fill-current" />
                      {{ t("common.video") }}
                    </span>
                  </template>
                </div>

                <div v-if="photoView === 'list'" class="min-w-0 flex-1 py-0.5 pr-8">
                  <p class="truncate text-sm font-medium text-foreground">{{ photoFileName(entry.photo) }}</p>
                  <p class="mt-1 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
                    <span class="truncate">{{ formatPhotoTime(entry.photo.takenAt) }}</span>
                    <span aria-hidden="true">·</span>
                    <span class="shrink-0">{{ photoType(entry.photo) }}</span>
                    <template v-if="entry.photo.width && entry.photo.height">
                      <span aria-hidden="true">·</span>
                      <span class="tabular shrink-0">{{ photoDimensions(entry.photo) }}</span>
                    </template>
                  </p>
                </div>

                <span v-if="selectedPhotoIds.has(entry.photo.id)" class="pointer-events-none absolute inset-0 z-[2] rounded-[inherit] bg-primary/15 ring-2 ring-primary ring-inset" aria-hidden="true" />
                <span
                  class="pointer-events-none absolute right-2 z-[3] grid size-5 place-items-center rounded-full border shadow-sm transition-all"
                  :class="[selectedPhotoIds.has(entry.photo.id) ? 'border-primary bg-primary text-primary-foreground' : 'border-white/70 bg-black/45 text-white opacity-0 group-hover:opacity-100', photoView === 'list' ? 'top-1/2 -translate-y-1/2' : 'top-2']"
                  aria-hidden="true"
                >
                  <Check v-if="selectedPhotoIds.has(entry.photo.id)" :size="12" :stroke-width="3" />
                  <span v-else class="size-1.5 rounded-full bg-white/90" />
                </span>
              </figure>
            </div>
          </section>

          <div v-if="skeletonCount > 0" class="mb-5 grid" :class="photoGridClass">
            <div v-for="index in skeletonCount" :key="`skeleton-more-${index}`" data-testid="photo-skeleton" class="animate-pulse overflow-hidden bg-muted" :class="photoSkeletonClass" :style="{ animationDelay: `${(index % 6) * 70}ms` }" aria-hidden="true">
              <div class="size-full bg-gradient-to-br from-muted via-secondary to-muted" />
            </div>
          </div>

          <div v-if="hasMore && busy" class="pointer-events-none flex justify-center px-3 pt-2 pb-2">
            <div class="flex min-h-8 items-center rounded-full border border-border bg-background/90 px-3 py-1.5 text-xs text-muted-foreground shadow-lg backdrop-blur" role="status" aria-live="polite">
              <Spinner :size="13" />
              <span class="ml-2">{{ t("photos.loadingMore") }}</span>
              <span class="ml-1 flex items-center gap-0.5" aria-hidden="true">
                <span v-for="index in 3" :key="`loading-dot-${index}`" class="size-1 animate-bounce rounded-full bg-current" :style="{ animationDelay: `${index * 120}ms` }" />
              </span>
            </div>
          </div>

          <div ref="sentinel" class="h-px" />
        </template>
      </div>
    </div>

    <div v-if="selectedCount > 0" class="absolute bottom-4 left-1/2 z-30 flex -translate-x-1/2 items-center gap-1.5 rounded-xl border border-border/80 bg-popover/95 p-1.5 pl-2.5 text-popover-foreground shadow-xl backdrop-blur-xl" role="status" aria-live="polite">
      <span class="flex items-center gap-2 text-xs font-medium">
        <span class="grid size-4 place-items-center rounded-full bg-primary/10 text-primary"><Check :size="10" :stroke-width="3" /></span>
        {{ t("photos.selected", { count: selectedCount }) }}
      </span>
      <span class="mx-0.5 h-4 w-px bg-border" aria-hidden="true" />
      <Button size="sm" variant="outline" class="h-7 gap-1 rounded-md border-primary/20 bg-primary/5 px-3 text-xs font-medium text-primary shadow-none hover:bg-primary/10 hover:text-primary" @click="downloadSelectedPhotos">
        <Download :size="13" />
        {{ t("photos.downloadSelected") }}
      </Button>
      <button type="button" class="grid size-7 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="t('photos.clearSelection')" :title="t('photos.clearSelection')" @click="clearPhotoSelection">
        <X :size="14" />
      </button>
    </div>

    <div v-if="contextMenu" class="fixed inset-0 z-[2040]" @pointerdown="contextMenu = null" @contextmenu.prevent="contextMenu = null">
      <div class="fixed w-52 overflow-hidden rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-2xl" :style="{ left: `${contextMenu.x}px`, top: `${contextMenu.y}px` }" @pointerdown.stop @click.stop>
        <p class="truncate px-2.5 pt-1.5 pb-2 text-xs text-muted-foreground" :title="photoFileName(contextMenu.photo)">{{ photoFileName(contextMenu.photo) }}</p>
        <button type="button" class="flex h-8 w-full items-center gap-2 rounded-md px-2.5 text-left text-sm transition-colors hover:bg-accent hover:text-accent-foreground" @click="openPhotoDetails(contextMenu.photo)">
          <Info :size="14" />
          {{ t("photos.details") }}
        </button>
        <button type="button" class="flex h-8 w-full items-center gap-2 rounded-md px-2.5 text-left text-sm transition-colors hover:bg-accent hover:text-accent-foreground disabled:opacity-50" :disabled="downloadingPhotoId === contextMenu.photo.id" @click="downloadSinglePhoto(contextMenu.photo)">
          <Spinner v-if="downloadingPhotoId === contextMenu.photo.id" :size="13" />
          <Download v-else :size="14" />
          {{ t("photos.downloadOriginal") }}
        </button>
        <button type="button" class="flex h-8 w-full items-center gap-2 rounded-md px-2.5 text-left text-sm transition-colors hover:bg-accent hover:text-accent-foreground" @click="selectPhotoFromContext(contextMenu.photo)">
          <Check :size="14" />
          {{ t("photos.selectThis") }}
        </button>
      </div>
    </div>

    <div v-if="detailsPhoto" class="fixed inset-0 z-[2100] grid place-items-center bg-black/45 p-6 backdrop-blur-[2px]" @click.self="detailsPhoto = null">
      <div class="flex max-h-[86vh] w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-2xl" role="dialog" aria-modal="true" aria-labelledby="photo-details-title">
        <header class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
          <Info :size="16" class="text-primary" />
          <h2 id="photo-details-title" class="text-sm font-semibold">{{ t("photos.details") }}</h2>
          <button type="button" class="ml-auto grid size-8 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="t('common.close')" @click="detailsPhoto = null">
            <X :size="16" />
          </button>
        </header>

        <div class="grid min-h-0 flex-1 overflow-y-auto md:grid-cols-[minmax(0,1.15fr)_minmax(280px,.85fr)]">
          <div class="relative grid min-h-[280px] place-items-center overflow-hidden bg-muted/70 p-4 md:min-h-[420px]">
            <img v-if="detailsPhoto.displayUrl || detailsPhoto.thumbUrl" :src="detailsPhoto.displayUrl || detailsPhoto.thumbUrl" :alt="photoFileName(detailsPhoto)" class="max-h-[60vh] max-w-full rounded-lg object-contain shadow-sm" draggable="false" />
            <ImageOff v-else :size="30" class="text-muted-foreground" />
          </div>

          <div class="flex min-h-0 flex-col p-5">
            <div>
              <p class="text-xs font-medium text-primary">{{ photoType(detailsPhoto) }}</p>
              <h3 class="mt-1 break-all text-base font-semibold">{{ photoFileName(detailsPhoto) }}</h3>
            </div>

            <dl class="mt-5 divide-y divide-border text-sm">
              <div class="grid grid-cols-[84px_minmax(0,1fr)] gap-3 py-3">
                <dt class="text-muted-foreground">{{ t("photos.taken") }}</dt>
                <dd>{{ formatPhotoTime(detailsPhoto.takenAt) }}</dd>
              </div>
              <div class="grid grid-cols-[84px_minmax(0,1fr)] gap-3 py-3">
                <dt class="text-muted-foreground">{{ t("photos.dimensions") }}</dt>
                <dd class="tabular">{{ photoDimensions(detailsPhoto) }}</dd>
              </div>
              <div class="grid grid-cols-[84px_minmax(0,1fr)] gap-3 py-3">
                <dt class="text-muted-foreground">{{ t("photos.type") }}</dt>
                <dd>{{ photoType(detailsPhoto) }}</dd>
              </div>
              <div class="grid grid-cols-[84px_minmax(0,1fr)] gap-3 py-3">
                <dt class="text-muted-foreground">{{ t("photos.original") }}</dt>
                <dd>{{ t(detailsPhoto.originalUrl ? "common.available" : "common.unavailable") }}</dd>
              </div>
            </dl>

            <div class="mt-auto flex items-center justify-end gap-2 pt-6">
              <Button variant="outline" size="sm" class="h-8" @click="openViewerForPhoto(detailsPhoto)">{{ t("photos.viewFullSize") }}</Button>
              <Button size="sm" class="h-8 gap-1.5" :disabled="downloadingPhotoId === detailsPhoto.id" @click="downloadSinglePhoto(detailsPhoto)">
                <Spinner v-if="downloadingPhotoId === detailsPhoto.id" :size="13" />
                <Download v-else :size="14" />
                {{ t("photos.downloadOriginal") }}
              </Button>
            </div>
          </div>
        </div>
      </div>
    </div>

    <PhotoViewer
      v-if="viewerIndex !== null"
      :photos="viewerList"
      :index="viewerIndex"
      :base-rank="viewerBaseRank"
      :total-count="viewerTotalCount"
      :session-id="sessionId"
      :has-more="hasMore"
      @update:index="viewerIndex = $event"
      @need-more="continueLoading"
      @seek-rank="seekViewerRank"
      @media-error="refreshLoadedPhotoUrls"
      @close="closeViewer"
    />
  </div>
</template>
