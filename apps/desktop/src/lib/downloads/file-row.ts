export interface DownloadFileRow {
  key: string;
  name: string;
  sizeBytes: number | null;
  downloadedBytes?: number;
  totalBytes?: number | null;
  status: "active" | "pending" | "paused" | "completed" | "failed";
  photoId?: string;
  ordinal?: number;
}
