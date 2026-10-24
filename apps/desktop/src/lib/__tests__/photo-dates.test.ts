import { describe, expect, it } from "vitest";

import { addDays, filterByRange, formatDateInput, groupByDay, presetRange, startOfDay } from "@/lib/photos/dates";
import type { PhotoItem } from "@/types";

function photo(id: string, takenAt: number | null): PhotoItem {
  return {
    id,
    fileName: `${id}.HEIC`,
    takenAt,
    isVideo: false,
    thumbUrl: `https://example.com/${id}.jpeg`,
    displayUrl: null,
    originalUrl: null,
    videoUrl: null,
    isLivePhoto: false,
  };
}

const NOW = new Date(2024, 2, 20, 15, 30, 0);
const TODAY = startOfDay(NOW);

describe("日期区间", () => {
  it("今天 = [今天 00:00, 明天 00:00)", () => {
    const { after, before } = presetRange("today", NOW);
    expect(after).toBe(TODAY);
    expect(before).toBe(addDays(TODAY, 1));
  });

  it("近 7 天含今天（往前数 6 天）", () => {
    const { after, before } = presetRange("week", NOW);
    expect(after).toBe(addDays(TODAY, -6));
    expect(before).toBe(addDays(TODAY, 1));
  });

  it("近 30 天含今天（往前数 29 天）", () => {
    const { after } = presetRange("month", NOW);
    expect(after).toBe(addDays(TODAY, -29));
  });

  it("今年 = 1 月 1 日到次年 1 月 1 日", () => {
    const { after, before } = presetRange("year", NOW);
    expect(after).toBe(new Date(2024, 0, 1).getTime());
    expect(before).toBe(new Date(2025, 0, 1).getTime());
  });

  it("相邻区间首尾相接（左闭右开，不重不漏）", () => {
    const { after, before } = presetRange("today", NOW);
    const yesterday = presetRange("today", new Date(2024, 2, 19, 12));

    expect(before).toBe(addDays(after, 1));
    expect(yesterday.before).toBe(after);
  });
});

describe("区间筛选", () => {
  const inRange = photo("in", TODAY + 3600_000);
  const atLowerBound = photo("lower", TODAY);
  const atUpperBound = photo("upper", addDays(TODAY, 1));
  const tooOld = photo("old", addDays(TODAY, -1));
  const noTime = photo("none", null);
  const zeroTime = photo("zero", 0);
  const all = [inRange, atLowerBound, atUpperBound, tooOld, noTime, zeroTime];
  const bounds = { after: TODAY, before: addDays(TODAY, 1) };

  it("下界含、上界不含", () => {
    const kept = filterByRange(all, bounds).map((item) => item.id);
    expect(kept).toEqual(["in", "lower"]);
  });

  it("没有拍摄时间的照片不进有界区间", () => {
    const kept = filterByRange(all, bounds).map((item) => item.id);
    expect(kept).not.toContain("none");
    expect(kept).not.toContain("zero");
  });

  it("不筛（两端都 null）时原样返回", () => {
    expect(filterByRange(all, { after: null, before: null })).toHaveLength(all.length);
  });

  it("只给下界时不限制上界", () => {
    const kept = filterByRange(all, { after: TODAY, before: null }).map((item) => item.id);
    expect(kept).toEqual(["in", "lower", "upper"]);
  });
});

describe("按天分组", () => {
  it("倒序数组分成「今天 / 昨天 / 更早」并保持顺序", () => {
    const photos = [photo("a", TODAY + 3600_000), photo("b", TODAY + 60_000), photo("c", addDays(TODAY, -1) + 3600_000), photo("d", new Date(2023, 9, 24, 10).getTime())];
    const sections = groupByDay(photos, NOW);
    expect(sections.map((section) => section.label)).toEqual(["Today", "Yesterday", "2023 Oct 24, Tue"]);
    expect(sections[0]?.items.map((entry) => entry.photo.id)).toEqual(["a", "b"]);

    expect(sections[1]?.items[0]).toMatchObject({ index: 2 });
  });

  it("没有拍摄时间的照片单列一节并放在最后", () => {
    const photos = [photo("a", TODAY), photo("none", null)];
    const sections = groupByDay(photos, NOW);
    expect(sections.map((section) => section.label)).toEqual(["Today", "Unknown date"]);
  });

  it("同一天的照片只产生一节", () => {
    const photos = [photo("a", TODAY + 1000), photo("b", TODAY + 2000), photo("c", TODAY + 3000)];
    expect(groupByDay(photos, NOW)).toHaveLength(1);
  });
});

describe("日期输入", () => {
  it("跨夏令时加减天数仍然是「同一钟点」的日期", () => {
    const before = new Date(2024, 2, 9, 12).getTime();
    const after = addDays(before, 1);
    expect(formatDateInput(after)).toBe("2024-03-10");
  });
});
