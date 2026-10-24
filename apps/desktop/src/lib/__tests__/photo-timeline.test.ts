import { describe, expect, it } from "vitest";

import { monthLabel, timelineSegment, timelineYearsDesc, yearLabel, yearMonthLabel } from "@/lib/photos/timeline";
import type { Timeline } from "@/types";

const timeline: Timeline = {
  total: 26,
  years: [
    { year: 2023, count: 6, months: [{ year: 2023, month: 12, count: 6, startRank: 0 }] },
    {
      year: 2024,
      count: 20,
      months: [
        { year: 2024, month: 1, count: 14, startRank: 6 },
        { year: 2024, month: 3, count: 6, startRank: 20 },
      ],
    },
  ],
};

describe("时间轴年月文案", () => {
  it("年、月与年月写成中文", () => {
    expect(yearLabel(2024)).toBe("2024");
    expect(monthLabel(3)).toBe("Mar");
    expect(yearMonthLabel(2024, 3)).toBe("Mar 2024");
    expect(yearMonthLabel(2024, 12)).toBe("Dec 2024");
  });
});

describe("侧栏用的年/月树", () => {
  it("年与月都从新到旧（网格也是从新到旧，方向必须一致）", () => {
    const years = timelineYearsDesc(timeline);
    expect(years.map((year) => year.label)).toEqual(["2024", "2023"]);
    expect(years[0]?.months.map((month) => month.label)).toEqual(["Mar", "Jan"]);
  });

  it("每一项都带着张数与落点名次（月键补零，便于当 Map 的键用）", () => {
    const years = timelineYearsDesc(timeline);
    expect(years[0]).toMatchObject({ key: "2024", year: 2024, count: 20 });
    expect(years[0]?.months[0]).toEqual({ key: "2024-03", label: "Mar", year: 2024, month: 3, count: 6, startRank: 20, fullLabel: "Mar 2024" });
    expect(years[1]?.months[0]?.key).toBe("2023-12");
  });

  it("没有时间轴时是空数组（侧栏据此整列都不渲染）", () => {
    expect(timelineYearsDesc(null)).toEqual([]);
    expect(timelineYearsDesc({ total: 0, years: [] })).toEqual([]);
  });
});

describe("后端年月视图查表（timelineSegment）", () => {
  it("月: 张数与锚点都取那一个月自己的", () => {
    expect(timelineSegment(timeline, { year: 2024, month: 1 })).toEqual({ count: 14, startRank: 6 });
  });

  it("年: 锚点取该年**最新那个月**的（落在年末才好往老翻完整年）", () => {
    expect(timelineSegment(timeline, { year: 2024, month: null })).toEqual({ count: 20, startRank: 20 });
  });

  it("查不到（没这个月 / 没时间轴）返回 null", () => {
    expect(timelineSegment(timeline, { year: 2024, month: 2 })).toBeNull();
    expect(timelineSegment(timeline, { year: 1999, month: null })).toBeNull();
    expect(timelineSegment(null, { year: 2024, month: 1 })).toBeNull();
  });
});
