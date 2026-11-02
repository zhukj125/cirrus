import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

import NotesView from "@/views/NotesView.vue";

const api = vi.hoisted(() => ({
  notesList: vi.fn(),
  noteDetail: vi.fn(),
  noteCreate: vi.fn(),
  noteUpdate: vi.fn(),
  noteDelete: vi.fn(),
  openExternal: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  notesList: api.notesList,
  noteDetail: api.noteDetail,
  noteCreate: api.noteCreate,
  noteUpdate: api.noteUpdate,
  noteDelete: api.noteDelete,
  openExternal: api.openExternal,
}));

function mountNotes() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(NotesView, { props: { sessionId: "session-1" }, global: { plugins: [pinia] } });
}

describe("备忘录页", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.noteCreate.mockResolvedValue({ status: "OK", message: null, noteId: "note-2" });
    api.noteUpdate.mockResolvedValue({ status: "OK", message: null, noteId: "note-1" });
    api.noteDelete.mockResolvedValue({ status: "OK", message: null, noteId: "note-1" });
    api.notesList.mockResolvedValue({
      status: "OK",
      message: null,
      folders: [{ id: "folder-1", name: "工作" }],
      notes: [
        {
          id: "note-1",
          title: "项目计划",
          snippet: "第一阶段的交付内容",
          folderId: "folder-1",
          createdAt: 1,
          modifiedAt: 1_700_000_000_000,
          pinned: true,
          deleted: false,
          firstAttachmentUti: null,
        },
      ],
      nextCursor: null,
    });
    api.noteDetail.mockResolvedValue({
      status: "OK",
      message: null,
      note: {
        id: "note-1",
        title: "项目计划",
        body: "\uFFFC\n第一行\n第二行",
        bodyHtml: `<div class="note-content-inner"><table class="note-table"><tbody><tr><td>112</td><td></td></tr><tr><td>22</td><td></td></tr></tbody></table><figure class="note-embedded-image note-embedded-paper" data-note-attachment-id="attachment-1"><img src="data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=" alt="画板"></figure><figure class="note-embedded-image" data-note-attachment-id="attachment-2"><img src="https://cvws.icloud-content.com.cn/original.png" alt="原图"></figure><h2 class="note-title">第一行</h2><div class="note-list-item note-checklist"><span class="note-checkbox"></span><span class="note-list-text">第二行</span></div></div>`,
        folderId: "folder-1",
        createdAt: 1,
        modifiedAt: 1_700_000_000_000,
        attachments: [
          {
            id: "attachment-1",
            name: "画板",
            uti: "com.apple.paper",
            previewUrl: "https://cvws.icloud-content.com.cn/light.png",
            darkPreviewUrl: "https://cvws.icloud-content.com.cn/dark.png",
            fallbackUrl: "https://cvws.icloud-content.com.cn/fallback.png",
            downloadUrl: "https://cvws.icloud-content.com.cn/drawing-preview.png",
            renderUrl: "data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=",
            table: null,
            fileSize: null,
            width: 768,
            height: 852,
          },
          {
            id: "attachment-2",
            name: "原图",
            uti: "public.png",
            previewUrl: "https://cvws.icloud-content.com.cn/preview.png",
            darkPreviewUrl: null,
            fallbackUrl: null,
            downloadUrl: "https://cvws.icloud-content.com.cn/original.png",
            renderUrl: null,
            table: null,
            fileSize: 5_629_697,
            width: 2303,
            height: 2976,
          },
          {
            id: "attachment-3",
            name: "表格",
            uti: "com.apple.notes.table",
            previewUrl: null,
            darkPreviewUrl: null,
            fallbackUrl: null,
            downloadUrl: null,
            renderUrl: null,
            table: [
              ["112", ""],
              ["22", ""],
            ],
            fileSize: null,
            width: null,
            height: null,
          },
        ],
      },
    });
  });

  it("加载文件夹和列表，优先使用矢量画板与图片原图", async () => {
    const wrapper = mountNotes();
    await flushPromises();

    expect(api.notesList).toHaveBeenCalledWith({ sessionId: "session-1", cursor: null });
    expect(wrapper.text()).toContain("项目计划");
    expect(wrapper.text()).toContain("工作");

    const noteButton = wrapper.findAll("button").find((button) => button.text().includes("项目计划"));
    expect(noteButton).toBeTruthy();
    await noteButton?.trigger("click");
    await flushPromises();

    expect(api.noteDetail).toHaveBeenCalledWith({ sessionId: "session-1", noteId: "note-1" });
    expect(wrapper.text()).toContain("第一行");
    expect(wrapper.text()).toContain("第二行");
    expect(wrapper.text()).not.toContain("\uFFFC");
    expect(wrapper.get(".note-content table").exists()).toBe(true);
    expect(wrapper.text()).toContain("2023");
    expect(wrapper.get('img[alt="画板"]').attributes("src")).toBe("data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=");
    expect(wrapper.get('img[alt="原图"]').attributes("src")).toBe("https://cvws.icloud-content.com.cn/original.png");

    expect(wrapper.findAll("button").some((button) => button.text().includes("表格"))).toBe(false);

    await wrapper.get('figure[data-note-attachment-id="attachment-1"]').trigger("click");
    expect(wrapper.get('[role="dialog"]').exists()).toBe(true);
    expect(wrapper.get('[role="dialog"] img[alt="画板"]').attributes("src")).toBe("data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=");
  });

  it("creates a note with the selected folder", async () => {
    api.noteDetail.mockResolvedValue({
      status: "OK",
      message: null,
      note: {
        id: "note-2",
        title: "新备忘录",
        body: "正文",
        bodyHtml: "",
        folderId: "DefaultFolder-CloudKit",
        createdAt: 1,
        modifiedAt: 2,
        attachments: [],
      },
    });

    const wrapper = mountNotes();
    await flushPromises();
    await wrapper.get('button[aria-label="New note"]').trigger("click");
    await wrapper.get('[data-testid="note-title-input"]').setValue("新备忘录");
    await wrapper.get('[data-testid="note-body-input"]').setValue("正文");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(api.noteCreate).toHaveBeenCalledWith({
      sessionId: "session-1",
      folderId: null,
      title: "新备忘录",
      body: "正文",
    });
  });

  afterEach(() => vi.unstubAllGlobals());

  it("edits text around inline images and preserves image positions", async () => {
    api.noteDetail.mockResolvedValue({
      status: "OK",
      message: null,
      note: {
        id: "note-1",
        title: "项目计划",
        body: "图片前\uFFFC中间\uFFFC图片后",
        bodyHtml: "",
        folderId: "folder-1",
        createdAt: 1,
        modifiedAt: 1_700_000_000_000,
        attachments: [
          {
            id: "attachment-1",
            name: "示意图.png",
            uti: "public.png",
            previewUrl: "https://cvws.icloud-content.com.cn/preview.png",
            darkPreviewUrl: null,
            fallbackUrl: null,
            downloadUrl: "https://cvws.icloud-content.com.cn/original.png",
            renderUrl: null,
            table: null,
            fileSize: 1024,
            width: 100,
            height: 100,
          },
          {
            id: "attachment-2",
            name: "示意图2.png",
            uti: "public.jpeg",
            previewUrl: "https://cvws.icloud-content.com.cn/preview-2.jpg",
            darkPreviewUrl: null,
            fallbackUrl: null,
            downloadUrl: "https://cvws.icloud-content.com.cn/original-2.jpg",
            renderUrl: null,
            table: null,
            fileSize: 2048,
            width: 120,
            height: 90,
          },
        ],
      },
    });
    const wrapper = mountNotes();
    await flushPromises();

    const noteButton = wrapper.findAll("button").find((button) => button.text().includes("项目计划"));
    await noteButton?.trigger("click");
    await flushPromises();
    await wrapper.get('button[aria-label="Edit note"]').trigger("click");
    await flushPromises();

    const editors = wrapper.findAll<HTMLTextAreaElement>('[data-testid="note-body-input"]');
    expect(editors).toHaveLength(3);
    expect(editors.map((editor) => editor.element.value)).toEqual(["图片前", "中间", "图片后"]);
    expect(wrapper.findAll(".note-draft-body img").map((image) => image.attributes("src"))).toEqual(["https://cvws.icloud-content.com.cn/original.png", "https://cvws.icloud-content.com.cn/original-2.jpg"]);

    await editors[0]?.setValue("修改前");
    await editors[1]?.setValue("修改中");
    await editors[2]?.setValue("修改后");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(api.noteUpdate).toHaveBeenCalledWith({
      sessionId: "session-1",
      noteId: "note-1",
      title: "项目计划",
      body: "修改前\uFFFC修改中\uFFFC修改后",
    });
  });

  it("updates and deletes the selected note", async () => {
    const confirm = vi.fn(() => true);
    vi.stubGlobal("confirm", confirm);
    api.noteDetail.mockResolvedValue({
      status: "OK",
      message: null,
      note: {
        id: "note-1",
        title: "项目计划",
        body: "第一行\n第二行",
        bodyHtml: "",
        folderId: "folder-1",
        createdAt: 1,
        modifiedAt: 1_700_000_000_000,
        attachments: [],
      },
    });
    const wrapper = mountNotes();
    await flushPromises();

    const noteButton = wrapper.findAll("button").find((button) => button.text().includes("项目计划"));
    await noteButton?.trigger("click");
    await flushPromises();
    await wrapper.get('button[aria-label="Edit note"]').trigger("click");
    await wrapper.get('[data-testid="note-title-input"]').setValue("更新后的标题");
    await wrapper.get('[data-testid="note-body-input"]').setValue("更新后的正文");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(api.noteUpdate).toHaveBeenCalledWith({
      sessionId: "session-1",
      noteId: "note-1",
      title: "更新后的标题",
      body: "更新后的正文",
    });

    await wrapper.get('button[aria-label="Delete note"]').trigger("click");
    await flushPromises();
    expect(confirm).toHaveBeenCalled();
    expect(api.noteDelete).toHaveBeenCalledWith({ sessionId: "session-1", noteId: "note-1" });
  });
});
