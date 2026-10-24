import { defineStore } from "pinia";
import { computed, onScopeDispose, ref, watch } from "vue";

import { StorageKey, type ThemeMode } from "@/constants";
import { readStored, writeStored } from "@/lib/common/storage";

const MEDIA_QUERY = "(prefers-color-scheme: dark)";
const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

type ThemeOrigin = { x: number; y: number };
type ThemeChangeSource = MouseEvent | ThemeOrigin | undefined;

function isThemeMode(value: string | null): value is ThemeMode {
  return value === "system" || value === "light" || value === "dark";
}

export const useThemeStore = defineStore("theme", () => {
  const stored = readStored(StorageKey.theme);
  const mode = ref<ThemeMode>(isThemeMode(stored) ? stored : "system");

  const media = typeof window.matchMedia === "function" ? window.matchMedia(MEDIA_QUERY) : null;

  const systemDark = ref(media?.matches ?? false);

  const isDark = computed(() => mode.value === "dark" || (mode.value === "system" && systemDark.value));

  let pendingOrigin: ThemeOrigin | null = null;
  let initialized = false;
  let fallbackTimer: number | null = null;
  let viewTransitionActive = false;

  function defaultOrigin(): ThemeOrigin {
    return { x: Math.max(24, window.innerWidth - 48), y: 24 };
  }

  function originOf(source: ThemeChangeSource): ThemeOrigin {
    if (!source) return defaultOrigin();
    if ("clientX" in source) return { x: source.clientX, y: source.clientY };
    return source;
  }

  function applyClass(): void {
    document.documentElement.classList.toggle("dark", isDark.value);
  }

  function apply(animate: boolean, origin: ThemeOrigin): void {
    const root = document.documentElement;
    const reducedMotion = typeof window.matchMedia === "function" && window.matchMedia(REDUCED_MOTION_QUERY).matches;
    if (!animate || reducedMotion) {
      root.classList.remove("theme-transition", "theme-view-transition");
      applyClass();
      return;
    }

    const x = Math.min(Math.max(origin.x, 0), window.innerWidth);
    const y = Math.min(Math.max(origin.y, 0), window.innerHeight);
    const radius = Math.hypot(Math.max(x, window.innerWidth - x), Math.max(y, window.innerHeight - y)) + 1;
    root.style.setProperty("--theme-reveal-x", `${x}px`);
    root.style.setProperty("--theme-reveal-y", `${y}px`);
    root.style.setProperty("--theme-reveal-radius", `${radius}px`);

    if (typeof document.startViewTransition === "function" && !viewTransitionActive) {
      viewTransitionActive = true;
      root.classList.add("theme-view-transition");
      try {
        const transition = document.startViewTransition(applyClass);
        void transition.finished.finally(() => {
          viewTransitionActive = false;
          root.classList.remove("theme-view-transition");
        });
        return;
      } catch {
        viewTransitionActive = false;
        root.classList.remove("theme-view-transition");
      }
    }

    if (fallbackTimer !== null) window.clearTimeout(fallbackTimer);
    root.classList.add("theme-transition");
    applyClass();
    fallbackTimer = window.setTimeout(() => {
      fallbackTimer = null;
      root.classList.remove("theme-transition");
    }, 320);
  }

  function setMode(next: ThemeMode, source?: ThemeChangeSource): void {
    if (next === mode.value) return;
    pendingOrigin = originOf(source);
    mode.value = next;
  }

  const onSystemChange = (event: MediaQueryListEvent) => {
    systemDark.value = event.matches;
  };
  media?.addEventListener("change", onSystemChange);
  onScopeDispose(() => {
    media?.removeEventListener("change", onSystemChange);
    if (fallbackTimer !== null) window.clearTimeout(fallbackTimer);
  });

  watch([mode, systemDark], () => {
    const origin = pendingOrigin ?? defaultOrigin();
    pendingOrigin = null;
    writeStored(StorageKey.theme, mode.value);
    apply(initialized, origin);
    initialized = true;
  });

  apply(false, defaultOrigin());
  initialized = true;

  return { mode, systemDark, isDark, setMode };
});
