import { defineStore } from "pinia";
import { ref } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";

import { StorageKey } from "@/constants";
import { readStored, writeStored } from "@/lib/common/storage";

const SCALE_VALUES = [0.5, 0.6, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95, 1, 1.05, 1.1, 1.15, 1.2, 1.25, 1.3, 1.4, 1.5, 1.75, 2] as const;

function normalizeScale(value: unknown): number {
  const parsed = Number(value);
  return SCALE_VALUES.includes(parsed as (typeof SCALE_VALUES)[number]) ? parsed : 1;
}

export const useInterfaceScaleStore = defineStore("interfaceScale", () => {
  const scale = ref(normalizeScale(readStored(StorageKey.interfaceScale)));
  const options = SCALE_VALUES.map((value) => ({ value, label: `${Math.round(value * 100)}%` }));

  async function apply(): Promise<void> {
    if (isTauri()) {
      try {
        await getCurrentWebview().setZoom(scale.value);
        return;
      } catch {}
    }
    document.documentElement.style.setProperty("zoom", String(scale.value));
  }

  async function setScale(value: unknown): Promise<void> {
    scale.value = normalizeScale(value);
    writeStored(StorageKey.interfaceScale, String(scale.value));
    await apply();
  }

  void apply();

  return { scale, options, apply, setScale };
});
