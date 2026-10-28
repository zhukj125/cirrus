export const APP_NAME = "Cirrus";

export const Status = {
  ok: "OK",
  error: "ERROR",
  success: "SUCCESS",
  need2fa: "NEED_2FA",
  trustBrowserRequired: "TRUST_BROWSER_REQUIRED",
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
  closeBehavior: "cirrus.closeBehavior",
  closeBehaviorPrompted: "cirrus.closeBehaviorPrompted",
  mapProvider: "cirrus.mapProvider",
  amapKey: "cirrus.amapKey",
  amapSecurityCode: "cirrus.amapSecurityCode",
  photosView: "cirrus.photosView",
} as const;

export type View = "findmy" | "photos-login" | "photos-gallery" | "notes" | "settings";

export type ShellView = Extract<View, "findmy" | "photos-gallery" | "notes">;

export type ThemeMode = "system" | "light" | "dark";
