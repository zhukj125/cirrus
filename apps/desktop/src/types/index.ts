export interface DeviceLocation {
  latitude: number | null;
  longitude: number | null;
  horizontalAccuracy: number | null;
  timeStamp: number | null;

  isOld?: boolean | null;
}

export interface Device {
  name: string | null;
  modelDisplayName: string | null;
  deviceDisplayName: string | null;
  deviceClass: string | null;
  serialNumber: string | null;
  udid: string | null;
  osVersion: string | null;
  batteryStatus: string | null;

  batteryLevel: number | null;
  location: DeviceLocation | null;

  deviceImageUrl: string | null;

  deviceId: string | null;

  activationLocked?: boolean | null;

  lostModeEnabled?: boolean | null;

  lostModePending?: boolean | null;

  locationEnabled?: boolean | null;

  deviceStatus?: string | null;
}

export interface DeviceInfoResult {
  status: "SUCCESS" | "ERROR";
  message: string | null;
  dsid: string | null;
  firstName: string | null;
  lastName: string | null;
  deviceCount: number;
  devices: Device[];
}

export interface DevicesResponse {
  sessionId: string | null;
  result: DeviceInfoResult;
}

export interface PlaySoundResult {
  success: boolean;
  message: string | null;
}

export interface AccountAvatar {
  dataUrl: string;
}

export interface AccountInfo {
  appleId: string | null;
  dsid: string | null;
  firstName: string | null;
  lastName: string | null;
}

export interface SessionStateResponse {
  valid: boolean;
  verified: boolean;
  firstName: string | null;
  lastName: string | null;

  appleId: string | null;

  dsid: string | null;
  ttlMinutes: number;
}

export interface LoadedDownloadTasks {
  tasks: unknown[];
  persistenceEnabled: boolean;

  writable: boolean;
  warning: string | null;
}

export interface DownloadProgressEvent {
  transferId: string;
  fileName: string;
  downloadedBytes: number;
  totalBytes: number | null;
}

export interface RuntimeInfo {
  ttlMinutes: number;
  sessionLimit: number;
  chinaMainlandDefault: boolean;
  version: string;

  vaultEnabled: boolean;
}

export interface PhotosLoginResult {
  status: "NEED_2FA" | "OK" | "ERROR";
  message: string | null;
  sessionId: string | null;
  authType: string | null;
  firstName: string | null;
  lastName: string | null;
}

export interface PhotoItem {
  id: string;
  fileName: string | null;
  takenAt: number | null;
  isVideo: boolean | null;

  thumbUrl: string;

  displayUrl?: string | null;

  originalUrl?: string | null;

  videoUrl?: string | null;

  isLivePhoto?: boolean | null;

  width?: number | null;

  height?: number | null;

  sizeBytes?: number | null;
}

export interface SavedPhotoResult {
  path: string;
  sizeBytes: number;
  fileName?: string;
}

export interface PhotosPage {
  status: "OK" | "ERROR";
  message: string | null;
  photos: PhotoItem[];
  nextCursor: string | null;
}

export interface LibraryTotal {
  photos: number;
  videos: number;
  assets: number;
}

export interface AssetBucket {
  startMs: number;

  count: number;

  startRank: number;

  secondMonthRank: number | null;
}

export interface AssetDistribution {
  bucketWidthMs: number;

  total: number;

  buckets: AssetBucket[];
}

export interface MonthEntry {
  year: number;

  month: number;

  count: number;

  startRank: number;
}

export interface YearEntry {
  year: number;

  count: number;

  months: MonthEntry[];
}

export interface Timeline {
  total: number;

  years: YearEntry[];
}

export interface PhotosTimeline {
  distribution: AssetDistribution;
  timeline: Timeline;
}

export interface PhotosListRequest {
  sessionId: string;

  cursor?: string | null;
  limit?: number;

  startRank?: number | null;
}

export interface Photos2faOptions {
  status: "OK" | "ERROR";
  message: string | null;
  trustedDevice: boolean;
  trustedDeviceCount: number;
  phones: PhoneOption[];
}

export interface PhoneOption {
  id: number;
  label: string;
}

export interface PhotosStatus {
  state: string;
  error?: string | null;
  authenticated: boolean;
  activated?: boolean;
  firstName?: string | null;
  lastName?: string | null;
}

export type KnownAccountStatus = "valid" | "expired" | "unknown";

export interface KnownAccount {
  key: string;
  appleId: string;
  dsid: string | null;
  firstName: string | null;
  lastName: string | null;
  photosSessionId: string | null;
  devicesSessionId: string | null;
  lastUsedAt: number;
  lastKnownStatus: KnownAccountStatus;
}

export interface AccountStoreSnapshot {
  accounts: KnownAccount[];
  activeAccountKey: string | null;
  avatars: Record<string, string>;
}
export type LoginMode = "normal" | "switch" | "relogin-expired";

export interface LoginContext {
  mode: LoginMode;

  appleId: string | null;
}
