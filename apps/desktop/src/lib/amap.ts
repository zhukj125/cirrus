import { StorageKey } from "@/constants";
import { readStored } from "@/lib/common/storage";

const ENV_AMAP_KEY = String(import.meta.env.VITE_AMAP_KEY ?? "").trim();
const ENV_AMAP_SECURITY_CODE = String(import.meta.env.VITE_AMAP_SECURITY_CODE ?? "").trim();

export interface AmapConfig {
  key: string;
  securityCode: string;
}

export function getAmapConfig(): AmapConfig {
  const localKey = (readStored(StorageKey.amapKey) ?? "").trim();
  if (localKey) {
    return {
      key: localKey,
      securityCode: (readStored(StorageKey.amapSecurityCode) ?? "").trim(),
    };
  }
  return { key: ENV_AMAP_KEY, securityCode: ENV_AMAP_SECURITY_CODE };
}

export function hasEnvironmentAmapKey(): boolean {
  return ENV_AMAP_KEY.length > 0;
}

declare global {
  interface Window {
    AMap: any;
    _AMapSecurityConfig?: { securityJsCode: string };
  }
}

let amapPromise: Promise<any> | null = null;
let amapPromiseSignature: string | null = null;
let loadedSignature: string | null = null;
let amapScript: HTMLScriptElement | null = null;

function signatureOf(config: AmapConfig): string {
  return `${config.key}\u0000${config.securityCode}`;
}

function resetAmap(): void {
  amapScript?.remove();
  amapScript = null;
  amapPromise = null;
  amapPromiseSignature = null;
  loadedSignature = null;
  if (typeof window !== "undefined") {
    window._AMapSecurityConfig = undefined;
    try {
      delete window.AMap;
    } catch {}
  }
}

export function loadAmap(config: AmapConfig = getAmapConfig()): Promise<any> {
  const normalized: AmapConfig = { key: config.key.trim(), securityCode: config.securityCode.trim() };
  const signature = signatureOf(normalized);

  if (!normalized.key) {
    resetAmap();
    return Promise.reject(new Error("AMap API key is not configured"));
  }

  if (window.AMap && loadedSignature === signature) {
    return Promise.resolve(window.AMap);
  }

  if (loadedSignature !== signature) resetAmap();

  if (!amapPromise || amapPromiseSignature !== signature) {
    amapPromiseSignature = signature;
    amapPromise = new Promise((resolve, reject) => {
      if (normalized.securityCode) {
        window._AMapSecurityConfig = { securityJsCode: normalized.securityCode };
      }
      const script = document.createElement("script");
      amapScript = script;
      script.src = `https://webapi.amap.com/maps?v=2.0&key=${encodeURIComponent(normalized.key)}&plugin=AMap.Geocoder`;
      script.onload = () => {
        loadedSignature = signature;

        const ready = () => {
          if (window.AMap) resolve(window.AMap);
          else setTimeout(ready, 100);
        };
        ready();
      };
      script.onerror = () => {
        amapScript?.remove();
        amapScript = null;
        amapPromise = null;
        amapPromiseSignature = null;
        loadedSignature = null;
        reject(new Error("Could not load AMap. Check your network connection and API key allowlist."));
      };
      document.head.appendChild(script);
    });
  }
  return amapPromise;
}
