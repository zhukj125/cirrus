import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import DownloadFileList from "@/components/photos/DownloadFileList.vue";
import type { DownloadFileRow } from "@/lib/downloads/file-row";

function rows(count: number): DownloadFileRow[] {
  return Array.from({ length: count }, (_, index) => ({
    key: `row-${index}`,
    name: `photo-${index}.jpeg`,
    sizeBytes: 1024,
    status: "completed",
  }));
}

describe("DownloadFileList", () => {
  it("virtualizes large completed lists", async () => {
    const wrapper = mount(DownloadFileList, { props: { rows: rows(1000), height: 180, itemHeight: 36 } });

    expect(wrapper.findAll('[data-testid="download-file-row"]').length).toBeLessThan(30);
    expect(wrapper.text()).toContain("photo-0.jpeg");

    const scroller = wrapper.get('[data-testid="download-file-list"]');
    (scroller.element as HTMLElement).scrollTop = 3600;
    await scroller.trigger("scroll");

    const rendered = wrapper.findAll('[data-testid="download-file-row"]');
    expect(rendered.length).toBeLessThan(30);
    expect(rendered[0]?.text()).toContain("photo-96.jpeg");
  });
  it("emits per-file pause actions", async () => {
    const wrapper = mount(DownloadFileList, {
      props: { rows: [{ key: "a", name: "photo.jpeg", sizeBytes: 1024, status: "active", photoId: "photo-a" }] },
    });
    await wrapper.get('button[aria-label="Pause this file"]').trigger("click");
    expect(wrapper.emitted("pause")?.[0]).toEqual(["photo-a"]);
  });
});
