import { ref, type Ref } from "vue";

import type { AssetDistribution } from "@/types";

import { mapWithConcurrency } from "./concurrency";
import type { DateBounds } from "./dates";

const EXACT_RANGE_PAGE_SIZE = 60;

const MAX_EXACT_RANGE_PAGES = 20;

const EXACT_RANGE_PAGE_CONCURRENCY = 4;

const EXACT_RANGE_MAX_INFLIGHT_PAGES = 4;

export interface ExactRangeResult {
  count: number;

  newestRank: number;

  oldestRank: number;
}

export interface ExactRangeBoundary {
  startMs: number;

  startRank: number;

  count: number;
}

export interface ExactRangePlan {
  bounds: DateBounds;

  insideCount: number;
  insideNewestRank: number | null;
  insideOldestRank: number | null;

  boundaries: ExactRangeBoundary[];
}

export function exactRangeKey(bounds: DateBounds): string {
  return `${bounds.after ?? "min"}:${bounds.before ?? "max"}`;
}

function inRange(takenAt: number, bounds: DateBounds): boolean {
  if (bounds.after !== null && takenAt < bounds.after) return false;
  if (bounds.before !== null && takenAt >= bounds.before) return false;
  return true;
}

export function planExactRange(dist: AssetDistribution, bounds: DateBounds): ExactRangePlan {
  if (dist.bucketWidthMs <= 0) throw new Error("Photo timeline is missing its bucket width; exact count failed");
  if (bounds.after !== null && bounds.before !== null && bounds.after >= bounds.before) {
    throw new Error("Date range is empty; exact count failed");
  }

  const width = dist.bucketWidthMs;
  const plan: ExactRangePlan = {
    bounds,
    insideCount: 0,
    insideNewestRank: null,
    insideOldestRank: null,
    boundaries: [],
  };

  for (const bucket of dist.buckets) {
    if (bucket.count <= 0) continue;
    const start = bucket.startMs;
    const end = start + width;
    if ((bounds.before !== null && start >= bounds.before) || (bounds.after !== null && end <= bounds.after)) continue;

    const fullyInside = (bounds.after === null || start >= bounds.after) && (bounds.before === null || end <= bounds.before);
    if (fullyInside) {
      plan.insideCount += bucket.count;
      const newest = bucket.secondMonthRank ?? bucket.startRank;
      plan.insideNewestRank = plan.insideNewestRank === null ? newest : Math.min(plan.insideNewestRank, newest);
      plan.insideOldestRank = plan.insideOldestRank === null ? bucket.startRank : Math.max(plan.insideOldestRank, bucket.startRank);
      continue;
    }

    plan.boundaries.push({ startMs: start, startRank: bucket.startRank, count: bucket.count });
  }

  const pages = plan.boundaries.reduce((sum, boundary) => sum + Math.ceil(boundary.count / EXACT_RANGE_PAGE_SIZE), 0);
  if (pages > MAX_EXACT_RANGE_PAGES) {
    throw new Error(`Exact count requires ${pages} pages, exceeding the ${MAX_EXACT_RANGE_PAGES}-page limit (invalid histogram data)`);
  }
  return plan;
}

export function finishExactRange(plan: ExactRangePlan, fetched: readonly (readonly (number | null)[])[]): ExactRangeResult {
  if (fetched.length !== plan.boundaries.length) throw new Error("Boundary-week data did not match the plan");

  let count = plan.insideCount;
  let newest = plan.insideNewestRank;
  let oldest = plan.insideOldestRank;

  for (let boundaryIndex = 0; boundaryIndex < plan.boundaries.length; boundaryIndex += 1) {
    const boundary = plan.boundaries[boundaryIndex] as ExactRangeBoundary;
    const dates = fetched[boundaryIndex] as readonly (number | null)[];
    if (dates.length !== boundary.count) {
      throw new Error(`Boundary-week histogram count (${boundary.count}) does not match the retrieved count (${dates.length})`);
    }
    for (let index = 0; index < dates.length; index += 1) {
      const takenAt = dates[index];

      if (takenAt === null || !Number.isFinite(takenAt) || takenAt <= 0) {
        throw new Error("A photo has no capture date; exact count failed");
      }
      if (!inRange(takenAt, plan.bounds)) continue;
      const rank = boundary.startRank - index;
      if (rank < 0) throw new Error("Photo rank is out of range; exact count failed");
      count += 1;
      newest = newest === null ? rank : Math.min(newest, rank);
      oldest = oldest === null ? rank : Math.max(oldest, rank);
    }
  }

  return {
    count,
    newestRank: newest ?? 0,
    oldestRank: oldest ?? 0,
  };
}

export type ExactRangePageLoader = (startRank: number, limit: number) => Promise<readonly (number | null)[]>;

export function createExactRangeLoader(dist: AssetDistribution, loadPage: ExactRangePageLoader): (bounds: DateBounds) => Promise<ExactRangeResult> {
  const bucketCache = new Map<string, Promise<readonly (number | null)[]>>();
  let activePages = 0;
  const waiting: Array<() => void> = [];

  async function loadPageWithSlot(startRank: number, limit: number): Promise<readonly (number | null)[]> {
    if (activePages >= EXACT_RANGE_MAX_INFLIGHT_PAGES) {
      await new Promise<void>((resolve) => waiting.push(resolve));
    }
    activePages += 1;
    try {
      return await loadPage(startRank, limit);
    } finally {
      activePages -= 1;
      waiting.shift()?.();
    }
  }

  async function loadBoundary(boundary: ExactRangeBoundary): Promise<readonly (number | null)[]> {
    const requests: Array<{ startRank: number; limit: number }> = [];
    let fetched = 0;
    while (fetched < boundary.count) {
      const limit = Math.min(EXACT_RANGE_PAGE_SIZE, boundary.count - fetched);
      requests.push({ startRank: boundary.startRank - fetched, limit });
      fetched += limit;
    }

    const pages = await mapWithConcurrency(requests, EXACT_RANGE_PAGE_CONCURRENCY, async (request) => {
      const dates = await loadPageWithSlot(request.startRank, request.limit);
      if (dates.length !== request.limit) {
        throw new Error(`Boundary week returned ${dates.length} photos; expected ${request.limit}`);
      }
      return dates;
    });
    return pages.flat();
  }

  return async (bounds: DateBounds): Promise<ExactRangeResult> => {
    const plan = planExactRange(dist, bounds);
    const fetched = await mapWithConcurrency(plan.boundaries, 2, async (boundary) => {
      const key = `${boundary.startMs}:${boundary.startRank}:${boundary.count}`;
      let request = bucketCache.get(key);
      if (!request) {
        request = loadBoundary(boundary);
        bucketCache.set(key, request);
      }
      try {
        return await request;
      } catch (error) {
        bucketCache.delete(key);
        throw error;
      }
    });
    return finishExactRange(plan, fetched);
  };
}

export interface ExactRangeStore {
  readonly results: Ref<Record<string, ExactRangeResult>>;
  readonly pending: Ref<Record<string, boolean>>;
  cached(bounds: DateBounds): ExactRangeResult | null;
  isPending(bounds: DateBounds): boolean;
  ensure(bounds: DateBounds): Promise<ExactRangeResult | null>;
  clear(): void;
}

export function createExactRangeStore(load: (bounds: DateBounds) => Promise<ExactRangeResult>): ExactRangeStore {
  const results = ref<Record<string, ExactRangeResult>>({});
  const pending = ref<Record<string, boolean>>({});
  const inflight = new Map<string, Promise<ExactRangeResult | null>>();
  let generation = 0;

  function cached(bounds: DateBounds): ExactRangeResult | null {
    return results.value[exactRangeKey(bounds)] ?? null;
  }

  function isPending(bounds: DateBounds): boolean {
    return pending.value[exactRangeKey(bounds)] === true;
  }

  function ensure(bounds: DateBounds): Promise<ExactRangeResult | null> {
    const key = exactRangeKey(bounds);
    const hit = results.value[key];
    if (hit) return Promise.resolve(hit);
    const running = inflight.get(key);
    if (running) return running;

    const requestGeneration = generation;
    pending.value = { ...pending.value, [key]: true };
    const request = Promise.resolve()
      .then(() => load({ ...bounds }))
      .then((result) => {
        if (requestGeneration !== generation) return null;
        if (!Number.isInteger(result?.count) || result.count < 0 || !Number.isInteger(result.newestRank) || !Number.isInteger(result.oldestRank)) {
          return null;
        }
        results.value = { ...results.value, [key]: result };
        return result;
      })
      .catch(() => null)
      .finally(() => {
        if (requestGeneration !== generation) return;
        if (inflight.get(key) === request) inflight.delete(key);
        const next = { ...pending.value };
        delete next[key];
        pending.value = next;
      });
    inflight.set(key, request);
    return request;
  }

  function clear(): void {
    generation += 1;
    results.value = {};
    pending.value = {};
    inflight.clear();
  }

  return { results, pending, cached, isPending, ensure, clear };
}
