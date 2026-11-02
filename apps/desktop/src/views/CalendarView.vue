<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { CalendarDays, ChevronLeft, ChevronRight, Clock3, FileText, MapPin, Pencil, Plus, RefreshCw, Save, Trash2, X } from "@lucide/vue";

import { Status } from "@/constants";
import { calendarDeleteEvent, calendarLoad, calendarSaveEvent } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { isSessionGoneMessage } from "@/lib/session-errors";
import { useLocaleStore } from "@/stores/locale";
import Spinner from "@/components/ui/Spinner.vue";
import type { CalendarCollection, CalendarEvent, CalendarEventInput } from "@/types";

type CalendarMode = "day" | "week" | "month";

const props = defineProps<{ sessionId: string }>();
const emit = defineEmits<{ logout: [] }>();
const locale = useLocaleStore();
const t = locale.t;

const mode = ref<CalendarMode>("month");
const anchorDate = ref(startOfDay(new Date()));
const calendars = ref<CalendarCollection[]>([]);
const events = ref<CalendarEvent[]>([]);
const visibleCalendarIds = ref(new Set<string>());
const selectedEventId = ref<string | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
const editorOpen = ref(false);
const editorError = ref<string | null>(null);
const savingEvent = ref(false);
const deletingEvent = ref(false);
const draft = ref<CalendarEventInput>(emptyDraft());
let loadEpoch = 0;

const HOURS = Array.from({ length: 24 }, (_, hour) => hour);

function startOfDay(value: Date): Date {
  return new Date(value.getFullYear(), value.getMonth(), value.getDate(), 12, 0, 0, 0);
}

function addDays(value: Date, amount: number): Date {
  return new Date(value.getFullYear(), value.getMonth(), value.getDate() + amount, 12, 0, 0, 0);
}

function addMonths(value: Date, amount: number): Date {
  return new Date(value.getFullYear(), value.getMonth() + amount, 1, 12, 0, 0, 0);
}

function startOfWeek(value: Date): Date {
  return addDays(value, -value.getDay());
}

function dateKey(value: Date): string {
  const month = String(value.getMonth() + 1).padStart(2, "0");
  const day = String(value.getDate()).padStart(2, "0");
  return `${value.getFullYear()}-${month}-${day}`;
}

function sameDay(left: Date, right: Date): boolean {
  return dateKey(left) === dateKey(right);
}

const range = computed(() => {
  if (mode.value === "day") return { start: startOfDay(anchorDate.value), end: startOfDay(anchorDate.value) };
  if (mode.value === "week") {
    const start = startOfWeek(anchorDate.value);
    return { start, end: addDays(start, 6) };
  }
  const first = new Date(anchorDate.value.getFullYear(), anchorDate.value.getMonth(), 1, 12);
  const last = new Date(anchorDate.value.getFullYear(), anchorDate.value.getMonth() + 1, 0, 12);
  return { start: startOfWeek(first), end: addDays(startOfWeek(last), 6) };
});

const monthDays = computed(() => {
  const days: Date[] = [];
  for (let cursor = range.value.start; cursor <= range.value.end; cursor = addDays(cursor, 1)) days.push(cursor);
  return days;
});

const miniMonthDays = computed(() => {
  const first = new Date(anchorDate.value.getFullYear(), anchorDate.value.getMonth(), 1, 12);
  const last = new Date(anchorDate.value.getFullYear(), anchorDate.value.getMonth() + 1, 0, 12);
  const start = startOfWeek(first);
  const end = addDays(startOfWeek(last), 6);
  const days: Date[] = [];
  for (let cursor = start; cursor <= end; cursor = addDays(cursor, 1)) days.push(cursor);
  return days;
});
const miniWeekdayLabels = computed(() => Array.from({ length: 7 }, (_, index) => new Date(2024, 0, 7 + index, 12).toLocaleDateString(locale.locale, { weekday: "narrow" })));

const monthWeeks = computed(() => {
  const weeks: Date[][] = [];
  for (let index = 0; index < monthDays.value.length; index += 7) weeks.push(monthDays.value.slice(index, index + 7));
  return weeks;
});

const weekDays = computed(() => {
  const start = startOfWeek(anchorDate.value);
  return Array.from({ length: 7 }, (_, index) => addDays(start, index));
});

const timeDays = computed(() => (mode.value === "day" ? [startOfDay(anchorDate.value)] : weekDays.value));
const weekdayLabels = computed(() => weekDays.value.map((day) => day.toLocaleDateString(locale.locale, { weekday: "short" })));
const today = startOfDay(new Date());

const visibleEvents = computed(() => events.value.filter((event) => visibleCalendarIds.value.has(event.calendarId)));
const selectedEvent = computed(() => visibleEvents.value.find((event) => event.id === selectedEventId.value) ?? null);
const longRunningEvents = computed(() => visibleEvents.value.filter((event) => isLongRunning(event) && eventOverlapsRange(event)));

const title = computed(() => {
  if (mode.value === "month") return anchorDate.value.toLocaleDateString(locale.locale, { year: "numeric", month: "long" });
  if (mode.value === "week") {
    const start = range.value.start;
    const end = range.value.end;
    return `${start.toLocaleDateString(locale.locale, { month: "short", day: "numeric" })} – ${end.toLocaleDateString(locale.locale, { year: "numeric", month: "short", day: "numeric" })}`;
  }
  return anchorDate.value.toLocaleDateString(locale.locale, { year: "numeric", month: "long", day: "numeric", weekday: "long" });
});

function eventsForDay(day: Date): CalendarEvent[] {
  return visibleEvents.value.filter((event) => eventOccursOn(event, day)).sort((left, right) => left.start.localeCompare(right.start));
}

function isLongRunning(event: CalendarEvent): boolean {
  const start = new Date(event.start);
  const end = new Date(event.end);
  return !Number.isNaN(start.getTime()) && !Number.isNaN(end.getTime()) && end.getTime() - start.getTime() > 31 * 24 * 60 * 60 * 1000;
}

function eventOverlapsRange(event: CalendarEvent): boolean {
  const start = new Date(event.start);
  const end = new Date(event.end);
  return start <= range.value.end && end >= range.value.start;
}

function longEventsForWeek(week: Date[]): CalendarEvent[] {
  const first = week[0];
  const last = week[week.length - 1];
  if (!first || !last) return [];
  return longRunningEvents.value.filter((event) => new Date(event.start) <= last && new Date(event.end) >= first);
}
function eventOccursOn(event: CalendarEvent, day: Date): boolean {
  const start = new Date(event.start);
  const end = new Date(event.end);
  if (Number.isNaN(start.getTime())) return false;
  if (sameDay(start, day)) return true;
  if (Number.isNaN(end.getTime())) return false;
  const duration = end.getTime() - start.getTime();
  if (duration <= 0 || duration > 31 * 24 * 60 * 60 * 1000) return false;
  return day >= startOfDay(start) && day <= startOfDay(end);
}

function eventsAtHour(day: Date, hour: number): CalendarEvent[] {
  return eventsForDay(day).filter((event) => {
    const start = new Date(event.start);
    return !event.allDay && start.getHours() === hour && sameDay(start, day);
  });
}

function allDayEvents(day: Date): CalendarEvent[] {
  return eventsForDay(day).filter((event) => event.allDay);
}

function calendarForEvent(event: CalendarEvent): CalendarCollection | undefined {
  return calendars.value.find((calendar) => calendar.id === event.calendarId);
}

function eventColor(event: CalendarEvent): string {
  const color = calendarForEvent(event)?.color ?? "#1a73e8";
  return /^#[0-9a-f]{6}$/i.test(color) ? color : "#1a73e8";
}

function eventStyle(event: CalendarEvent): Record<string, string> {
  const color = eventColor(event);
  return {
    borderColor: color,
    backgroundColor: `color-mix(in srgb, ${color} 16%, transparent)`,
  };
}

function dotStyle(calendar: CalendarCollection): Record<string, string> {
  return { backgroundColor: /^#[0-9a-f]{6}$/i.test(calendar.color) ? calendar.color : "#1a73e8" };
}

function eventTime(event: CalendarEvent): string {
  if (event.allDay) return t("calendar.allDay");
  return new Date(event.start).toLocaleTimeString(locale.locale, { hour: "2-digit", minute: "2-digit" });
}

function selectedEventTime(event: CalendarEvent): string {
  const start = new Date(event.start);
  const end = new Date(event.end);
  if (event.allDay) {
    return start.toLocaleDateString(locale.locale, { year: "numeric", month: "long", day: "numeric" });
  }
  const date = start.toLocaleDateString(locale.locale, { year: "numeric", month: "long", day: "numeric", weekday: "long" });
  const times = `${start.toLocaleTimeString(locale.locale, { hour: "2-digit", minute: "2-digit" })} – ${end.toLocaleTimeString(locale.locale, { hour: "2-digit", minute: "2-digit" })}`;
  return `${date} · ${times}`;
}

function isToday(day: Date): boolean {
  return sameDay(day, today);
}

function isSelected(day: Date): boolean {
  return sameDay(day, anchorDate.value);
}

function isInMonth(day: Date): boolean {
  return day.getMonth() === anchorDate.value.getMonth();
}

function toggleCalendar(id: string): void {
  const next = new Set(visibleCalendarIds.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  visibleCalendarIds.value = next;
  if (selectedEvent.value && !next.has(selectedEvent.value.calendarId)) selectedEventId.value = null;
}

function shift(direction: number): void {
  if (mode.value === "day") anchorDate.value = addDays(anchorDate.value, direction);
  else if (mode.value === "week") anchorDate.value = addDays(anchorDate.value, direction * 7);
  else anchorDate.value = addMonths(anchorDate.value, direction);
}

const writableCalendars = computed(() => calendars.value.filter((calendar) => !calendar.readOnly));

function emptyDraft(): CalendarEventInput {
  return {
    eventId: null,
    calendarId: "",
    title: "",
    start: "",
    end: "",
    allDay: false,
    location: "",
    notes: "",
    url: "",
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai",
    timezoneOffset: "+08:00",
    locale: locale.locale,
  };
}

function timezoneOffsetFor(value: string): string {
  const date = new Date(value.length === 16 ? `${value}:00` : value);
  const minutes = Number.isFinite(date.getTime()) ? -date.getTimezoneOffset() : 480;
  const sign = minutes >= 0 ? "+" : "-";
  const absolute = Math.abs(minutes);
  return `${sign}${String(Math.floor(absolute / 60)).padStart(2, "0")}:${String(absolute % 60).padStart(2, "0")}`;
}

function datetimeInputValue(value: string): string {
  return value.slice(0, 16);
}

function datetimeValue(value: string): string {
  return value.length === 16 ? `${value}:00` : value;
}

function canModifyEvent(event: CalendarEvent): boolean {
  return !event.readOnly && !event.recurring && !(calendarForEvent(event)?.readOnly ?? false);
}

function openCreateEvent(): void {
  const calendar = writableCalendars.value.find((item) => visibleCalendarIds.value.has(item.id)) ?? writableCalendars.value[0];
  if (!calendar) {
    toast.error(t("calendar.noWritableCalendar"));
    return;
  }
  const day = dateKey(anchorDate.value);
  const start = `${day}T09:00`;
  const end = `${day}T10:00`;
  draft.value = {
    ...emptyDraft(),
    calendarId: calendar.id,
    start,
    end,
    timezoneOffset: timezoneOffsetFor(start),
  };
  selectedEventId.value = null;
  editorError.value = null;
  editorOpen.value = true;
}

function openEditEvent(): void {
  const event = selectedEvent.value;
  if (!event || !canModifyEvent(event)) {
    toast.error(t("calendar.eventReadOnly"));
    return;
  }
  draft.value = {
    eventId: event.id,
    calendarId: event.calendarId,
    title: event.title,
    start: datetimeInputValue(event.start),
    end: datetimeInputValue(event.end),
    allDay: event.allDay,
    location: event.location,
    notes: event.notes,
    url: event.url,
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai",
    timezoneOffset: timezoneOffsetFor(event.start),
    locale: locale.locale,
  };
  editorError.value = null;
  editorOpen.value = true;
}

function closeEditor(): void {
  if (savingEvent.value) return;
  editorOpen.value = false;
  editorError.value = null;
}

function onAllDayChange(): void {
  if (!draft.value.allDay) return;
  const day = draft.value.start.slice(0, 10);
  const next = addDays(new Date(`${day}T12:00:00`), 1);
  draft.value.start = `${day}T00:00`;
  draft.value.end = `${dateKey(next)}T00:00`;
}

async function saveCalendarEvent(): Promise<void> {
  if (savingEvent.value) return;
  if (!draft.value.calendarId) {
    editorError.value = t("calendar.selectCalendar");
    return;
  }
  if (!draft.value.title.trim()) {
    editorError.value = t("calendar.titleRequired");
    return;
  }
  const start = datetimeValue(draft.value.start);
  const end = datetimeValue(draft.value.end);
  if (!start || !end || new Date(end).getTime() < new Date(start).getTime()) {
    editorError.value = t("calendar.invalidTimeRange");
    return;
  }
  savingEvent.value = true;
  editorError.value = null;
  try {
    const result = await calendarSaveEvent({
      sessionId: props.sessionId,
      event: {
        ...draft.value,
        title: draft.value.title.trim(),
        start,
        end,
        timezone: draft.value.timezone || Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai",
        timezoneOffset: timezoneOffsetFor(start),
        locale: locale.locale,
      },
    });
    if (isSessionGoneMessage(result.message)) {
      emit("logout");
      return;
    }
    if (result.status === Status.error) {
      editorError.value = result.message ?? t("calendar.saveFailed");
      return;
    }
    const eventId = result.event?.id ?? draft.value.eventId;
    editorOpen.value = false;
    await loadCalendar();
    if (eventId) selectedEventId.value = eventId;
    toast.success(t("calendar.saved"));
  } catch (caught) {
    editorError.value = caught instanceof Error ? caught.message : t("calendar.saveFailed");
  } finally {
    savingEvent.value = false;
  }
}

async function deleteSelectedEvent(): Promise<void> {
  const event = selectedEvent.value;
  if (!event || deletingEvent.value) return;
  if (!canModifyEvent(event)) {
    toast.error(t("calendar.eventReadOnly"));
    return;
  }
  if (!window.confirm(t("calendar.deleteConfirm", { title: event.title }))) return;
  deletingEvent.value = true;
  try {
    const result = await calendarDeleteEvent({
      sessionId: props.sessionId,
      eventId: event.id,
      calendarId: event.calendarId,
      start: datetimeValue(event.start),
      end: datetimeValue(event.end),
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai",
      locale: locale.locale,
    });
    if (isSessionGoneMessage(result.message)) {
      emit("logout");
      return;
    }
    if (result.status === Status.error) {
      toast.error(result.message ?? t("calendar.deleteFailed"));
      return;
    }
    selectedEventId.value = null;
    await loadCalendar();
    toast.success(t("calendar.deleted"));
  } catch (caught) {
    toast.error(t("calendar.deleteFailedWithError", { error: caught instanceof Error ? caught.message : t("app.unknownError") }));
  } finally {
    deletingEvent.value = false;
  }
}
async function loadCalendar(): Promise<void> {
  if (loading.value) return;
  loading.value = true;
  error.value = null;
  loadEpoch += 1;
  const epoch = loadEpoch;
  const start = addDays(range.value.start, -1);
  const end = addDays(range.value.end, 1);
  try {
    const response = await calendarLoad({
      sessionId: props.sessionId,
      startDate: dateKey(start),
      endDate: dateKey(end),
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai",
      locale: locale.locale,
    });
    if (epoch !== loadEpoch) return;
    if (response.status === Status.error) {
      if (isSessionGoneMessage(response.message)) {
        emit("logout");
        return;
      }
      error.value = response.message ?? t("calendar.loadFailed");
      return;
    }
    calendars.value = response.calendars;
    events.value = response.events;
    if (visibleCalendarIds.value.size === 0) {
      visibleCalendarIds.value = new Set(response.calendars.filter((calendar) => calendar.enabled).map((calendar) => calendar.id));
    }
  } catch (caught) {
    if (epoch === loadEpoch) error.value = caught instanceof Error ? caught.message : t("calendar.loadFailed");
  } finally {
    if (epoch === loadEpoch) loading.value = false;
  }
}

function selectEvent(id: string): void {
  selectedEventId.value = id;
}

watch([mode, anchorDate], () => {
  selectedEventId.value = null;
  void loadCalendar();
});

watch(
  () => props.sessionId,
  () => void loadCalendar(),
);

onMounted(() => void loadCalendar());
</script>

<template>
  <div class="flex h-full min-h-0 bg-background">
    <aside class="flex w-56 shrink-0 flex-col border-r border-border bg-card/35">
      <div class="flex h-12 items-center gap-2 border-b border-border px-3">
        <CalendarDays :size="16" class="text-primary" />
        <span class="text-sm font-semibold">{{ t("calendar.calendars") }}</span>
        <button type="button" class="ml-auto grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-50" :disabled="loading" @click="loadCalendar">
          <RefreshCw :size="13" :class="loading ? 'animate-spin' : ''" />
        </button>
      </div>
      <div class="space-y-1 overflow-y-auto p-2">
        <button v-for="calendar in calendars" :key="calendar.id" type="button" class="flex w-full items-center gap-2.5 rounded-md px-2 py-2 text-left text-sm hover:bg-accent" @click="toggleCalendar(calendar.id)">
          <span class="grid size-4 shrink-0 place-items-center rounded border" :class="visibleCalendarIds.has(calendar.id) ? 'border-transparent text-white' : 'border-muted-foreground/40'" :style="visibleCalendarIds.has(calendar.id) ? dotStyle(calendar) : {}">
            <span v-if="visibleCalendarIds.has(calendar.id)" class="text-[10px] leading-none">✓</span>
          </span>
          <span class="min-w-0 flex-1 truncate">{{ calendar.title }}</span>
        </button>
      </div>
    </aside>

    <div class="flex min-w-0 flex-1 flex-col">
      <header class="flex h-14 shrink-0 items-center gap-3 border-b border-border px-4">
        <h1 class="min-w-0 flex-1 truncate text-lg font-semibold">{{ title }}</h1>
        <div class="flex rounded-lg bg-muted p-0.5 text-sm">
          <button
            v-for="option in [
              { value: 'day', label: t('calendar.day') },
              { value: 'week', label: t('calendar.week') },
              { value: 'month', label: t('calendar.month') },
            ]"
            :key="option.value"
            type="button"
            :data-testid="`calendar-mode-${option.value}`"
            class="rounded-md px-3 py-1.5 transition-colors"
            :class="mode === option.value ? 'bg-background font-medium shadow-sm' : 'text-muted-foreground hover:text-foreground'"
            @click="mode = option.value as CalendarMode"
          >
            {{ option.label }}
          </button>
        </div>
        <button type="button" class="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-50" :disabled="writableCalendars.length === 0" @click="openCreateEvent">
          <Plus :size="14" />
          {{ t("calendar.newEvent") }}
        </button>
        <div class="flex items-center gap-1">
          <button type="button" class="grid size-8 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" @click="shift(-1)"><ChevronLeft :size="16" /></button>
          <button type="button" class="h-8 rounded-md px-2.5 text-sm text-muted-foreground hover:bg-accent hover:text-foreground" @click="anchorDate = startOfDay(new Date())">{{ t("calendar.today") }}</button>
          <button type="button" class="grid size-8 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" @click="shift(1)"><ChevronRight :size="16" /></button>
        </div>
      </header>

      <div v-if="loading && events.length === 0" class="grid min-h-0 flex-1 place-items-center text-muted-foreground"><Spinner :size="22" /></div>
      <div v-else-if="error" class="grid min-h-0 flex-1 place-items-center px-6 text-center text-sm text-destructive">{{ error }}</div>
      <div v-else class="flex min-h-0 flex-1">
        <div class="min-w-0 flex-1 overflow-auto">
          <div v-if="mode === 'month'" class="min-w-[760px]">
            <div class="grid grid-cols-7 border-b border-border bg-card/35">
              <div v-for="label in weekdayLabels" :key="label" class="px-3 py-2 text-center text-xs text-muted-foreground">{{ label }}</div>
            </div>
            <div v-for="week in monthWeeks" :key="`week-${dateKey(week[0])}`" class="border-b border-border/70">
              <div v-if="longEventsForWeek(week).length" class="border-b border-border/60 px-2 py-1">
                <button v-for="event in longEventsForWeek(week)" :key="event.id" type="button" class="mb-0.5 block w-full truncate rounded border-l-2 px-2 py-1 text-left text-xs" :style="eventStyle(event)" @click="selectEvent(event.id)">{{ event.title }}</button>
              </div>
              <div class="grid grid-cols-7">
                <div v-for="day in week" :key="dateKey(day)" class="min-h-28 border-r border-border/70 p-2 last:border-r-0" :class="!isInMonth(day) ? 'bg-muted/20 text-muted-foreground' : ''">
                  <div class="mb-1 flex justify-end">
                    <span class="grid size-6 place-items-center rounded-full text-xs" :class="isToday(day) ? 'bg-destructive font-semibold text-white' : ''">{{ day.getDate() }}</span>
                  </div>
                  <button v-for="event in eventsForDay(day).slice(0, 3)" :key="event.id" type="button" class="mb-1 block w-full truncate rounded border-l-2 px-1.5 py-0.5 text-left text-xs" :style="eventStyle(event)" @click="selectEvent(event.id)">{{ event.title }}</button>
                  <p v-if="eventsForDay(day).length > 3" class="px-1 text-2xs text-muted-foreground">+{{ eventsForDay(day).length - 3 }}</p>
                </div>
              </div>
            </div>
          </div>

          <div v-else class="min-w-full">
            <div class="sticky top-0 z-10 grid border-b border-border bg-background" :style="{ gridTemplateColumns: `64px repeat(${timeDays.length}, minmax(0, 1fr))` }">
              <div class="border-r border-border"></div>
              <div v-for="day in timeDays" :key="dateKey(day)" class="border-r border-border px-2 py-2 text-center">
                <p class="text-xs text-muted-foreground">{{ day.toLocaleDateString(locale.locale, { weekday: "short" }) }}</p>
                <p class="mt-0.5 text-lg" :class="isToday(day) ? 'font-semibold text-destructive' : ''">{{ day.getDate() }}</p>
              </div>
            </div>
            <div v-if="longRunningEvents.length" class="grid border-b border-border" :style="{ gridTemplateColumns: `64px repeat(${timeDays.length}, minmax(0, 1fr))` }">
              <div class="border-r border-border"></div>
              <div :style="{ gridColumn: `span ${timeDays.length}` }" class="p-1">
                <button v-for="event in longRunningEvents" :key="event.id" type="button" class="mb-1 block w-full truncate rounded border-l-2 px-1.5 py-1 text-left text-xs" :style="eventStyle(event)" @click="selectEvent(event.id)">{{ event.title }}</button>
              </div>
            </div>
            <div class="grid border-b border-border" :style="{ gridTemplateColumns: `64px repeat(${timeDays.length}, minmax(0, 1fr))` }">
              <div class="border-r border-border px-2 py-2 text-right text-2xs text-muted-foreground">{{ t("calendar.allDay") }}</div>
              <div v-for="day in timeDays" :key="`allday-${dateKey(day)}`" class="min-h-8 border-r border-border p-1">
                <button v-for="event in allDayEvents(day)" :key="event.id" type="button" class="mb-1 block w-full truncate rounded border-l-2 px-1.5 py-0.5 text-left text-xs" :style="eventStyle(event)" @click="selectEvent(event.id)">{{ event.title }}</button>
              </div>
            </div>
            <div v-for="hour in HOURS" :key="hour" class="grid border-b border-border/60" :style="{ gridTemplateColumns: `64px repeat(${timeDays.length}, minmax(0, 1fr))` }">
              <div class="border-r border-border px-2 py-1 text-right text-2xs text-muted-foreground">{{ new Date(2000, 0, 1, hour).toLocaleTimeString(locale.locale, { hour: "numeric" }) }}</div>
              <div v-for="day in timeDays" :key="`${dateKey(day)}-${hour}`" class="min-h-14 border-r border-border/60 p-1">
                <button v-for="event in eventsAtHour(day, hour)" :key="event.id" type="button" class="mb-1 block w-full truncate rounded border-l-2 px-1.5 py-1 text-left text-xs" :style="eventStyle(event)" @click="selectEvent(event.id)">
                  <span class="font-medium">{{ event.title }}</span>
                  <span class="ml-1 opacity-70">{{ eventTime(event) }}</span>
                </button>
              </div>
            </div>
          </div>
        </div>

        <aside v-if="mode === 'day'" class="w-72 shrink-0 border-l border-border bg-card/20 p-4">
          <div class="mb-4 grid grid-cols-7 gap-1 text-center text-2xs text-muted-foreground">
            <span v-for="label in miniWeekdayLabels" :key="`mini-${label}`">{{ label }}</span>
          </div>
          <div data-testid="mini-month" class="grid grid-cols-7 gap-1 text-center text-xs">
            <button
              v-for="day in miniMonthDays"
              :key="`mini-${dateKey(day)}`"
              type="button"
              :data-date="dateKey(day)"
              :aria-current="isSelected(day) ? 'date' : undefined"
              class="grid size-7 place-items-center rounded-full"
              :class="[
                isSelected(day) ? 'bg-destructive font-semibold text-white' : '',
                !isSelected(day) && isToday(day) ? 'font-semibold text-destructive hover:bg-accent' : '',
                !isSelected(day) && !isToday(day) && !isInMonth(day) ? 'text-muted-foreground/40' : '',
                !isSelected(day) && !isToday(day) && isInMonth(day) ? 'hover:bg-accent' : '',
              ]"
              @click="anchorDate = day"
            >
              {{ day.getDate() }}
            </button>
          </div>
          <div class="mt-6 border-t border-border pt-5">
            <template v-if="selectedEvent">
              <p class="text-xs font-medium text-muted-foreground">{{ calendarForEvent(selectedEvent)?.title }}</p>
              <h2 class="mt-1 text-lg font-semibold">{{ selectedEvent.title }}</h2>
              <p class="mt-2 flex items-start gap-2 text-sm text-muted-foreground"><Clock3 :size="14" class="mt-0.5 shrink-0" />{{ selectedEventTime(selectedEvent) }}</p>
              <p v-if="selectedEvent.location" class="mt-3 flex items-start gap-2 text-sm"><MapPin :size="14" class="mt-0.5 shrink-0" />{{ selectedEvent.location }}</p>
              <p v-if="selectedEvent.notes" class="mt-3 flex items-start gap-2 whitespace-pre-wrap text-sm"><FileText :size="14" class="mt-0.5 shrink-0" />{{ selectedEvent.notes }}</p>
              <div class="mt-4 flex items-center gap-2">
                <button type="button" class="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs transition-colors hover:bg-accent disabled:opacity-40" :disabled="!canModifyEvent(selectedEvent)" @click="openEditEvent">
                  <Pencil :size="13" />
                  {{ t("calendar.edit") }}
                </button>
                <button
                  type="button"
                  class="inline-flex h-8 items-center gap-1.5 rounded-md border border-destructive/25 px-2.5 text-xs text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-40"
                  :disabled="deletingEvent || !canModifyEvent(selectedEvent)"
                  @click="deleteSelectedEvent"
                >
                  <Spinner v-if="deletingEvent" :size="12" />
                  <Trash2 v-else :size="13" />
                  {{ t("calendar.delete") }}
                </button>
              </div>
            </template>
            <p v-else class="text-center text-sm text-muted-foreground">{{ t("calendar.noEventSelected") }}</p>
          </div>
        </aside>
      </div>
    </div>

    <div v-if="selectedEvent && mode !== 'day'" class="fixed inset-0 z-50 grid place-items-center bg-black/35 p-6" @click.self="selectedEventId = null">
      <div class="w-full max-w-md rounded-xl border border-border bg-card p-5 shadow-2xl">
        <div class="flex items-start gap-3">
          <span class="mt-1 size-3 shrink-0 rounded-full" :style="dotStyle(calendarForEvent(selectedEvent) ?? { id: '', title: '', color: eventColor(selectedEvent), enabled: true, readOnly: false, isDefault: false, order: 0 })"></span>
          <div class="min-w-0 flex-1">
            <p class="text-xs text-muted-foreground">{{ calendarForEvent(selectedEvent)?.title }}</p>
            <h2 class="mt-1 text-lg font-semibold">{{ selectedEvent.title }}</h2>
          </div>
          <button type="button" class="grid size-8 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" @click="selectedEventId = null"><X :size="15" /></button>
        </div>
        <p class="mt-4 flex items-start gap-2 text-sm text-muted-foreground"><Clock3 :size="14" class="mt-0.5 shrink-0" />{{ selectedEventTime(selectedEvent) }}</p>
        <p v-if="selectedEvent.location" class="mt-3 flex items-start gap-2 text-sm"><MapPin :size="14" class="mt-0.5 shrink-0" />{{ selectedEvent.location }}</p>
        <p v-if="selectedEvent.notes" class="mt-3 flex items-start gap-2 whitespace-pre-wrap text-sm"><FileText :size="14" class="mt-0.5 shrink-0" />{{ selectedEvent.notes }}</p>
        <div class="mt-5 flex items-center justify-end gap-2 border-t border-border pt-4">
          <button type="button" class="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-3 text-xs transition-colors hover:bg-accent disabled:opacity-40" :disabled="!canModifyEvent(selectedEvent)" @click="openEditEvent">
            <Pencil :size="13" />
            {{ t("calendar.edit") }}
          </button>
          <button type="button" class="inline-flex h-8 items-center gap-1.5 rounded-md border border-destructive/25 px-3 text-xs text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-40" :disabled="deletingEvent || !canModifyEvent(selectedEvent)" @click="deleteSelectedEvent">
            <Spinner v-if="deletingEvent" :size="12" />
            <Trash2 v-else :size="13" />
            {{ t("calendar.delete") }}
          </button>
        </div>
      </div>
    </div>
  </div>
  <div v-if="editorOpen" class="fixed inset-0 z-[70] grid place-items-center bg-black/40 p-5" @click.self="closeEditor">
    <form class="max-h-[92vh] w-full max-w-xl overflow-y-auto rounded-xl border border-border bg-card p-5 text-card-foreground shadow-2xl" @submit.prevent="saveCalendarEvent">
      <div class="flex items-center gap-2">
        <CalendarDays :size="17" class="text-primary" />
        <h2 class="min-w-0 flex-1 text-base font-semibold">{{ draft.eventId ? t("calendar.editEvent") : t("calendar.newEvent") }}</h2>
        <button type="button" class="grid size-8 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" :aria-label="t('common.close')" @click="closeEditor">
          <X :size="16" />
        </button>
      </div>

      <div class="mt-4 space-y-3">
        <label class="block text-xs text-muted-foreground">
          {{ t("calendar.eventTitle") }}
          <input v-model="draft.title" data-testid="calendar-event-title" class="mt-1 h-9 w-full rounded-md border border-border bg-background px-3 text-sm text-foreground outline-none focus:border-primary" autofocus />
        </label>

        <label class="block text-xs text-muted-foreground">
          {{ t("calendar.eventCalendar") }}
          <select v-model="draft.calendarId" class="mt-1 h-9 w-full rounded-md border border-border bg-background px-2 text-sm text-foreground outline-none focus:border-primary">
            <option v-for="calendar in writableCalendars" :key="calendar.id" :value="calendar.id">{{ calendar.title }}</option>
          </select>
        </label>

        <label class="flex items-center gap-2 text-sm">
          <input v-model="draft.allDay" type="checkbox" class="size-4 rounded border-border" @change="onAllDayChange" />
          {{ t("calendar.allDay") }}
        </label>

        <div class="grid gap-3 sm:grid-cols-2">
          <label class="block text-xs text-muted-foreground">
            {{ t("calendar.start") }}
            <input v-model="draft.start" type="datetime-local" class="mt-1 h-9 w-full rounded-md border border-border bg-background px-2 text-sm text-foreground outline-none focus:border-primary" />
          </label>
          <label class="block text-xs text-muted-foreground">
            {{ t("calendar.end") }}
            <input v-model="draft.end" type="datetime-local" class="mt-1 h-9 w-full rounded-md border border-border bg-background px-2 text-sm text-foreground outline-none focus:border-primary" />
          </label>
        </div>

        <label class="block text-xs text-muted-foreground">
          {{ t("calendar.eventLocation") }}
          <input v-model="draft.location" class="mt-1 h-9 w-full rounded-md border border-border bg-background px-3 text-sm text-foreground outline-none focus:border-primary" />
        </label>

        <label class="block text-xs text-muted-foreground">
          {{ t("calendar.eventNotes") }}
          <textarea v-model="draft.notes" class="mt-1 min-h-28 w-full resize-y rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary"></textarea>
        </label>

        <label class="block text-xs text-muted-foreground">
          URL
          <input v-model="draft.url" type="url" class="mt-1 h-9 w-full rounded-md border border-border bg-background px-3 text-sm text-foreground outline-none focus:border-primary" />
        </label>
      </div>

      <p v-if="editorError" class="mt-3 rounded-md border border-destructive/25 bg-destructive/5 p-2.5 text-xs text-destructive">{{ editorError }}</p>

      <div class="mt-5 flex items-center justify-end gap-2 border-t border-border pt-4">
        <button type="button" class="h-8 rounded-md border border-border px-3 text-xs text-muted-foreground hover:bg-accent hover:text-foreground" :disabled="savingEvent" @click="closeEditor">{{ t("common.cancel") }}</button>
        <button type="submit" class="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50" :disabled="savingEvent">
          <Spinner v-if="savingEvent" :size="12" />
          <Save v-else :size="13" />
          {{ savingEvent ? t("calendar.saving") : t("calendar.save") }}
        </button>
      </div>
    </form>
  </div>
</template>
