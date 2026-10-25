export const APP_NAME = "Cirrus";

export const Status = {
  ok: "OK",
  error: "ERROR",
  success: "SUCCESS",
  need2fa: "NEED_2FA",
  submitted: "SUBMITTED",
  authenticated: "AUTHENTICATED",
  pending: "PENDING",
} as const;

export const StorageKey = {
  view: "cirrus.view",
  theme: "cirrus.theme",
  downloadConcurrency: "cirrus.downloadConcurrency",
  downloadTaskConcurrency: "cirrus.downloadTaskConcurrency",
  downloadSpeedLimit: "cirrus.downloadSpeedLimit",
  downloadSubfolder: "cirrus.downloadSubfolder",
  downloadDirectory: "cirrus.downloadDirectory",
  locale: "cirrus.locale",
  sidebarCollapsed: "cirrus.sidebarCollapsed",
  mapProvider: "cirrus.mapProvider",
  amapKey: "cirrus.amapKey",
  amapSecurityCode: "cirrus.amapSecurityCode",
  photosView: "cirrus.photosView",
} as const;

export type View = "findmy" | "photos-login" | "photos-gallery" | "settings";

export type ShellView = Extract<View, "findmy" | "photos-gallery">;

export type ThemeMode = "system" | "light" | "dark";
