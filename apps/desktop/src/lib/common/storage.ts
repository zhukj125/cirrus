import { invoke, isTauri } from "@tauri-apps/api/core";

const DESKTOP_SETTINGS_PREFIX = "cirrus.";

export function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeStored(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {}
  if (isTauri()) {
    void invoke("desktop_setting_set", { payload: { key, value } }).catch((error) => {
      console.warn("[SETTINGS] could not persist desktop setting", key, error);
    });
  }
}

export async function hydrateDesktopSettings(): Promise<void> {
  if (!isTauri()) return;
  try {
    const local = new Map<string, string>();
    for (let index = 0; index < localStorage.length; index += 1) {
      const key = localStorage.key(index);
      if (!key?.startsWith(DESKTOP_SETTINGS_PREFIX)) continue;
      const value = localStorage.getItem(key);
      if (value !== null) local.set(key, value);
    }

    const remote = await invoke<Record<string, string>>("desktop_settings_load");
    for (const [key, value] of Object.entries(remote)) localStorage.setItem(key, value);

    const missing = Object.fromEntries([...local].filter(([key]) => !(key in remote)));
    if (Object.keys(missing).length > 0) {
      await invoke("desktop_settings_merge", { payload: missing });
    }
  } catch (error) {
    console.warn("[SETTINGS] could not restore desktop settings", error);
  }
}
