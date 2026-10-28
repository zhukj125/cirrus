import type { PhotoItem } from "@/types";

const PHOTO_URL_KEYS = ["thumbUrl", "displayUrl", "originalUrl"] as const;

export function signedPhotoUrlExpiryMs(url: string | null | undefined): number | null {
  if (!url) return null;
  try {
    const value = new URL(url).searchParams.get("e");
    if (!value) return null;
    const seconds = Number(value);
    return Number.isFinite(seconds) && seconds > 0 ? seconds * 1000 : null;
  } catch {
    return null;
  }
}

export function earliestPhotoUrlExpiryMs(photos: readonly PhotoItem[]): number | null {
  let earliest: number | null = null;
  for (const photo of photos) {
    for (const key of PHOTO_URL_KEYS) {
      const expiry = signedPhotoUrlExpiryMs(photo[key]);
      if (expiry !== null && (earliest === null || expiry < earliest)) earliest = expiry;
    }
  }
  return earliest;
}

export function photoPageStarts(photoCount: number, pageSize: number): number[] {
  if (photoCount <= 0 || pageSize <= 0) return [];
  const starts: number[] = [];
  for (let rank = 0; rank < photoCount; rank += pageSize) starts.push(rank);
  return starts;
}

export function photoUrlRefreshDelayMs(photos: readonly PhotoItem[], now = Date.now(), marginMs = 60_000, minimumDelayMs = 5_000): number | null {
  const expiry = earliestPhotoUrlExpiryMs(photos);
  if (expiry === null) return null;
  return Math.max(minimumDelayMs, expiry - now - marginMs);
}
