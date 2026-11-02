import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import CalendarView from "@/views/CalendarView.vue";

const api = vi.hoisted(() => ({
  calendarLoad: vi.fn(),
  calendarSaveEvent: vi.fn(),
  calendarDeleteEvent: vi.fn(),
}));
vi.mock("@/lib/api", () => ({
  calendarLoad: api.calendarLoad,
  calendarSaveEvent: api.calendarSaveEvent,
  calendarDeleteEvent: api.calendarDeleteEvent,
}));

function mountCalendar() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(CalendarView, { props: { sessionId: "session-1" }, global: { plugins: [pinia] } });
}

describe("日历页", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 9, 28, 12, 0, 0));
    vi.clearAllMocks();
    api.calendarSaveEvent.mockResolvedValue({
      status: "OK",
      message: null,
      event: { id: "event-2", title: "测试新日程", calendarId: "cal-1", start: "2026-10-28T09:00:00", end: "2026-10-28T10:00:00", allDay: false, recurring: false, readOnly: false, location: "", notes: "", url: "" },
    });
    api.calendarDeleteEvent.mockResolvedValue({ status: "OK", message: null, eventId: "event-1" });
    api.calendarLoad.mockResolvedValue({
      status: "OK",
      message: null,
      calendars: [{ id: "cal-1", title: "日历", color: "#cc73e1", enabled: true, readOnly: false, isDefault: true, order: 0 }],
      events: [{ id: "event-1", title: "新建日程", calendarId: "cal-1", start: "2026-10-28T09:30:00", end: "2026-10-28T10:30:00", allDay: false, recurring: false, readOnly: false, location: "会议室", notes: "测试日程", url: "" }],
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("加载月视图、日历列表和日程详情", async () => {
    const wrapper = mountCalendar();
    await flushPromises();

    expect(api.calendarLoad).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "session-1", startDate: "2026-09-26", endDate: "2026-11-01", locale: "en-US" }));
    expect(wrapper.text()).toContain("日历");
    expect(wrapper.text()).toContain("新建日程");
    expect(wrapper.text()).toContain("October 2026");

    const eventButton = wrapper.findAll("button").find((button) => button.text().includes("新建日程"));
    await eventButton?.trigger("click");
    expect(wrapper.text()).toContain("会议室");
    expect(wrapper.text()).toContain("测试日程");
  });

  it("日视图主时间轴显示一天，迷你月历显示完整月份", async () => {
    const wrapper = mountCalendar();
    await flushPromises();

    await wrapper.get('[data-testid="calendar-mode-day"]').trigger("click");
    await flushPromises();

    const miniMonthButtons = wrapper.findAll('[data-testid="mini-month"] button');
    expect(miniMonthButtons.length).toBeGreaterThanOrEqual(35);
    expect(miniMonthButtons.length).toBeLessThanOrEqual(42);
    expect(miniMonthButtons.some((button) => button.text() === "28")).toBe(true);
    expect(miniMonthButtons.filter((button) => button.text() === "1").length).toBeGreaterThanOrEqual(1);

    const selectedDay = wrapper.get('[data-testid="mini-month"] [data-date="2026-10-28"]');
    expect(selectedDay.classes()).toContain("bg-destructive");

    await wrapper.get('[data-testid="mini-month"] [data-date="2026-10-21"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="mini-month"] [data-date="2026-10-21"]').classes()).toContain("bg-destructive");
    expect(wrapper.get('[data-testid="mini-month"] [data-date="2026-10-28"]').classes()).not.toContain("bg-destructive");
    expect(wrapper.text()).toContain("October 21, 2026");
  });

  it("creates a calendar event", async () => {
    const wrapper = mountCalendar();
    await flushPromises();

    const newButton = wrapper.findAll("button").find((button) => button.text().includes("New event"));
    await newButton?.trigger("click");
    await wrapper.get('[data-testid="calendar-event-title"]').setValue("测试新日程");
    const form = wrapper.get("form");
    await form.trigger("submit");
    await flushPromises();

    expect(api.calendarSaveEvent).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: "session-1",
        event: expect.objectContaining({
          eventId: null,
          calendarId: "cal-1",
          title: "测试新日程",
          allDay: false,
        }),
      }),
    );
  });

  it("edits and deletes the selected event", async () => {
    const confirm = vi.fn(() => true);
    vi.stubGlobal("confirm", confirm);
    const wrapper = mountCalendar();
    await flushPromises();

    const eventButton = wrapper.findAll("button").find((button) => button.text().includes("新建日程"));
    await eventButton?.trigger("click");
    const editButton = wrapper.findAll("button").find((button) => button.text() === "Edit");
    await editButton?.trigger("click");
    await wrapper.get('[data-testid="calendar-event-title"]').setValue("修改后的日程");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(api.calendarSaveEvent).toHaveBeenCalledWith(
      expect.objectContaining({
        event: expect.objectContaining({ eventId: "event-1", title: "修改后的日程" }),
      }),
    );

    const eventButtonAfterSave = wrapper.findAll("button").find((button) => button.text().includes("新建日程"));
    await eventButtonAfterSave?.trigger("click");
    const deleteButton = wrapper.findAll("button").find((button) => button.text() === "Delete");
    await deleteButton?.trigger("click");
    await flushPromises();

    expect(confirm).toHaveBeenCalled();
    expect(api.calendarDeleteEvent).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: "session-1",
        eventId: "event-1",
        calendarId: "cal-1",
      }),
    );
  });
});
