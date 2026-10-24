import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { nextTick } from "vue";

import { StorageKey } from "@/constants";
import { useThemeStore } from "@/stores/theme";

describe("主题切换", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    localStorage.clear();
    document.documentElement.className = "";
    setActivePinia(createPinia());
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
    document.documentElement.className = "";
  });

  it("切换深浅色时更新根节点并保存设置", async () => {
    const theme = useThemeStore();

    theme.setMode("dark", { x: 120, y: 20 });
    await nextTick();

    expect(theme.isDark).toBe(true);
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(localStorage.getItem(StorageKey.theme)).toBe("dark");

    theme.setMode("light", { x: 120, y: 20 });
    await nextTick();
    vi.advanceTimersByTime(320);

    expect(theme.isDark).toBe(false);
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(document.documentElement.classList.contains("theme-transition")).toBe(false);
  });
});
