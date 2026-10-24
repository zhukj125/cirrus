import { describe, expect, it } from "vitest";

import { isPastLowerBound, oldestTakenAt, passedLowerBound, passedUpperBound, scanRangePage } from "@/lib/photos/range";
import type { PhotoItem } from "@/types";

function photo(id: string, takenAt: number | null): PhotoItem {
  return {
    id,
    fileName: `${id}.jpeg`,
    takenAt,
    isVideo: false,
    thumbUrl: `https://example.com/${id}.jpeg`,
    displayUrl: null,
    originalUrl: null,
    videoUrl: null,
    isLivePhoto: false,
  };
}

const BOUNDS = { after: 10, before: 20 };

describe("区间扫描", () => {
  it("跳过比区间新的与比区间老的，只留下区间内的", () => {
    const scan = scanRangePage([photo("new", 30), photo("in1", 19), photo("in2", 10), photo("old", 9)], BOUNDS);

    expect(scan.matched.map((item) => item.id)).toEqual(["in1", "in2"]);
  });

  it("最老的一张早于下界才算翻过；正好等于下界不算", () => {
    expect(scanRangePage([photo("in", 19), photo("edge", 10)], BOUNDS).passedLowerBound).toBe(false);

    expect(scanRangePage([photo("in", 19), photo("old", 9)], BOUNDS).passedLowerBound).toBe(true);
  });

  it("跨页累计最老时间，忽略没有拍摄时间的照片", () => {
    const first = scanRangePage([photo("a", 30), photo("b", 25)], BOUNDS);
    expect(first.matched).toEqual([]);
    expect(first.passedLowerBound).toBe(false);

    const second = scanRangePage([photo("c", 18), photo("unknown", null), photo("unknown-0", 0)], BOUNDS, first.oldestTakenAt);
    expect(second.matched.map((item) => item.id)).toEqual(["c"]);

    expect(second.oldestTakenAt).toBe(18);
    expect(second.passedLowerBound).toBe(false);

    const third = scanRangePage([photo("d", 8)], BOUNDS, second.oldestTakenAt);
    expect(third.oldestTakenAt).toBe(8);
    expect(third.passedLowerBound).toBe(true);
  });

  it("不限区间时永远不算翻过下界（只能靠游标尽头收手）", () => {
    const unbounded = { after: null, before: null };
    const scan = scanRangePage([photo("a", 1)], unbounded);
    expect(scan.matched.map((item) => item.id)).toEqual(["a"]);
    expect(scan.passedLowerBound).toBe(false);
    expect(isPastLowerBound(1, unbounded)).toBe(false);
  });

  it("oldestTakenAt 取整批里最老的一个（不假设调用方已经排好序）", () => {
    expect(oldestTakenAt([photo("a", 5), photo("b", 30), photo("c", 12)])).toBe(5);
    expect(oldestTakenAt([photo("a", null), photo("b", 0)])).toBeNull();
  });

  it("passedLowerBound: 一批已翻过的照片是否已经跨过下界", () => {
    expect(passedLowerBound([photo("a", 30)], BOUNDS)).toBe(false);
    expect(passedLowerBound([photo("a", 30), photo("b", 9)], BOUNDS)).toBe(true);
    expect(passedLowerBound([], BOUNDS)).toBe(false);
  });

  it("passedUpperBound: 从区间最老端向新翻时，以上界判断收手", () => {
    expect(passedUpperBound([photo("old", 9), photo("in", 19)], BOUNDS)).toBe(false);
    expect(passedUpperBound([photo("in", 19), photo("new", 20)], BOUNDS)).toBe(true);
    expect(passedUpperBound([photo("new", 30)], BOUNDS)).toBe(true);
    expect(passedUpperBound([photo("a", 30)], { after: 10, before: null })).toBe(false);
    expect(passedUpperBound([], BOUNDS)).toBe(false);
  });
});
