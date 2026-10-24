import { describe, expect, it } from "vitest";

import { estimateCount, monthBounds, segmentLanding, yearBounds } from "@/lib/photos/distribution";
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

describe("区间落点（segmentLanding）", () => {
  it("落点用「区间上界前一刻」所在桶的**主锚点**（桶里最老那张）", () => {
    expect(segmentLanding(dist, { after: WEEK, before: 2 * WEEK })).toEqual({ count: 7, startRank: 14 });

    expect(segmentLanding(dist, { after: 2 * WEEK, before: 3 * WEEK })).toEqual({ count: 8, startRank: 7 });
  });

  it("区间只覆盖桶的一部分时仍从整桶最老端起跳，靠日期筛选掉邻居周", () => {
    const landing = segmentLanding(dist, { after: WEEK + WEEK / 2, before: 2 * WEEK });
    expect(landing?.startRank).toBe(14);

    expect(landing?.count).toBe(4);
  });

  it("区间一直到最新（没有上界）时落点给 0: 从最新翻就是对的", () => {
    expect(segmentLanding(dist, { after: 2 * WEEK, before: null })).toEqual({ count: 8, startRank: 0 });
  });

  it("「全部」（两端都不设界）不走直方图，返回 null", () => {
    expect(segmentLanding(dist, { after: null, before: null })).toBeNull();
  });

  it("没有直方图（null / 空桶 / 桶宽为 0）一律返回 null，调用方退回逐页翻", () => {
    expect(segmentLanding(null, { after: 0, before: WEEK })).toBeNull();
    expect(segmentLanding({ bucketWidthMs: WEEK, total: 0, buckets: [] }, { after: 0, before: WEEK })).toBeNull();
    expect(segmentLanding({ bucketWidthMs: 0, total: 3, buckets: dist.buckets }, { after: 0, before: WEEK })).toBeNull();
  });

  it("区间的另一端越界时仍给最靠边那一周的主锚点", () => {
    expect(segmentLanding(dist, { after: 0, before: 1 })).toEqual({ count: 0, startRank: 19 });

    expect(segmentLanding(dist, { after: 9 * WEEK, before: 10 * WEEK })).toEqual({ count: 0, startRank: 7 });
  });

  it("空桶不会制造额外落点，边界周统一回退到桶主锚点", () => {
    const withEmpty: AssetDistribution = {
      bucketWidthMs: WEEK,
      total: 20,
      buckets: [
        { startMs: 0, count: 5, startRank: 19, secondMonthRank: 15 },
        { startMs: WEEK, count: 0, startRank: 14, secondMonthRank: null },
        { startMs: 2 * WEEK, count: 8, startRank: 14, secondMonthRank: 7 },
      ],
    };
    expect(segmentLanding(withEmpty, { after: 2 * WEEK, before: 3 * WEEK })?.startRank).toBe(14);
    expect(segmentLanding(withEmpty, { after: WEEK, before: 2 * WEEK })?.startRank).toBe(14);
  });

  it("年月锚点与桶主锚点取更老的那个，保证整段不漏", () => {
    const bounds = { after: WEEK, before: 2 * WEEK };

    expect(segmentLanding(dist, bounds, 4)?.startRank).toBe(14);

    expect(segmentLanding(dist, bounds, 12)?.startRank).toBe(14);

    expect(segmentLanding(dist, bounds, 16)?.startRank).toBe(16);
  });

  it("名次夹在 [0, total - 1] 之间（落到 total 会取到空页）", () => {
    const bounds = { after: WEEK, before: 2 * WEEK };

    expect(segmentLanding(dist, bounds, -5)?.startRank).toBe(14);

    expect(segmentLanding(dist, bounds, 999)?.startRank).toBe(19);

    const bogus: AssetDistribution = { bucketWidthMs: WEEK, total: 3, buckets: [{ startMs: 0, count: 1, startRank: 2, secondMonthRank: 99 }] };
    expect(segmentLanding(bogus, { after: 0, before: WEEK })?.startRank).toBe(2);
  });
});

describe("区间近似张数（estimateCount）", () => {
  it("区间两端正好落在桶边界上时等于整桶求和", () => {
    expect(estimateCount(dist, { after: WEEK, before: 3 * WEEK })).toBe(15);
    expect(estimateCount(dist, { after: null, before: WEEK })).toBe(5);
    expect(estimateCount(dist, { after: 2 * WEEK, before: null })).toBe(8);
  });

  it("桶与区间边界不重合时按重叠比例摊分（所以只能是「约」）", () => {
    expect(estimateCount(dist, { after: 2 * WEEK + WEEK * 0.75, before: 3 * WEEK })).toBe(2);

    expect(estimateCount(dist, { after: WEEK + WEEK / 2, before: 2 * WEEK + WEEK / 2 })).toBe(8);
  });

  it("桶宽未知时不猜（返回 0）", () => {
    expect(estimateCount({ bucketWidthMs: 0, total: 3, buckets: dist.buckets }, { after: 0, before: WEEK })).toBe(0);
  });
});

describe("本地年/月边界", () => {
  it("月是 [当月 1 日 00:00, 次月 1 日 00:00)", () => {
    expect(monthBounds(2024, 3)).toEqual({ after: new Date(2024, 2, 1).getTime(), before: new Date(2024, 3, 1).getTime() });

    expect(monthBounds(2024, 12)).toEqual({ after: new Date(2024, 11, 1).getTime(), before: new Date(2025, 0, 1).getTime() });
  });

  it("年是 [1 月 1 日 00:00, 次年 1 月 1 日 00:00)", () => {
    expect(yearBounds(2024)).toEqual({ after: new Date(2024, 0, 1).getTime(), before: new Date(2025, 0, 1).getTime() });
  });
});
