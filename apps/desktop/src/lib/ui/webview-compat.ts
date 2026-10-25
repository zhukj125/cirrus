export type WebViewCompatTier = "ok" | "degraded" | "fatal";

export interface WebViewCompatReport {
  tier: WebViewCompatTier;
  missing: string[];
  engine: string;
  platform: string;
  desktop: boolean;
  bypassed: boolean;
}

declare global {
  interface Window {
    __CIRRUS_WEBVIEW_COMPAT__?: WebViewCompatReport;
    __CIRRUS_COMPAT_BLOCKED__?: boolean;
  }
}

export function webViewCompatReport(): WebViewCompatReport | undefined {
  if (typeof window === "undefined") return undefined;
  const report = window.__CIRRUS_WEBVIEW_COMPAT__;
  if (!report || !["ok", "degraded", "fatal"].includes(report.tier) || !Array.isArray(report.missing)) return undefined;
  return report;
}

export function isBlockingCompatFailure(): boolean {
  const report = webViewCompatReport();
  return report?.tier === "fatal" && report.bypassed !== true;
}
