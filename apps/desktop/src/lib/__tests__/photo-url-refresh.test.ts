import { describe, expect, it } from "vitest";

import { earliestPhotoUrlExpiryMs, photoPageStarts, photoUrlRefreshDelayMs, signedPhotoUrlExpiryMs } from "@/lib/photos/url-refresh";
import type { PhotoItem } from "@/types";

function photo(id: string, thumbUrl: string, extra: Partial<PhotoItem> = {}): PhotoItem {
  return { id, fileName: `${id}.jpeg`, takenAt: null, isVideo: false, thumbUrl, ...extra };
}

describe("photo URL refresh", () => {
  it("reads the signed expiry from iCloud media URLs", () => {
    expect(signedPhotoUrlExpiryMs("https://cvws.icloud-content.com.cn/B/a/unknown.jpeg?e=1793181126&x=1")).toBe(1_793_181_126_000);
    expect(signedPhotoUrlExpiryMs("https://example.com/photo.jpeg")).toBeNull();
    expect(signedPhotoUrlExpiryMs("not a URL")).toBeNull();
  });

  it("uses the earliest signed URL expiry in the loaded photos", () => {
    const photos = [photo("a", "https://cvws.icloud-content.com.cn/B/a?e=100"), photo("b", "https://cvws.icloud-content.com.cn/B/b?e=50", { displayUrl: "https://cvws.icloud-content.com.cn/B/c?e=75" })];
    expect(earliestPhotoUrlExpiryMs(photos)).toBe(50_000);
  });

  it("schedules a refresh before expiry and covers every loaded page", () => {
    const photos = [photo("a", "https://cvws.icloud-content.com.cn/B/a?e=1060")];
    expect(photoUrlRefreshDelayMs(photos, 1_000_000, 60_000, 5_000)).toBe(5_000);
    expect(photoPageStarts(130, 60)).toEqual([0, 60, 120]);
  });
});
