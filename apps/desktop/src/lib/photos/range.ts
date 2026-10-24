import type { PhotoItem } from "@/types";

import { filterByRange, type DateBounds } from "./dates";

export function oldestTakenAt(photos: readonly PhotoItem[]): number | null {
  let oldest: number | null = null;
  for (const photo of photos) {
    const taken = photo.takenAt;
    if (taken === null || taken <= 0) continue;
    if (oldest === null || taken < oldest) oldest = taken;
  }
  return oldest;
}

export function isPastLowerBound(oldest: number | null, bounds: DateBounds): boolean {
  const after = bounds.after;
  return after !== null && oldest !== null && oldest < after;
}

export function passedLowerBound(photos: readonly PhotoItem[], bounds: DateBounds): boolean {
  return isPastLowerBound(oldestTakenAt(photos), bounds);
}

export function passedUpperBound(photos: readonly PhotoItem[], bounds: DateBounds): boolean {
  const before = bounds.before;
  if (before === null) return false;
  return photos.some((photo) => photo.takenAt !== null && photo.takenAt > 0 && photo.takenAt >= before);
}

export interface RangeScan {
  matched: PhotoItem[];

  oldestTakenAt: number | null;

  passedLowerBound: boolean;
}

export function scanRangePage(page: PhotoItem[], bounds: DateBounds, oldestSoFar: number | null = null): RangeScan {
  const pageOldest = oldestTakenAt(page);
  const oldest = pageOldest === null ? oldestSoFar : oldestSoFar === null ? pageOldest : Math.min(oldestSoFar, pageOldest);

  return {
    matched: filterByRange(page, bounds),
    oldestTakenAt: oldest,
    passedLowerBound: isPastLowerBound(oldest, bounds),
  };
}
