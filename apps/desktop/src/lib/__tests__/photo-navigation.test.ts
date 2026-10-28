import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import PhotosGalleryView from "@/views/PhotosGalleryView.vue";
import type { AssetDistribution, PhotoItem, PhotosTimeline, Timeline } from "@/types";

const WEEK = 604_800_000;
const DAY = 86_400_000;

const marchStart = new Date(2024, 2, 1).getTime();

const distribution: AssetDistribution = {
  bucketWidthMs: WEEK,
  total: 22,
  buckets: [
    { startMs: marchStart - WEEK, count: 5, startRank: 21, secondMonthRank: 17 },
    { startMs: marchStart, count: 7, startRank: 16, secondMonthRank: 10 },
    { startMs: marchStart + WEEK, count: 6, startRank: 9, secondMonthRank: 4 },
    { startMs: marchStart + 2 * WEEK, count: 4, startRank: 3, secondMonthRank: 0 },
  ],
};

const timeline: Timeline = {
  total: 22,
  years: [{ year: 2024, count: 22, months: [{ year: 2024, month: 2, count: 5, startRank: 21 }] }],
};

const timelineResponse: PhotosTimeline = { distribution, timeline };

const api = vi.hoisted(() => ({
  photosList: vi.fn(),
  photosTotal: vi.fn(),
  photosAssetDistribution: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  photosList: api.photosList,
  photosTotal: api.photosTotal,
  photosAssetDistribution: api.photosAssetDistribution,
  loadDownloadTasks: vi.fn(async () => ({ tasks: [], persistenceEnabled: true, writable: true, warning: null })),
  saveDownloadTasks: vi.fn(async () => undefined),
  rekeyDownloadTasks: vi.fn(async () => undefined),
  deleteDownloadTasks: vi.fn(async () => undefined),
  setDownloadSpeedLimit: vi.fn(async () => undefined),
  listenDownloadProgress: vi.fn(async () => () => undefined),
  cancelDownloadTransfer: vi.fn(async () => undefined),
}));

vi.mock("@/lib/common/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));

function photo(id: string, takenAt: number): PhotoItem {
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

function page(photos: PhotoItem[], nextCursor: string | null = null) {
  return { status: "OK" as const, message: null, photos, nextCursor };
}

function listPayload(index: number): { cursor?: string | null; startRank?: number | null } {
  return (api.photosList.mock.calls[index]?.[0] ?? {}) as { cursor?: string | null; startRank?: number | null };
}

function mountGallery() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(PhotosGalleryView, { props: { sessionId: "s1" }, global: { plugins: [pinia] } });
}

async function openTimelinePicker(wrapper: ReturnType<typeof mountGallery>): Promise<void> {
  if (wrapper.find('[role="dialog"][aria-label="Select date range"]').exists()) return;
  const trigger = wrapper.findAll("button").find((item) => item.attributes("aria-haspopup") === "dialog");
  expect(trigger, "工具条里应该有日期范围按钮").toBeTruthy();
  await trigger?.trigger("click");
  await flushPromises();
}

async function clickFebruary(wrapper: ReturnType<typeof mountGallery>): Promise<void> {
  await openTimelinePicker(wrapper);
  const button = wrapper.findAll("button").find((item) => item.attributes("title") === "Feb 2024");
  expect(button, "年月浮层里应该有「2024 年 2 月」").toBeTruthy();
  await button?.trigger("click");
  await flushPromises();
}

describe("照片页 · 年/月导航", () => {
  beforeEach(() => {
    api.photosList.mockReset();
    api.photosTotal.mockReset();
    api.photosAssetDistribution.mockReset();

    api.photosList.mockResolvedValue(page([photo("newest", marchStart + 10 * WEEK)], null));
    api.photosTotal.mockResolvedValue({ photos: 20, videos: 2, assets: 22 });
  });

  it("首屏取一次直方图；「全部」不从名次起跳，年月按需弹出", async () => {
    api.photosAssetDistribution.mockResolvedValue(timelineResponse);

    const wrapper = mountGallery();
    await flushPromises();

    expect(api.photosAssetDistribution).toHaveBeenCalledTimes(1);
    expect(api.photosAssetDistribution).toHaveBeenCalledWith("s1");

    expect(api.photosList).toHaveBeenCalled();
    expect(listPayload(0).cursor).toBeNull();
    expect(listPayload(0).startRank).toBeUndefined();

    expect(wrapper.find('[role="dialog"][aria-label="Select date range"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="library-media-counts"]').text()).toBe("20 photos · 2 videos");
    await openTimelinePicker(wrapper);
    expect(wrapper.text()).toContain("2024");
    expect(wrapper.text()).toContain("Feb");
    expect(wrapper.text()).not.toContain("approx.");
  });

  it("右键详情显示完整的拍摄元数据", async () => {
    api.photosAssetDistribution.mockResolvedValue(timelineResponse);
    const detailed = {
      ...photo("detailed", marchStart + 10 * WEEK),
      metadata: {
        cameraMake: "Apple",
        cameraModel: "iPhone 15 Pro",
        lensModel: "Main Camera",
        lensSpecification: null,
        dateTaken: null,
        exposureTime: "1/120",
        fNumber: "f/1.8",
        iso: "ISO 100",
        focalLength: "24 mm",
        focalLength35mm: "24 mm",
        exposureBias: null,
        meteringMode: "Pattern",
        flash: "Off",
        whiteBalance: "Auto",
        colorSpace: "Display P3",
        software: "17.0",
        gpsLatitude: null,
        gpsLongitude: null,
        gpsAltitude: null,
      },
      keywords: ["旅行", "杭州"],
    };
    api.photosList.mockResolvedValue(page([detailed]));

    const wrapper = mountGallery();
    await flushPromises();
    await wrapper.get('figure[data-photo-id="detailed"]').trigger("contextmenu", { clientX: 20, clientY: 20 });
    await flushPromises();
    const detailsButton = wrapper.findAll("button").find((button) => button.text().includes("Photo details"));
    expect(detailsButton).toBeTruthy();
    await detailsButton?.trigger("click");
    await flushPromises();

    const text = wrapper.text();
    expect(text).toContain("Apple iPhone 15 Pro");
    expect(text).toContain("Main Camera");
    expect(text).toContain("f/1.8");
    expect(text).toContain("ISO 100");
    expect(text).toContain("旅行");
    expect(text).toContain("杭州");
  });
  it("点某月 → 用那个月的落点名次取页；整周数据直接给出精确总数", async () => {
    api.photosAssetDistribution.mockResolvedValue(timelineResponse);
    api.photosList.mockImplementation(async (payload: { startRank?: number | null }) => {
      if (payload.startRank === 21) {
        return page([photo("f1", marchStart - 10 * DAY), photo("f2", marchStart - 2 * DAY), photo("newer", marchStart + DAY)], null);
      }
      return page([photo("other", marchStart + 10 * WEEK)], null);
    });

    const wrapper = mountGallery();
    await flushPromises();
    api.photosList.mockClear();

    await clickFebruary(wrapper);

    expect(api.photosList).toHaveBeenCalled();
    expect(listPayload(0)).toMatchObject({ sessionId: "s1", startRank: 21, limit: 60 });

    expect(wrapper.findAll("figure")).toHaveLength(2);
    expect(wrapper.text()).toContain("5 photos in this range");
    expect(wrapper.text()).not.toContain("95 photos");
    expect(wrapper.text()).not.toContain("approx.");
    expect(wrapper.text()).not.toContain("Located");
  });

  it("精确值回来后年月格与工具栏直接显示，不再出现估算", async () => {
    api.photosAssetDistribution.mockResolvedValue(timelineResponse);
    api.photosList.mockImplementation(async (payload: { startRank?: number | null }) => {
      if (payload.startRank === 21) {
        return page([photo("f1", marchStart - 3 * DAY), photo("f2", marchStart - 2 * DAY), photo("f3", marchStart - DAY), photo("newer-1", marchStart), photo("newer-2", marchStart + DAY)], null);
      }
      return page([photo("other", marchStart + 10 * WEEK)], null);
    });

    const wrapper = mountGallery();
    await flushPromises();
    await flushPromises();

    await clickFebruary(wrapper);

    expect(wrapper.text()).toContain("5 photos");
    expect(wrapper.text()).not.toContain("approx.");
    expect(wrapper.text()).toContain("5 photos in this range");
  });

  it("直方图拿不到时快捷筛选仍可用，年月区静默降级", async () => {
    api.photosAssetDistribution.mockRejectedValue(new Error("时间轴查询失败: HTTP 500"));

    const wrapper = mountGallery();
    await flushPromises();
    api.photosList.mockClear();

    expect(wrapper.find('button[aria-haspopup="dialog"]').exists()).toBe(true);
    await openTimelinePicker(wrapper);
    expect(wrapper.text()).toContain("Year and month navigation is unavailable");
    const button = wrapper.findAll("button").find((item) => item.text() === "Last 30 days");
    await button?.trigger("click");
    await flushPromises();

    expect(api.photosList).toHaveBeenCalled();
    expect(listPayload(0).cursor).toBeNull();
    expect(listPayload(0).startRank).toBeUndefined();

    expect(wrapper.text()).not.toContain("HTTP 500");
    expect(wrapper.find('button[aria-haspopup="dialog"]').exists()).toBe(true);
  });

  it("名次取页失败时完全退回游标翻页（时间轴只是加速器，不能挡住列表）", async () => {
    api.photosAssetDistribution.mockResolvedValue(timelineResponse);
    api.photosList.mockImplementation(async (payload: { startRank?: number | null }) => {
      if (typeof payload.startRank === "number") return { status: "ERROR" as const, message: "按名次取列表失败: HTTP 500", photos: [], nextCursor: null };
      return page([photo("c1", marchStart - 5 * DAY)], null);
    });

    const wrapper = mountGallery();
    await flushPromises();
    api.photosList.mockClear();

    await clickFebruary(wrapper);

    expect(listPayload(0)).toMatchObject({ startRank: 21 });
    expect(listPayload(1).cursor).toBeNull();
    expect(listPayload(1).startRank).toBeUndefined();
    expect(wrapper.findAll("figure")).toHaveLength(1);
    expect(wrapper.text()).not.toContain("Located");
  });
});
