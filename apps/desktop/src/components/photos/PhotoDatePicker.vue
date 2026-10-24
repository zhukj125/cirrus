<script setup lang="ts">
import { Check, ChevronLeft, ChevronRight } from "@lucide/vue";
import { computed, ref, watch } from "vue";

import DateRangeCalendar from "@/components/ui/date-range-picker/DateRangeCalendar.vue";
import { RANGE_OPTIONS, type DateBounds, type RangeKey } from "@/lib/photos/dates";
import type { TimelineSelection, TimelineYearView } from "@/lib/photos/timeline";

const props = defineProps<{
  activeRange: RangeKey;

  customValue: DateBounds;

  customLabel?: string | null;

  years: TimelineYearView[];

  selected: TimelineSelection | null;

  loading: boolean;

  exactCounts?: Record<string, number>;

  pendingCounts?: Record<string, boolean>;
}>();

const emit = defineEmits<{
  selectRange: [RangeKey];
  selectCustomRange: [DateBounds];
  selectTimeline: [TimelineSelection];
  ensureYear: [number];
}>();

const mode = ref<"browse" | "custom">(props.activeRange === "custom" && !props.selected ? "custom" : "browse");
const activeYear = ref<number | null>(null);
const quickOptions = computed(() => RANGE_OPTIONS.filter((option) => option.key !== "custom"));
const activeIndex = computed(() => props.years.findIndex((year) => year.year === activeYear.value));
const activeEntry = computed(() => props.years[activeIndex.value] ?? props.years[0] ?? null);
const activeYearKey = computed(() => (activeEntry.value ? String(activeEntry.value.year) : ""));
const selectedKey = computed(() => {
  const selected = props.selected;
  if (!selected) return null;
  return selected.month === null ? String(selected.year) : `${selected.year}-${String(selected.month).padStart(2, "0")}`;
});

const months = computed(() => {
  const entry = activeEntry.value;
  if (!entry) return [];
  return Array.from({ length: 12 }, (_, index) => {
    const month = index + 1;
    return {
      key: `${entry.year}-${String(month).padStart(2, "0")}`,
      label: new Date(2020, month - 1, 1).toLocaleDateString("en-US", { month: "short" }),
      year: entry.year,
      month,
      fullLabel: `${new Date(entry.year, month - 1, 1).toLocaleDateString("en-US", { month: "short" })} ${entry.year}`,
    };
  });
});

watch(
  [() => props.years, () => props.selected],
  () => {
    const preferred = props.selected?.year ?? activeYear.value;
    const next = props.years.some((year) => year.year === preferred) ? preferred : (props.years[0]?.year ?? null);
    if (next !== null && next !== activeYear.value) activeYear.value = next;
  },
  { immediate: true },
);

watch(
  activeYear,
  (year) => {
    if (year !== null) emit("ensureYear", year);
  },
  { immediate: true },
);

watch(
  () => [props.activeRange, props.selected] as const,
  ([activeRange, selected]) => {
    if (activeRange === "custom" && !selected) mode.value = "custom";
    else if (activeRange !== "custom") mode.value = "browse";
  },
);

function exactOf(key: string): number | null {
  const value = props.exactCounts?.[key];
  return typeof value === "number" ? value : null;
}

function pendingOf(key: string): boolean {
  return props.pendingCounts?.[key] === true;
}

function countOf(key: string): string {
  const exact = exactOf(key);
  if (exact !== null) return `${exact} photos`;
  return pendingOf(key) ? "…" : "—";
}

function countTitle(key: string): string {
  if (exactOf(key) !== null) return "Exact count";
  return pendingOf(key) ? "Calculating exact count…" : "Exact count unavailable";
}

function stepYear(delta: number): void {
  const next = props.years[activeIndex.value + delta];
  if (next) activeYear.value = next.year;
}

function applyCustom(value: DateBounds): void {
  emit("selectCustomRange", value);
}
</script>

<template>
  <section class="w-[352px] overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-2xl">
    <header class="border-b border-border p-2">
      <div class="grid grid-cols-2 gap-0.5 rounded-md bg-muted p-0.5">
        <button type="button" class="h-7 rounded-[5px] text-xs transition-colors" :class="mode === 'browse' ? 'bg-card font-medium text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'" @click="mode = 'browse'">Quick & Month</button>
        <button type="button" class="h-7 rounded-[5px] text-xs transition-colors" :class="mode === 'custom' ? 'bg-card font-medium text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'" @click="mode = 'custom'">Custom date</button>
      </div>
    </header>

    <template v-if="mode === 'browse'">
      <div class="p-3">
        <p class="mb-2 text-2xs font-medium tracking-wide text-muted-foreground">Quick select</p>
        <div class="grid grid-cols-3 gap-1.5">
          <button
            v-for="option in quickOptions"
            :key="option.key"
            type="button"
            class="flex h-8 items-center justify-center gap-1 rounded-md border text-xs transition-colors"
            :class="activeRange === option.key ? 'border-primary/35 bg-primary/10 font-medium text-primary' : 'border-border bg-background text-muted-foreground hover:bg-accent hover:text-foreground'"
            @click="emit('selectRange', option.key)"
          >
            <Check v-if="activeRange === option.key" :size="12" />
            {{ option.label }}
          </button>
        </div>
      </div>

      <div class="border-t border-border px-3 pt-3 pb-3">
        <div class="mb-2 flex items-center justify-between">
          <p class="text-2xs font-medium tracking-wide text-muted-foreground">By year and month</p>
          <p v-if="activeEntry" class="tabular text-2xs text-muted-foreground" :title="countTitle(activeYearKey)">Full year {{ countOf(activeYearKey) }}</p>
        </div>

        <template v-if="activeEntry">
          <div class="flex items-center gap-1.5">
            <button
              type="button"
              class="grid size-7 shrink-0 place-items-center rounded-md border border-border bg-background text-muted-foreground transition-colors enabled:hover:bg-accent enabled:hover:text-foreground disabled:opacity-35"
              :disabled="activeIndex <= 0"
              title="Previous year"
              aria-label="Previous year"
              @click="stepYear(-1)"
            >
              <ChevronLeft :size="14" />
            </button>
            <select v-model="activeYear" class="h-7 min-w-0 flex-1 rounded-md border border-border bg-background px-2 text-center text-xs font-medium text-foreground outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring/20" aria-label="Select year">
              <option v-for="year in years" :key="year.key" :value="year.year">{{ year.label }}</option>
            </select>
            <button
              type="button"
              class="grid size-7 shrink-0 place-items-center rounded-md border border-border bg-background text-muted-foreground transition-colors enabled:hover:bg-accent enabled:hover:text-foreground disabled:opacity-35"
              :disabled="activeIndex < 0 || activeIndex >= years.length - 1"
              title="Next year"
              aria-label="Next year"
              @click="stepYear(1)"
            >
              <ChevronRight :size="14" />
            </button>
          </div>

          <button
            type="button"
            class="mt-2 flex h-8 w-full items-center rounded-md border px-2.5 text-xs transition-colors"
            :class="selected?.year === activeEntry.year && selected.month === null ? 'border-primary/35 bg-primary/10 text-primary' : 'border-border bg-background text-muted-foreground hover:bg-accent hover:text-foreground'"
            @click="emit('selectTimeline', { year: activeEntry.year, month: null })"
          >
            <span class="font-medium text-foreground">Full year</span>
            <span class="tabular ml-auto" :title="countTitle(activeYearKey)">{{ countOf(activeYearKey) }}</span>
          </button>

          <div class="mt-1.5 grid grid-cols-4 gap-1.5">
            <button
              v-for="month in months"
              :key="month.key"
              type="button"
              class="flex h-11 flex-col items-center justify-center rounded-md border text-xs transition-colors"
              :class="selectedKey === month.key ? 'border-primary/35 bg-primary/10 text-primary' : 'border-border bg-background text-muted-foreground hover:bg-accent hover:text-foreground'"
              :title="month.fullLabel"
              :aria-pressed="selectedKey === month.key"
              @click="emit('selectTimeline', { year: month.year, month: month.month })"
            >
              <span class="font-medium">{{ month.label }}</span>
              <span class="tabular mt-0.5 text-2xs" :title="countTitle(month.key)">{{ countOf(month.key) }}</span>
            </button>
          </div>
        </template>

        <p v-else class="py-5 text-center text-xs text-muted-foreground">{{ loading ? "Loading year and month data…" : "Year and month navigation is unavailable" }}</p>
      </div>
    </template>

    <div v-else class="p-3">
      <div class="mb-2 flex items-center justify-between gap-3">
        <div class="min-w-0">
          <p class="text-2xs font-medium tracking-wide text-muted-foreground">Custom date</p>
          <p v-if="customLabel" class="tabular mt-0.5 truncate text-xs text-foreground" :title="customLabel">{{ customLabel }}</p>
        </div>
        <button type="button" class="shrink-0 rounded-md px-1.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" @click="mode = 'browse'">Back</button>
      </div>
      <DateRangeCalendar :model-value="customValue" @update:model-value="applyCustom" @cancel="mode = 'browse'" />
    </div>
  </section>
</template>
