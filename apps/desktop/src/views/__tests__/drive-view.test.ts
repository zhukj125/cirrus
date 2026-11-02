import { beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import DriveView from "@/views/DriveView.vue";

const api = vi.hoisted(() => ({
  driveList: vi.fn(),
  driveDownload: vi.fn(),
  save: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  driveList: api.driveList,
  driveDownload: api.driveDownload,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ save: api.save }));

function mountDrive() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(DriveView, { props: { sessionId: "session-1" }, global: { plugins: [pinia] } });
}

const folder = {
  id: "folder-item-id",
  drivewsid: "FOLDER::com.apple.CloudDocs::ABC",
  parentId: null,
  name: "Downloads",
  kind: "folder" as const,
  zone: "com.apple.CloudDocs",
  extension: null,
  sizeBytes: null,
  dateModified: "2026-08-08T01:16:02Z",
  downloadUrl: null,
  childCount: 1,
  favorite: true,
};

const file = {
  id: "file-item-id",
  drivewsid: "FILE::com.apple.CloudDocs::DEF",
  parentId: folder.drivewsid,
  name: "archive",
  kind: "file" as const,
  zone: "com.apple.CloudDocs",
  extension: "rar",
  sizeBytes: 305_877,
  dateModified: "2024-03-19T10:27:31Z",
  downloadUrl: "https://p232-docws.icloud.com/v1/download/file-item-id",
  childCount: null,
  favorite: false,
};

describe("iCloud Drive page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.save.mockResolvedValue("C:\\tmp\\archive.rar");
    api.driveDownload.mockResolvedValue({ path: "C:\\tmp\\archive.rar", fileName: "archive.rar", sizeBytes: 305_877 });
    api.driveList
      .mockResolvedValueOnce({
        status: "OK",
        message: null,
        folderId: "root",
        items: [folder],
        nextCursor: null,
      })
      .mockResolvedValueOnce({
        status: "OK",
        message: null,
        folderId: folder.id,
        items: [file],
        nextCursor: null,
      });
  });

  it("virtualizes large folders instead of rendering every row", async () => {
    api.driveList.mockReset();
    const manyFiles = Array.from({ length: 500 }, (_, index) => ({
      ...file,
      id: `file-${index}`,
      name: `file-${index}`,
      drivewsid: `FILE::com.apple.CloudDocs::${index}`,
    }));
    api.driveList.mockResolvedValue({
      status: "OK",
      message: null,
      folderId: "root",
      items: manyFiles,
      nextCursor: null,
    });

    const wrapper = mountDrive();
    await flushPromises();

    expect(wrapper.text()).toContain("500 items");
    const renderedRows = wrapper.findAll('[data-testid="drive-item"]');
    expect(renderedRows.length).toBeGreaterThan(0);
    expect(renderedRows.length).toBeLessThan(50);
  });

  it("navigates with the item id and downloads a file", async () => {
    const wrapper = mountDrive();
    await flushPromises();

    expect(api.driveList).toHaveBeenCalledWith({ sessionId: "session-1", folderId: "root" });
    const folderButton = wrapper.findAll("button").find((button) => button.text().includes("Downloads"));
    expect(folderButton).toBeTruthy();
    await folderButton?.trigger("click");
    await flushPromises();

    expect(api.driveList).toHaveBeenLastCalledWith({ sessionId: "session-1", folderId: folder.id });
    expect(api.driveList).not.toHaveBeenCalledWith({ sessionId: "session-1", folderId: folder.drivewsid });

    const fileButton = wrapper.findAll("button").find((button) => button.text().includes("archive.rar"));
    expect(fileButton).toBeTruthy();
    await fileButton?.trigger("click");
    await flushPromises();

    expect(api.save).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: "archive.rar" }));
    expect(api.driveDownload).toHaveBeenCalledWith({
      sessionId: "session-1",
      url: file.downloadUrl,
      name: "archive.rar",
      targetPath: "C:\\tmp\\archive.rar",
    });
  });
});
