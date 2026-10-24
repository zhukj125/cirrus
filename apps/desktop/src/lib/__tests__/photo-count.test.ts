import { describe, expect, it } from "vitest";

import { countLabel } from "@/lib/photos/count";

const base = { shown: 60, total: null, filtered: false, hasMore: true, truncated: false } as const;

describe("张数文案", () => {
  it("拿到总数时显示「已加载 N / 共 M 张」", () => {
    expect(countLabel({ ...base, total: 1555 })).toBe("60 loaded / 1555 photos");
  });

  it("翻完之后「共」用总数而不是已加载数", () => {
    expect(countLabel({ ...base, shown: 1500, total: 1555, hasMore: false })).toBe("1500 loaded / 1555 photos");
  });

  it("拿不到总数时保持原来的三种说法", () => {
    expect(countLabel(base)).toBe("60 loaded…");
    expect(countLabel({ ...base, truncated: true })).toBe("60 loaded (more available)");
    expect(countLabel({ ...base, hasMore: false })).toBe("60 photos");
  });

  it("区间筛选不掺总数（总数是整库的，不是区间内的）", () => {
    expect(countLabel({ ...base, shown: 12, total: 1555, filtered: true, hasMore: false })).toBe("In this range: 12 photos");
    expect(countLabel({ ...base, shown: 12, total: 1555, filtered: true })).toBe("In this range: 12 loaded…");
    expect(countLabel({ ...base, shown: 12, total: 1555, filtered: true, truncated: true })).toBe("In this range: 12 loaded (more available)");
  });

  it("精确统计进行中只写中性的进度，不展示估算值", () => {
    expect(countLabel({ ...base, shown: 12, filtered: true, exactLoading: true })).toBe("Calculating exact count… (12 loaded)");
    expect(countLabel({ ...base, shown: 0, filtered: true, exactLoading: true })).toBe("Calculating exact count… (0 loaded)");
    expect(countLabel({ ...base, shown: 12, filtered: true, exactLoading: true, hasMore: false })).toBe("Calculating exact count…");
  });

  it("精确值优先，且空区间也能显示 0 张", () => {
    expect(countLabel({ ...base, shown: 12, filtered: true, exact: 128 })).toBe("12 of 128 photos loaded in this range");
    expect(countLabel({ ...base, shown: 0, filtered: true, exact: 0, hasMore: false })).toBe("0 photos in this range");
  });
});
