import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";

import PhotoViewer from "@/components/PhotoViewer.vue";
import type { PhotoItem } from "@/types";

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
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue(undefined);
  });

  it("打开实况照片时自动播放配对视频", async () => {
    const wrapper = mount(PhotoViewer, { props: { photos: [livePhoto], index: 0 } });
    await flushPromises();

    expect(wrapper.find("video").exists()).toBe(true);
    expect(wrapper.find("video").attributes("src")).toBe(livePhoto.videoUrl);
    expect(HTMLMediaElement.prototype.play).toHaveBeenCalled();
  });

  it("普通照片支持旋转和水平翻转", async () => {
    const wrapper = mount(PhotoViewer, { props: { photos: [stillPhoto], index: 0 } });
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
