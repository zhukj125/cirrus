import type { AssetBucket, AssetDistribution } from "@/types";

import type { DateBounds } from "./dates";

export interface SegmentLanding {
  count: number;

  startRank: number;
}

export function monthBounds(year: number, month: number): { after: number; before: number } {
  return { after: new Date(year, month - 1, 1).getTime(), before: new Date(year, month, 1).getTime() };
}

export function yearBounds(year: number): { after: number; before: number } {
  return { after: new Date(year, 0, 1).getTime(), before: new Date(year + 1, 0, 1).getTime() };
}

function bucketAt(dist: AssetDistribution, ms: number): AssetBucket | null {
  const buckets = dist.buckets;
  const first = buckets[0];
  const last = buckets[buckets.length - 1];
  if (!first || !last) return null;
  if (ms < first.startMs) return first;
  if (ms >= last.startMs) return last;

  let low = 0;
  let high = buckets.length - 1;
  while (low < high) {
    const mid = Math.ceil((low + high) / 2);
    const bucket = buckets[mid];
    if (bucket && bucket.startMs <= ms) low = mid;
    else high = mid - 1;
  }
  return buckets[low] ?? null;
}

export function estimateCount(dist: AssetDistribution, bounds: DateBounds): number {
  const { after, before } = bounds;
  const width = dist.bucketWidthMs;
  if (width <= 0) return 0;
  let count = 0;
  for (const bucket of dist.buckets) {
    const start = bucket.startMs;
    const end = start + width;
    const from = after === null ? start : Math.max(start, after);
    const to = before === null ? end : Math.min(end, before);
    if (to <= from) continue;
    const overlap = (to - from) / width;
    count += overlap >= 1 ? bucket.count : Math.round(bucket.count * overlap);
  }
  return count;
}

function clampRank(dist: AssetDistribution, rank: number): number {
  if (!Number.isFinite(rank)) return 0;
  const max = Math.max(0, dist.total - 1);
  return Math.min(max, Math.max(0, Math.floor(rank)));
}

export function segmentLanding(dist: AssetDistribution | null, bounds: DateBounds, extraAnchor: number | null = null): SegmentLanding | null {
  if (!dist || dist.buckets.length === 0 || dist.bucketWidthMs <= 0) return null;
  const { after, before } = bounds;
  if (after === null && before === null) return null;

  const count = estimateCount(dist, bounds);

  const startBucket = after === null ? dist.buckets[0] : bucketAt(dist, after);
  const anchor = before === null ? 0 : (startBucket?.startRank ?? 0);

  const startRank = clampRank(dist, extraAnchor === null ? anchor : Math.max(anchor, extraAnchor));
  return { count, startRank };
}
