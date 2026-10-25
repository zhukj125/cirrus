import type { Timeline } from "@/types";

export function yearLabel(year: number): string {
  return `${year}`;
}

export function monthLabel(month: number, locale = "en-US"): string {
  return new Date(2020, month - 1, 1).toLocaleDateString(locale, { month: "short" });
}

export function yearMonthLabel(year: number, month: number, locale = "en-US"): string {
  return new Date(year, month - 1, 1).toLocaleDateString(locale, { month: "short", year: "numeric" });
}

export interface TimelineMonthView {
  key: string;
  label: string;
  year: number;

  month: number;

  count: number;

  startRank: number;

  fullLabel: string;
}

export interface TimelineYearView {
  key: string;
  label: string;
  year: number;

  count: number;

  months: TimelineMonthView[];
}

export function timelineYearsDesc(timeline: Timeline | null, locale = "en-US"): TimelineYearView[] {
  if (!timeline) return [];
  return [...timeline.years].reverse().map((year) => ({
    key: String(year.year),
    label: yearLabel(year.year),
    year: year.year,
    count: year.count,
    months: [...year.months].reverse().map((month) => ({
      key: `${month.year}-${String(month.month).padStart(2, "0")}`,
      label: monthLabel(month.month, locale),
      year: month.year,
      month: month.month,
      count: month.count,
      startRank: month.startRank,
      fullLabel: yearMonthLabel(month.year, month.month, locale),
    })),
  }));
}

export interface TimelineSelection {
  year: number;

  month: number | null;
}

export function timelineSegment(timeline: Timeline | null, selection: TimelineSelection): { count: number; startRank: number } | null {
  const year = timeline?.years.find((item) => item.year === selection.year);
  if (!year) return null;
  if (selection.month === null) {
    const newest = year.months[year.months.length - 1];
    return newest ? { count: year.count, startRank: newest.startRank } : null;
  }
  const month = year.months.find((item) => item.month === selection.month);
  return month ? { count: month.count, startRank: month.startRank } : null;
}
