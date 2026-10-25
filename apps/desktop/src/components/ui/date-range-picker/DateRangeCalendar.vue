<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { ChevronLeft, ChevronRight } from "@lucide/vue";

import { addDays, startOfDay, type DateBounds } from "@/lib/photos/dates";
import { useLocaleStore } from "@/stores/locale";

const props = withDefaults(
  defineProps<{
    modelValue: DateBounds;
    showClear?: boolean;
  }>(),
  { showClear: true },
);

const emit = defineEmits<{
  "update:modelValue": [value: DateBounds];
  cancel: [];
}>();

const locale = useLocaleStore();
const t = locale.t;
const CURRENT_YEAR = new Date().getFullYear();
const YEAR_OPTIONS = Array.from({ length: CURRENT_YEAR - 1990 + 1 }, (_, index) => CURRENT_YEAR - index);
const MONTH_OPTIONS = Array.from({ length: 12 }, (_, index) => index + 1);
const today = startOfDay(new Date());

const startValue = ref<number | null>(null);
const endValue = ref<number | null>(null);
const viewMonth = ref(startOfDay(new Date()));

function monthStartOf(ms: number): number {
  const date = new Date(ms);
  return new Date(date.getFullYear(), date.getMonth(), 1).getTime();
}

function beginDraft(): void {
  startValue.value = props.modelValue.after === null ? null : startOfDay(new Date(props.modelValue.after));
  endValue.value = props.modelValue.before === null ? null : startOfDay(new Date(addDays(props.modelValue.before, -1)));
  viewMonth.value = monthStartOf(startValue.value ?? today);
}

watch(
  () => props.modelValue,
  () => beginDraft(),
  { immediate: true, deep: true },
);

const monthStart = computed(() => monthStartOf(viewMonth.value));
const selectedYear = computed(() => new Date(monthStart.value).getFullYear());
const selectedMonth = computed(() => new Date(monthStart.value).getMonth() + 1);
const canApply = computed(() => startValue.value !== null && endValue.value !== null);
const weekdays = computed(() => {
  const formatter = new Intl.DateTimeFormat(locale.locale, { weekday: "narrow" });
  const monday = new Date(2024, 0, 1);
  return Array.from({ length: 7 }, (_, index) => formatter.format(new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + index)));
});
const monthLabels = computed(() => MONTH_OPTIONS.map((month) => ({ value: month, label: new Date(2020, month - 1, 1).toLocaleDateString(locale.locale, { month: "short" }) })));

interface CalendarCell {
  time: number;
  day: number;
  inMonth: boolean;
  isToday: boolean;
  isStart: boolean;
  isEnd: boolean;
  inRange: boolean;
}

const cells = computed<CalendarCell[]>(() => {
  const first = new Date(monthStart.value);
  const offset = (first.getDay() + 6) % 7;
  const gridStart = addDays(monthStart.value, -offset);
  return Array.from({ length: 42 }, (_, index) => {
    const time = addDays(gridStart, index);
    const date = new Date(time);
    return {
      time,
      day: date.getDate(),
      inMonth: date.getMonth() === first.getMonth(),
      isToday: time === today,
      isStart: time === startValue.value,
      isEnd: time === endValue.value,
      inRange: startValue.value !== null && endValue.value !== null && time > startValue.value && time < endValue.value,
    };
  });
});

function moveMonth(delta: number): void {
  const date = new Date(monthStart.value);
  date.setMonth(date.getMonth() + delta);
  viewMonth.value = monthStartOf(date.getTime());
}

function onYearChange(event: Event): void {
  const year = Number((event.target as HTMLSelectElement).value);
  if (!Number.isInteger(year)) return;
  const date = new Date(monthStart.value);
  date.setFullYear(year);
  viewMonth.value = monthStartOf(date.getTime());
}

function onMonthChange(event: Event): void {
  const month = Number((event.target as HTMLSelectElement).value);
  if (!Number.isInteger(month) || month < 1 || month > 12) return;
  const date = new Date(monthStart.value);
  date.setMonth(month - 1);
  viewMonth.value = monthStartOf(date.getTime());
}

function selectDay(time: number): void {
  if (startValue.value === null || endValue.value !== null) {
    startValue.value = time;
    endValue.value = null;
    return;
  }
  let after = startValue.value;
  let before = time;
  if (before < after) [after, before] = [before, after];
  startValue.value = after;
  endValue.value = before;
}

function dayClass(cell: CalendarCell): string {
  if (cell.isStart || cell.isEnd) return "bg-primary text-primary-foreground shadow-sm";
  if (cell.inRange) return "bg-primary/12 text-foreground";
  if (!cell.inMonth) return "text-muted-foreground/35 hover:bg-accent";
  if (cell.isToday) return "ring-1 ring-primary/50 hover:bg-accent";
  return "hover:bg-accent";
}

function apply(): void {
  if (startValue.value === null || endValue.value === null) return;
  emit("update:modelValue", { after: startValue.value, before: addDays(endValue.value, 1) });
}

function clear(): void {
  emit("update:modelValue", { after: null, before: null });
}
</script>

<template>
  <div class="w-full">
    <div class="mb-2 flex items-center gap-1.5">
      <button type="button" class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="t('photos.previousMonth')" @click="moveMonth(-1)">
        <ChevronLeft :size="15" />
      </button>
      <select :value="selectedYear" class="h-7 min-w-0 flex-1 rounded-md border border-input bg-card px-1.5 text-xs text-foreground outline-none transition-colors focus:border-primary" :aria-label="t('photos.year')" @change="onYearChange">
        <option v-for="year in YEAR_OPTIONS" :key="year" :value="year">{{ year }}</option>
      </select>
      <select :value="selectedMonth" class="h-7 w-[68px] shrink-0 rounded-md border border-input bg-card px-1.5 text-xs text-foreground outline-none transition-colors focus:border-primary" :aria-label="t('photos.month')" @change="onMonthChange">
        <option v-for="month in monthLabels" :key="month.value" :value="month.value">{{ month.label }}</option>
      </select>
      <button type="button" class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="t('photos.nextMonth')" @click="moveMonth(1)">
        <ChevronRight :size="15" />
      </button>
    </div>

    <div class="grid grid-cols-7">
      <span v-for="(day, index) in weekdays" :key="index" class="grid h-7 place-items-center text-2xs font-medium text-muted-foreground">{{ day }}</span>
    </div>
    <div class="grid grid-cols-7 gap-y-0.5">
      <button v-for="cell in cells" :key="cell.time" type="button" class="mx-auto grid size-8 place-items-center rounded-md text-xs transition-colors" :class="dayClass(cell)" @click="selectDay(cell.time)">
        {{ cell.day }}
      </button>
    </div>

    <div class="mt-2 flex items-center gap-1 border-t border-border pt-2">
      <button v-if="showClear" type="button" class="rounded-md px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" @click="clear">{{ t("common.clear") }}</button>
      <button type="button" class="ml-auto rounded-md border border-border px-3 py-1 text-xs text-foreground transition-colors hover:bg-accent" @click="emit('cancel')">{{ t("common.cancel") }}</button>
      <button type="button" class="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground transition-opacity disabled:cursor-not-allowed disabled:opacity-45" :disabled="!canApply" @click="apply">{{ t("common.apply") }}</button>
    </div>
  </div>
</template>
