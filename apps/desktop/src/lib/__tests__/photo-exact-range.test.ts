import { describe, expect, it } from "vitest";

import { createExactRangeLoader, createExactRangeStore, finishExactRange, planExactRange, type ExactRangeResult } from "@/lib/photos/exact";
import type { AssetDistribution } from "@/types";

const WEEK = 604_800_000;

const dist: AssetDistribution = {
  bucketWidthMs: WEEK,
  total: 20,
  buckets: [
    { startMs: 0, count: 5, startRank: 19, secondMonthRank: 15 },
    { startMs: WEEK, count: 7, startRank: 14, secondMonthRank: 8 },
    { startMs: 2 * WEEK, count: 8, startRank: 7, secondMonthRank: 0 },
  ],
};

describe("任意日期区间的精确张数", () => {
  it("整周直接加直方图计数，不取边界照片", () => {
    const plan = planExactRange(dist, { after: WEEK, before: 2 * WEEK });
    expect(plan.insideCount).toBe(7);
    expect(plan.boundaries).toEqual([]);
    expect(finishExactRange(plan, [])).toEqual({ count: 7, newestRank: 8, oldestRank: 14 });
  });

  it("只取两个边界周，并按真实拍摄时间筛选", () => {
    const plan = planExactRange(dist, { after: WEEK / 2, before: 2.5 * WEEK });
    expect(plan.insideCount).toBe(7);
    expect(plan.boundaries.map((boundary) => boundary.startMs)).toEqual([0, 2 * WEEK]);

    const first = [0.25 * WEEK, 0.6 * WEEK, 0.8 * WEEK, 0.9 * WEEK, 0.99 * WEEK];
    const last = [2.1 * WEEK, 2.2 * WEEK, 2.3 * WEEK, 2.4 * WEEK, 2.6 * WEEK, 2.7 * WEEK, 2.8 * WEEK, 2.9 * WEEK];
    expect(finishExactRange(plan, [first, last])).toEqual({ count: 15, newestRank: 4, oldestRank: 18 });
  });

  it("边界桶数据对不上时失败，不把不完整的数当精确值", () => {
    const plan = planExactRange(dist, { after: WEEK / 2, before: 2 * WEEK });
    expect(() => finishExactRange(plan, [[WEEK / 2]])).toThrow("does not match");
  });

  it("同一个边界周跨多个区间只取一次，结果缓存并发去重", async () => {
    let pageCalls = 0;
    const oneBucket: AssetDistribution = {
      bucketWidthMs: WEEK,
      total: 5,
      buckets: [{ startMs: 0, count: 5, startRank: 4, secondMonthRank: 0 }],
    };
    const reader = createExactRangeLoader(oneBucket, async (startRank, limit) => {
      pageCalls += 1;
      expect(startRank).toBe(4);
      expect(limit).toBe(5);
      return [1, 2, 3, 4, 5];
    });

    const first = { after: 0, before: 3 };
    const second = { after: 2, before: 5 };
    expect(await reader(first)).toEqual({ count: 2, newestRank: 3, oldestRank: 4 });
    expect(await reader(second)).toEqual({ count: 3, newestRank: 1, oldestRank: 3 });
    expect(pageCalls).toBe(1);

    let loads = 0;
    const store = createExactRangeStore(async (): Promise<ExactRangeResult> => {
      loads += 1;
      return { count: 3, newestRank: 1, oldestRank: 3 };
    });
    const [a, b] = await Promise.all([store.ensure(first), store.ensure(first)]);
    expect(a).toEqual(b);
    expect(loads).toBe(1);
  });
});
