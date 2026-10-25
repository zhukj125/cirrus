import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { describe, expect, it } from "vitest";

import PhotoDatePicker from "@/components/photos/PhotoDatePicker.vue";
import { UNBOUNDED } from "@/lib/photos/dates";
import type { TimelineYearView } from "@/lib/photos/timeline";

const years: TimelineYearView[] = [
  {
    key: "2024",
    label: "2024 年",
    year: 2024,
    count: 101,
    months: [{ key: "2024-03", label: "3 月", year: 2024, month: 3, count: 95, startRank: 89, fullLabel: "2024 年 3 月" }],
  },
];

const selected = { year: 2024, month: 3 };

function mountPicker(props: Record<string, unknown> = {}) {
  return mount(PhotoDatePicker, {
    props: { activeRange: "all", customValue: UNBOUNDED, years, selected: null, loading: false, ...props },
    global: { plugins: [createPinia()] },
  });
}

describe("照片日期范围浮层", () => {
  it("同时提供快捷范围和 12 个月，默认定位最新年并请求精确统计", () => {
    const wrapper = mountPicker();
    expect(wrapper.text()).toContain("Quick select");
    expect(wrapper.text()).toContain("All");
    expect(wrapper.text()).toContain("Mar");
    expect(wrapper.text()).toContain("Dec");
    expect(wrapper.emitted("ensureYear")).toEqual([[2024]]);
  });

  it("点击快捷范围只发出范围事件", async () => {
    const wrapper = mountPicker();
    const button = wrapper.findAll("button").find((item) => item.text() === "Last 30 days");
    await button?.trigger("click");
    expect(wrapper.emitted("selectRange")).toEqual([["month"]]);
  });

  it("切到自定义日期时在同一浮层内展开日历", async () => {
    const wrapper = mountPicker();
    const tab = wrapper.findAll("button").find((item) => item.text() === "Custom date");
    await tab?.trigger("click");
    expect(wrapper.text()).toContain("Apply");
    expect(wrapper.emitted("selectRange")).toBeUndefined();
  });

  it("自定义日历选完两天后提交范围", async () => {
    const wrapper = mountPicker();
    await wrapper
      .findAll("button")
      .find((item) => item.text() === "Custom date")
      ?.trigger("click");
    const days = wrapper.findAll("button").filter((item) => /^\d{1,2}$/.test(item.text()));
    expect(days.length).toBeGreaterThan(2);
    await days[0]?.trigger("click");
    await days[1]?.trigger("click");
    await wrapper
      .findAll("button")
      .find((item) => item.text() === "Apply")
      ?.trigger("click");
    expect(wrapper.emitted("selectCustomRange")).toHaveLength(1);
  });

  it("年和月拿到精确值后写「N 张」", () => {
    const wrapper = mountPicker({ selected, exactCounts: { "2024": 101, "2024-03": 98 } });
    expect(wrapper.text()).toContain("98 photos");
    expect(wrapper.text()).not.toContain("95 photos");
    expect(wrapper.text()).toContain("101 photos");
    expect(wrapper.text()).not.toContain("approx.");
  });

  it("没拿到精确值时显示占位符，不显示估算值", () => {
    const wrapper = mountPicker({ selected });
    expect(wrapper.text()).not.toContain("95 photos");
    expect(wrapper.text()).toContain("—");
  });

  it("精确统计进行中显示「…」", () => {
    const wrapper = mountPicker({ selected, pendingCounts: { "2024": true, "2024-03": true } });
    expect(wrapper.text()).toContain("…");
    expect(wrapper.text()).not.toContain("95 photos");
  });

  it("没有时间轴时仍保留快捷范围，只在年月区降级", () => {
    const wrapper = mountPicker({ years: [] });
    expect(wrapper.text()).toContain("Last 30 days");
    expect(wrapper.text()).toContain("Year and month navigation is unavailable");
  });
});
