import type { PhotoItem } from "@/types";

export type RangeKey = "all" | "today" | "week" | "month" | "year" | "custom";

export const RANGE_OPTIONS: Array<{ key: RangeKey; label: string }> = [
  { key: "all", label: "All" },
  { key: "today", label: "Today" },
  { key: "week", label: "Last 7 days" },
  { key: "month", label: "Last 30 days" },
  { key: "year", label: "This year" },
  { key: "custom", label: "Custom" },
];

export interface DateBounds {
  after: number | null;
  before: number | null;
}

export const UNBOUNDED: DateBounds = { after: null, before: null };

export function startOfDay(date: Date): number {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
}

export function addDays(ms: number, days: number): number {
  const date = new Date(ms);
  date.setDate(date.getDate() + days);
  return date.getTime();
}

export function formatDateInput(ms: number): string {
  const date = new Date(ms);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function presetRange(key: Exclude<RangeKey, "all" | "custom">, now: Date = new Date()): { after: number; before: number } {
  const today = startOfDay(now);
  const tomorrow = addDays(today, 1);
  switch (key) {
    case "today":
      return { after: today, before: tomorrow };
    case "week":
      return { after: addDays(today, -6), before: tomorrow };
    case "month":
      return { after: addDays(today, -29), before: tomorrow };
    case "year":
      return { after: new Date(now.getFullYear(), 0, 1).getTime(), before: new Date(now.getFullYear() + 1, 0, 1).getTime() };
  }
}

export function dayLabel(dayStart: number, now: Date = new Date()): string {
  const today = startOfDay(now);
  if (dayStart === today) return "Today";
  if (dayStart === addDays(today, -1)) return "Yesterday";
  const date = new Date(dayStart);
  const weekday = date.toLocaleDateString("en-US", { weekday: "short" });
  const prefix = date.getFullYear() === now.getFullYear() ? "" : `${date.getFullYear()} `;
  return `${prefix}${date.toLocaleDateString("en-US", { month: "short", day: "numeric" })}, ${weekday}`;
}

export function filterByRange(photos: PhotoItem[], bounds: DateBounds): PhotoItem[] {
  const { after, before } = bounds;
  if (after === null && before === null) return photos;
  return photos.filter((photo) => {
    const taken = photo.takenAt;
    if (!taken || taken <= 0) return false;
    if (after !== null && taken < after) return false;
    if (before !== null && taken >= before) return false;
    return true;
  });
}

export interface PhotoDaySection {
  key: string;
  label: string;

  items: Array<{ photo: PhotoItem; index: number }>;
}

export function groupByDay(photos: PhotoItem[], now: Date = new Date()): PhotoDaySection[] {
  const buckets = new Map<number, Array<{ photo: PhotoItem; index: number }>>();
  photos.forEach((photo, index) => {
    const key = photo.takenAt && photo.takenAt > 0 ? startOfDay(new Date(photo.takenAt)) : Number.NaN;
    const bucket = buckets.get(key);
    if (bucket) bucket.push({ photo, index });
    else buckets.set(key, [{ photo, index }]);
  });

  return [...buckets.entries()]
    .sort((a, b) => (Number.isNaN(a[0]) ? 1 : Number.isNaN(b[0]) ? -1 : b[0] - a[0]))
    .map(([dayStart, items]) => ({
      key: String(dayStart),
      label: Number.isNaN(dayStart) ? "Unknown date" : dayLabel(dayStart, now),
      items,
    }));
}
