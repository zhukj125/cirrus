import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import StorageView from "@/views/StorageView.vue";

const api = vi.hoisted(() => ({
  storageUsage: vi.fn(),
  openExternal: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  storageUsage: api.storageUsage,
  openExternal: api.openExternal,
}));

function mountStorage() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(StorageView, { props: { sessionId: "session-1" }, global: { plugins: [pinia] } });
}

describe("存储空间页", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.openExternal.mockResolvedValue(undefined);
    api.storageUsage.mockResolvedValue({
      status: "OK",
      message: null,
      storageUsageByMedia: [
        { mediaKey: "photos", displayLabel: "Photos", displayColor: "ffcc00", usageInBytes: 53_464_975_543 },
        { mediaKey: "docs", displayLabel: "Documents", displayColor: "ff9500", usageInBytes: 140_625_035 },
      ],
      usedStorageInBytes: 53_687_091_200,
      totalStorageInBytes: 53_687_091_200,
      commerceStorageInBytes: 53_687_091_200,
      compStorageInBytes: 0,
      quotaStatus: { overQuota: true, haveMaxQuotaTier: false, almostFull: true, paidQuota: true },
      manageUrl: "https://www.icloud.com/storage/",
    });
  });

  it("显示配额状态与分类用量，并支持刷新和管理", async () => {
    const wrapper = mountStorage();
    await flushPromises();

    expect(api.storageUsage).toHaveBeenCalledWith("session-1");
    expect(wrapper.text()).toContain("Your iCloud Storage");
    expect(wrapper.get('[data-testid="storage-total"]').text()).toBe("50 GB");
    expect(wrapper.text()).toContain("iCloud Storage is full");
    expect(wrapper.text()).toContain("Photos");
    expect(wrapper.text()).toContain("All photos and videos");
    expect(wrapper.text()).toContain("Documents");
    expect(wrapper.text()).toContain("All files");
    expect(wrapper.findAll('[data-testid="storage-category"]')).toHaveLength(2);

    await wrapper.get('button[aria-label="Refresh storage"]').trigger("click");
    await flushPromises();
    expect(api.storageUsage).toHaveBeenCalledTimes(2);

    const manage = wrapper.findAll("button").find((button) => button.text().includes("Manage storage"));
    await manage?.trigger("click");
    expect(api.openExternal).toHaveBeenCalledWith("https://www.icloud.com/storage/");
  });

  it("接口返回错误时显示重试入口", async () => {
    api.storageUsage.mockResolvedValueOnce({
      status: "ERROR",
      message: "Session expired",
      storageUsageByMedia: [],
      usedStorageInBytes: 0,
      totalStorageInBytes: 0,
      commerceStorageInBytes: 0,
      compStorageInBytes: 0,
      quotaStatus: { overQuota: false, haveMaxQuotaTier: false, almostFull: false, paidQuota: false },
      manageUrl: null,
    });

    const wrapper = mountStorage();
    await flushPromises();

    expect(wrapper.text()).toContain("Could not load iCloud storage");
    expect(wrapper.text()).toContain("Session expired");
    expect(wrapper.text()).toContain("Try again");
  });
});
