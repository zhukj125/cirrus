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
  workspaceTabs: "cirrus.workspaceTabs",
} as const;

export const SHELL_VIEWS = ["findmy", "photos-gallery", "notes", "contacts", "calendar"] as const;

export type ShellView = (typeof SHELL_VIEWS)[number];

export type View = ShellView | "photos-login" | "settings";

export type WorkspaceTab = ShellView | "settings";

export function isShellView(value: string): value is ShellView {
  return (SHELL_VIEWS as readonly string[]).includes(value);
}

export type ThemeMode = "system" | "light" | "dark";
