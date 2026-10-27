import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";

import PhotoViewer from "@/components/PhotoViewer.vue";
import { openExternal } from "@/lib/api";
import type { PhotoItem } from "@/types";

vi.mock("@/lib/geocode", () => ({
  reverseGeocode: vi.fn(async () => ({ full: "Hangzhou, Zhejiang, China", short: "Hangzhou" })),
}));

vi.mock("@/lib/api", () => ({
  accountStoreLoad: vi.fn(async () => ({ accounts: [], activeAccountKey: null, avatars: {} })),
  accountStoreSave: vi.fn(async () => undefined),
  downloadPhoto: vi.fn(),
  loadDownloadTasks: vi.fn(async () => ({ tasks: [], persistenceEnabled: true, writable: true, warning: null })),
  saveDownloadTasks: vi.fn(async () => undefined),
  rekeyDownloadTasks: vi.fn(async () => undefined),
  deleteDownloadTasks: vi.fn(async () => undefined),
  setDownloadSpeedLimit: vi.fn(async () => undefined),
  listenDownloadProgress: vi.fn(async () => () => undefined),
  cancelDownloadTransfer: vi.fn(async () => undefined),
  openExternal: vi.fn(async () => undefined),
}));

const stillPhoto: PhotoItem = {
  id: "still-1",
  fileName: "still.jpeg",
  takenAt: 1_700_000_000_000,
  isVideo: false,
  thumbUrl: "https://example.com/still-thumb.jpeg",
  displayUrl: "https://example.com/still.jpeg",
  originalUrl: null,
  videoUrl: null,
  isLivePhoto: false,
  width: 1200,
  height: 800,
  sizeBytes: 2_097_152,
  addedAt: 1_710_000_000_000,
  timeZoneOffset: 28_800,
  metadata: {
    cameraMake: "Apple",
    cameraModel: "iPhone 15 Pro",
    lensModel: "Main Camera 24mm f/1.78",
    lensSpecification: null,
    dateTaken: "2024:03:25 11:59:00",
    exposureTime: "1/120 s",
    fNumber: "f/1.8",
    iso: "100",
    focalLength: "24 mm",
    focalLength35mm: "24 mm",
    exposureBias: "0 EV",
    meteringMode: "Pattern",
    flash: "No flash",
    whiteBalance: "Auto",
    colorSpace: "sRGB",
    software: "17.3.1",
    gpsLatitude: 31.2304,
    gpsLongitude: 121.4737,
    gpsAltitude: 8.4,
  },
};

const livePhoto: PhotoItem = {
  id: "live-1",
  fileName: "live.jpeg",
  takenAt: 1_700_000_000_000,
  isVideo: false,
  thumbUrl: "https://example.com/live-thumb.jpeg",
  displayUrl: "https://example.com/live.jpeg",
  originalUrl: null,
  videoUrl: "https://example.com/live.mov",
  isLivePhoto: true,
};

describe("PhotoViewer · 实况", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
  });

  it("打开实况照片时自动播放配对视频", async () => {
    const wrapper = mount(PhotoViewer, { props: { photos: [livePhoto], index: 0 }, global: { plugins: [createPinia()] } });
    await flushPromises();

    expect(wrapper.find("video").exists()).toBe(true);
    expect(wrapper.find("video").attributes("src")).toBe(livePhoto.videoUrl);
    expect(HTMLMediaElement.prototype.play).toHaveBeenCalled();
  });

  it("信息按钮会显示当前照片的完整信息", async () => {
    const wrapper = mount(PhotoViewer, { props: { photos: [stillPhoto], index: 0 }, global: { plugins: [createPinia()] } });
    await flushPromises();

    await wrapper.get('button[title="Info"]').trigger("click");
    await flushPromises();
    const panel = wrapper.get('[role="dialog"][aria-label="Info"]');
    expect(panel.text()).toContain("Apple iPhone 15 Pro");
    expect(panel.text()).toContain("Main Camera 24mm f/1.78");
    expect(panel.text()).toContain("f/1.8");
    expect(panel.text()).toContain("1/120 s");
    expect(panel.text()).toContain("31.230400, 121.473700");
    expect(panel.text()).toContain("still.jpeg");
    expect(panel.text()).toContain("1200 × 800");
    expect(panel.text()).toContain("Hangzhou, Zhejiang, China");
    expect(panel.text()).toContain("Added");
    expect(panel.text()).toContain("2.0 MB");
    expect(panel.text()).toContain("JPEG");
    await panel.get('button[title="Open in map"]').trigger("click");
    expect(openExternal).toHaveBeenCalledWith(expect.stringContaining("uri.amap.com/marker?position=121.4737,31.2304"));
  });

  it("视频信息直接使用列表数据，不请求原图", async () => {
    const video: PhotoItem = {
      ...stillPhoto,
      id: "video-1",
      fileName: "clip.mov",
      isVideo: true,
      videoUrl: "https://example.com/clip.mov",
      durationSeconds: 14.4,
      frameRate: 60,
      codec: "HEVC",
      addedAt: 1_710_000_000_000,
      metadata: { ...stillPhoto.metadata!, cameraModel: "iPhone 13", lensModel: null, lensSpecification: null },
    };
    const wrapper = mount(PhotoViewer, { props: { photos: [video], index: 0 }, global: { plugins: [createPinia()] } });
    await flushPromises();

    await wrapper.get('button[title="Info"]').trigger("click");
    await flushPromises();

    const panel = wrapper.get('[role="dialog"][aria-label="Info"]');
    expect(panel.text()).toContain("00:14");
    expect(panel.text()).toContain("60 fps");
    expect(panel.text()).toContain("Apple iPhone 13");
    expect(panel.text()).toContain("No lens information");
    expect(panel.text()).toContain("HEVC");
  });

  it("底部缩略图栏显示当前照片并可点击切换", async () => {
    const photos = Array.from({ length: 80 }, (_, index) => ({
      ...stillPhoto,
      id: `still-${index}`,
      fileName: `still-${index}.jpeg`,
      thumbUrl: `https://example.com/still-${index}-thumb.jpeg`,
    }));
    const wrapper = mount(PhotoViewer, { props: { photos, index: 0, hasMore: true }, global: { plugins: [createPinia()] } });
    await flushPromises();

    const strip = wrapper.get('div[aria-label="Photo thumbnails"]');
    expect(strip.findAll("[data-filmstrip-index]").length).toBeGreaterThan(0);
    expect(strip.findAll("[data-filmstrip-index]").length).toBeLessThan(80);
    expect(strip.get('[data-filmstrip-index="0"]').attributes("aria-current")).toBe("true");

    await strip.get('[data-filmstrip-index="1"]').trigger("click");
    expect(wrapper.emitted("update:index")?.[0]).toEqual([1]);
  });

  it("横向滚动缩略图会切换照片并预加载后续页", async () => {
    const photos = Array.from({ length: 80 }, (_, index) => ({
      ...stillPhoto,
      id: `scroll-${index}`,
      fileName: `scroll-${index}.jpeg`,
      thumbUrl: `https://example.com/scroll-${index}-thumb.jpeg`,
    }));
    const wrapper = mount(PhotoViewer, { props: { photos, index: 0, hasMore: true }, global: { plugins: [createPinia()] } });
    await flushPromises();

    const strip = wrapper.get('div[aria-label="Photo thumbnails"]');
    const stripElement = strip.element as HTMLElement;
    let scrollLeft = 3_340;
    Object.defineProperties(stripElement, {
      scrollLeft: { get: () => scrollLeft, set: (value: number) => (scrollLeft = value), configurable: true },
      clientWidth: { value: 400, configurable: true },
      scrollWidth: { value: 8_000, configurable: true },
    });
    window.dispatchEvent(new Event("resize"));
    await flushPromises();
    await new Promise((resolve) => setTimeout(resolve, 220));
    stripElement.scrollLeft = 0;
    const animationFrame = vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
      callback(0);
      return 1;
    });
    await strip.trigger("pointerdown", { button: 0, pointerId: 1, clientX: 8_000 });
    await strip.trigger("pointermove", { pointerId: 1, clientX: 200 });
    await flushPromises();
    animationFrame.mockRestore();
    await strip.trigger("pointerup", { pointerId: 1, clientX: 200 });
    await new Promise((resolve) => setTimeout(resolve, 180));
    await flushPromises();

    expect(wrapper.emitted("update:index")?.[0]).toEqual([79]);
    expect(wrapper.emitted("needMore")).toHaveLength(1);

    scrollLeft = 1_000;
    await strip.trigger("pointerdown", { button: 0, pointerId: 2, clientX: 500 });
    await strip.trigger("pointermove", { pointerId: 2, clientX: 400 });
    expect(scrollLeft).toBe(1_100);
    await strip.trigger("pointerup", { pointerId: 2, clientX: 400 });
    wrapper.unmount();
  });

  it("全库缩略图拖到未加载位置会请求对应排名", async () => {
    const photos = Array.from({ length: 5 }, (_, index) => ({
      ...stillPhoto,
      id: `rank-${100 + index}`,
      fileName: `rank-${100 + index}.jpeg`,
    }));
    const wrapper = mount(PhotoViewer, {
      props: { photos, index: 0, baseRank: 100, totalCount: 1_000, hasMore: false },
      global: { plugins: [createPinia()] },
    });
    await flushPromises();

    const strip = wrapper.get('div[aria-label="Photo thumbnails"]');
    const stripElement = strip.element as HTMLElement;
    let scrollLeft = 0;
    Object.defineProperties(stripElement, {
      scrollLeft: { get: () => scrollLeft, set: (value: number) => (scrollLeft = value), configurable: true },
      clientWidth: { value: 400, configurable: true },
      scrollWidth: { value: 40_000, configurable: true },
    });
    window.dispatchEvent(new Event("resize"));
    await flushPromises();

    const animationFrame = vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
      callback(0);
      return 1;
    });
    await strip.trigger("pointerdown", { button: 0, pointerId: 4, clientX: 8_000 });
    await strip.trigger("pointermove", { pointerId: 4, clientX: 200 });
    animationFrame.mockRestore();
    await strip.trigger("pointerup", { pointerId: 4, clientX: 200 });
    await new Promise((resolve) => setTimeout(resolve, 180));

    expect(wrapper.emitted("seekRank")?.[0]?.[0]).toBeGreaterThan(104);
    wrapper.unmount();
  });

  it("普通照片支持旋转和水平翻转", async () => {
    const wrapper = mount(PhotoViewer, { props: { photos: [stillPhoto], index: 0 }, global: { plugins: [createPinia()] } });
    await flushPromises();

    const rotateRight = wrapper.findAll("button").find((button) => button.attributes("title") === "Rotate right (])");
    const flip = wrapper.findAll("button").find((button) => button.attributes("title") === "Flip horizontally (F)");
    expect(rotateRight).toBeTruthy();
    expect(flip).toBeTruthy();

    await rotateRight?.trigger("click");
    const image = wrapper.find('img[alt="still.jpeg"]');
    expect(image.attributes("style")).toContain("rotate(90deg)");

    await flip?.trigger("click");
    expect(wrapper.find('img[alt="still.jpeg"]').attributes("style")).toContain("scaleX(-1)");
  });
});
