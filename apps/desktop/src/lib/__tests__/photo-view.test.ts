import { describe, expect, it } from "vitest";

import { PHOTO_VIEW_MODES, galleryGridClass, gallerySkeletonClass, galleryTileClass, isPhotoViewMode } from "@/lib/photos/view";

describe("photo view modes", () => {
  it("offers four gallery layouts", () => {
    expect(PHOTO_VIEW_MODES.map((mode) => mode.label)).toEqual(["Comfortable", "Compact", "Dense", "List"]);
  });

  it("builds distinct layouts for grids and list view", () => {
    expect(galleryGridClass("comfortable")).toContain("minmax(172px,1fr)");
    expect(galleryGridClass("compact")).toContain("minmax(128px,1fr)");
    expect(galleryGridClass("dense")).toContain("minmax(96px,1fr)");
    expect(galleryGridClass("list")).toBe("grid-cols-1 gap-1.5");
    expect(galleryTileClass("list")).toContain("min-h-[76px]");
    expect(gallerySkeletonClass("list")).toBe("h-[76px] rounded-lg");
  });

  it("rejects unknown stored values", () => {
    expect(isPhotoViewMode("compact")).toBe(true);
    expect(isPhotoViewMode("unknown")).toBe(false);
    expect(isPhotoViewMode(null)).toBe(false);
  });
});
