<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { AlertTriangle, Laptop, RefreshCw } from "@lucide/vue";

import StorageCategoryIcon from "@/components/storage/StorageCategoryIcon.vue";
import Spinner from "@/components/ui/Spinner.vue";
import { Button } from "@/components/ui/button";
import { openExternal, storageUsage } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { useLocaleStore } from "@/stores/locale";
import type { StorageUsageCategory, StorageUsageResult } from "@/types";

const props = defineProps<{ sessionId: string }>();

const locale = useLocaleStore();
const t = locale.t;
const result = ref<StorageUsageResult | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
let loadEpoch = 0;

const usedBytes = computed(() => Math.max(0, result.value?.usedStorageInBytes ?? 0));
const totalBytes = computed(() => Math.max(0, result.value?.totalStorageInBytes ?? 0));
const availableBytes = computed(() => Math.max(0, totalBytes.value - usedBytes.value));
const status = computed<"over" | "almost" | "healthy">(() => {
  if (result.value?.quotaStatus.overQuota) return "over";
  if (result.value?.quotaStatus.almostFull) return "almost";
  return "healthy";
});
const alertTitle = computed(() => t(status.value === "over" ? "storage.quotaFullTitle" : "storage.almostFull"));
const alertIconTone = computed(() => (status.value === "over" ? "border-destructive text-destructive" : "border-warning text-warning"));

const categories = computed(() => [...(result.value?.storageUsageByMedia ?? [])].sort((left, right) => right.usageInBytes - left.usageInBytes));
const categoryTotal = computed(() => categories.value.reduce((total, category) => total + Math.max(0, category.usageInBytes), 0));
const categoryCount = computed(() => t("storage.categoryCount", { count: categories.value.length }));
const segments = computed(() =>
  categories.value
    .map((category, index) => ({
      key: `${category.mediaKey}-${index}`,
      color: categoryColor(category, index),
      width: categoryTotal.value > 0 ? (Math.max(0, category.usageInBytes) / categoryTotal.value) * 100 : 0,
    }))
    .filter((segment) => segment.width > 0),
);

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const formatted = new Intl.NumberFormat(locale.formatLocale, {
    maximumFractionDigits: value >= 10 ? 1 : 2,
    minimumFractionDigits: 0,
  }).format(value);
  return `${formatted} ${units[unit]}`;
}

function categoryColor(category: StorageUsageCategory, index: number): string {
  const raw = category.displayColor.trim().replace(/^#/, "");
  if (/^[0-9a-f]{6}$/i.test(raw)) return `#${raw}`;
  const fallback = ["#0a84ff", "#ff9f0a", "#30d158", "#bf5af2", "#ff375f", "#64d2ff"];
  return fallback[index % fallback.length] ?? "#0a84ff";
}

function categoryLabel(category: StorageUsageCategory): string {
  const key = `storage.category.${category.mediaKey}`;
  const translated = t(key);
  return translated === key ? category.displayLabel || category.mediaKey : translated;
}

function categoryDescription(category: StorageUsageCategory): string {
  const key = `storage.category.${category.mediaKey}Description`;
  const translated = t(key);
  return translated === key ? "" : translated;
}

async function load(): Promise<void> {
  if (loading.value) return;
  const epoch = ++loadEpoch;
  loading.value = true;
  error.value = null;
  try {
    const next = await storageUsage(props.sessionId);
    if (epoch !== loadEpoch) return;
    if (next.status !== "OK") throw new Error(next.message || t("storage.loadFailed"));
    result.value = next;
  } catch (cause) {
    if (epoch !== loadEpoch) return;
    error.value = cause instanceof Error ? cause.message : t("storage.loadFailed");
  } finally {
    if (epoch === loadEpoch) loading.value = false;
  }
}

async function manageStorage(): Promise<void> {
  if (!result.value?.manageUrl) return;
  try {
    await openExternal(result.value.manageUrl);
  } catch (cause) {
    toast.error(cause instanceof Error ? cause.message : t("storage.loadFailed"));
  }
}

onMounted(() => void load());
</script>

<template>
  <div class="h-full overflow-y-auto bg-background">
    <div class="mx-auto w-full max-w-[1180px] px-8 py-8 pb-12">
      <div v-if="loading && !result" class="grid min-h-[420px] place-items-center">
        <div class="flex flex-col items-center gap-3 text-sm text-muted-foreground">
          <Spinner :size="22" />
          <span>{{ t("storage.loading") }}</span>
        </div>
      </div>

      <div v-else-if="error && !result" class="grid min-h-[420px] place-items-center">
        <div class="max-w-md text-center">
          <span class="mx-auto grid size-11 place-items-center rounded-full bg-destructive/10 text-destructive">
            <AlertTriangle :size="20" />
          </span>
          <h2 class="mt-4 text-base font-semibold">{{ t("storage.loadFailed") }}</h2>
          <p class="mt-1.5 text-sm leading-6 text-muted-foreground">{{ error }}</p>
          <Button class="mt-4" variant="outline" @click="load">
            <RefreshCw :size="14" />
            {{ t("storage.retry") }}
          </Button>
        </div>
      </div>

      <div v-else-if="result">
        <div v-if="error" class="mb-6 flex items-center gap-3 rounded-xl border border-destructive/25 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          <AlertTriangle :size="16" class="shrink-0" />
          <span class="min-w-0 flex-1">{{ error }}</span>
          <button type="button" class="shrink-0 text-xs font-medium underline-offset-4 hover:underline" @click="load">{{ t("storage.retry") }}</button>
        </div>

        <header class="max-w-[920px]">
          <h1 class="text-[40px] font-semibold leading-[1.15] tracking-[-0.035em] sm:text-[48px]">{{ t("storage.pageTitle") }}</h1>
          <p class="mt-4 max-w-[900px] text-base leading-7 text-muted-foreground">{{ t("storage.pageDescription") }}</p>
        </header>

        <section class="mt-9 max-w-[920px]">
          <div class="flex flex-wrap items-end justify-between gap-4">
            <div class="relative inline-flex">
              <span class="rounded-[14px] bg-foreground px-4 py-2.5 text-background shadow-sm">
                <span class="tabular text-2xl font-semibold tracking-tight" data-testid="storage-total">{{ formatBytes(totalBytes) }}</span>
              </span>
              <span v-if="status !== 'healthy'" class="absolute -top-2.5 -right-2.5 grid size-6 place-items-center rounded-full bg-warning text-white shadow-sm" aria-hidden="true">
                <span class="text-sm font-bold leading-none">!</span>
              </span>
            </div>

            <p class="text-base text-muted-foreground">
              {{ t("common.available") }} <span class="tabular text-foreground">{{ formatBytes(availableBytes) }}</span>
              <span class="mx-2 text-border">•</span>
              {{ t("storage.used") }} <span class="tabular text-foreground">{{ formatBytes(usedBytes) }}</span>
            </p>
          </div>

          <div class="mt-4 flex h-3 gap-0.5 overflow-hidden rounded-full bg-secondary" role="progressbar" :aria-label="t('nav.storage')">
            <span v-for="segment in segments" :key="segment.key" class="h-full first:rounded-l-full last:rounded-r-full" :style="{ width: `${segment.width}%`, backgroundColor: segment.color }" />
          </div>

          <div v-if="status !== 'healthy'" class="mt-6 flex gap-4 rounded-2xl bg-secondary/60 p-5">
            <span class="mt-0.5 grid size-8 shrink-0 place-items-center rounded-full border-2" :class="alertIconTone">
              <span class="text-base font-semibold leading-none">!</span>
            </span>
            <div class="min-w-0">
              <h2 class="text-lg font-semibold">{{ alertTitle }}</h2>
              <p class="mt-1.5 text-sm leading-6 text-muted-foreground">{{ t("storage.quotaFullDescription") }}</p>
              <button v-if="result.manageUrl" type="button" class="mt-3 text-sm font-medium text-primary underline-offset-4 hover:underline" @click="manageStorage">{{ t("storage.manage") }}</button>
            </div>
          </div>
        </section>

        <div class="mt-10 grid gap-8 lg:grid-cols-[minmax(0,1fr)_320px] lg:items-start">
          <section class="min-w-0">
            <div class="flex items-end justify-between gap-4">
              <h2 class="text-lg font-semibold tracking-tight">{{ t("storage.usageByCategory") }}</h2>
              <div class="flex items-center gap-2 text-xs text-muted-foreground">
                <span>{{ categoryCount }}</span>
                <Button variant="ghost" size="icon" class="-mr-2 size-7" :disabled="loading" :aria-label="t('storage.refresh')" :title="t('storage.refresh')" @click="load">
                  <RefreshCw :size="13" :class="loading ? 'animate-spin' : ''" />
                </Button>
              </div>
            </div>

            <div class="mt-5">
              <div v-for="(category, index) in categories" :key="`${category.mediaKey}-${index}`" class="flex items-center gap-4 py-4" data-testid="storage-category">
                <StorageCategoryIcon :media-key="category.mediaKey" />
                <div class="min-w-0 flex-1 sm:grid sm:grid-cols-[150px_minmax(0,1fr)_auto] sm:items-center sm:gap-5">
                  <span class="truncate text-[17px] font-medium">{{ categoryLabel(category) }}</span>
                  <span class="mt-0.5 block truncate text-sm text-muted-foreground sm:mt-0">{{ categoryDescription(category) }}</span>
                  <div class="mt-2 flex items-center gap-3 sm:mt-0 sm:justify-end">
                    <span class="tabular text-[17px] font-medium">{{ formatBytes(category.usageInBytes) }}</span>
                    <span class="size-2.5 shrink-0 rounded-full" :style="{ backgroundColor: categoryColor(category, index) }" />
                  </div>
                </div>
              </div>
            </div>

            <p v-if="categories.length === 0" class="py-10 text-center text-sm text-muted-foreground">{{ t("storage.noUsage") }}</p>
          </section>

          <aside>
            <div class="rounded-2xl bg-secondary/60 p-6">
              <h2 class="text-base font-semibold">{{ t("storage.helpTitle") }}</h2>
              <p class="mt-3 text-sm leading-6 text-muted-foreground">{{ t("storage.helpDescription") }}</p>
              <button v-if="result.manageUrl" type="button" class="mt-5 inline-flex items-center gap-2 text-sm font-medium text-primary underline-offset-4 hover:underline" @click="manageStorage">
                <Laptop :size="16" />
                {{ t("storage.manage") }}
              </button>
            </div>
          </aside>
        </div>
      </div>
    </div>
  </div>
</template>
