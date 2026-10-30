<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Monitor, Moon, RefreshCw, Settings as SettingsIcon, Square, Sun, X } from "@lucide/vue";

import BrandMark from "@/components/layout/BrandMark.vue";
import BrandWordmark from "@/components/layout/BrandWordmark.vue";
import { openExternal } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { useLocaleStore } from "@/stores/locale";
import { useThemeStore } from "@/stores/theme";

const props = withDefaults(
  defineProps<{
    showBrand?: boolean;
    showSettings?: boolean;
    showUpdate?: boolean;
    checkingUpdate?: boolean;
    updateAvailable?: boolean;
    settingsActive?: boolean;
    contextLabel?: string | null;
  }>(),
  {
    showBrand: true,
    showSettings: false,
    showUpdate: false,
    checkingUpdate: false,
    updateAvailable: false,
    settingsActive: false,
    contextLabel: null,
  },
);
const emit = defineEmits<{ settings: []; "check-update": [] }>();
const theme = useThemeStore();
const locale = useLocaleStore();
const themeMeta = computed(() => {
  if (theme.mode === "light") return { label: locale.t("theme.light"), icon: Sun };
  if (theme.mode === "dark") return { label: locale.t("theme.dark"), icon: Moon };
  return { label: locale.t("theme.system"), icon: Monitor };
});

function cycleTheme(event: MouseEvent): void {
  const next = theme.mode === "system" ? "light" : theme.mode === "light" ? "dark" : "system";
  theme.setMode(next, event);
}
const inTauri = isTauri();
const GITHUB_URL = "https://github.com/zhukj125/cirrus";

const maximized = ref(false);

let unlisten: (() => void) | undefined;

async function minimize(): Promise<void> {
  if (inTauri) await getCurrentWindow().minimize();
}

async function toggleMaximize(): Promise<void> {
  if (inTauri) await getCurrentWindow().toggleMaximize();
}

async function close(): Promise<void> {
  if (inTauri) await getCurrentWindow().close();
}

function openGithub(): void {
  void openExternal(GITHUB_URL).catch(() => toast.error(locale.t("titlebar.githubOpenFailed")));
}

onMounted(async () => {
  if (!inTauri) return;
  const appWindow = getCurrentWindow();
  maximized.value = await appWindow.isMaximized();

  unlisten = await appWindow.onResized(async () => {
    maximized.value = await appWindow.isMaximized();
  });
});

onUnmounted(() => unlisten?.());
</script>

<template>
  <header class="flex h-8 shrink-0 items-center gap-2 border-b border-sidebar-border bg-sidebar pr-0 pl-3 text-sidebar-foreground select-none" data-tauri-drag-region>
    <template v-if="props.showBrand">
      <BrandMark class="size-[19px] shrink-0 text-foreground" />
      <BrandWordmark class="ml-0.5" />
    </template>
    <span v-if="props.contextLabel" class="ml-1.5 flex min-w-0 items-center gap-2">
      <i class="h-3 w-px shrink-0 bg-border" />
      <span class="truncate text-xs text-muted-foreground" data-testid="titlebar-context">{{ props.contextLabel }}</span>
    </span>

    <div class="ml-auto flex h-full items-stretch">
      <button
        v-if="props.showUpdate"
        type="button"
        class="relative grid w-10 place-items-center text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-60"
        :class="props.updateAvailable ? 'text-primary' : ''"
        :disabled="props.checkingUpdate"
        :aria-label="locale.t(props.updateAvailable ? 'titlebar.updateAvailable' : 'titlebar.checkUpdates')"
        :title="locale.t(props.updateAvailable ? 'titlebar.updateAvailable' : props.checkingUpdate ? 'titlebar.checkingUpdates' : 'titlebar.checkUpdates')"
        @click="emit('check-update')"
      >
        <RefreshCw :size="14" :class="props.checkingUpdate ? 'animate-spin' : ''" />
        <span v-if="props.updateAvailable" class="absolute top-1.5 right-1.5 size-1.5 rounded-full bg-primary ring-2 ring-sidebar" />
      </button>
      <button type="button" class="grid w-10 place-items-center text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="locale.t('titlebar.github')" :title="locale.t('titlebar.github')" @click="openGithub">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path
            d="M12 .5C5.73.5.99 5.24.99 11.52c0 4.87 3.15 9 7.52 10.46.55.1.75-.24.75-.53v-2.07c-3.06.67-3.71-1.3-3.71-1.3-.5-1.28-1.22-1.62-1.22-1.62-.99-.68.08-.67.08-.67 1.1.08 1.68 1.13 1.68 1.13.97 1.66 2.54 1.18 3.16.9.1-.7.38-1.18.69-1.45-2.44-.28-5.01-1.22-5.01-5.43 0-1.2.43-2.18 1.13-2.95-.11-.28-.49-1.4.11-2.91 0 0 .92-.29 3.01 1.12a10.5 10.5 0 0 1 5.48 0c2.09-1.41 3.01-1.12 3.01-1.12.6 1.51.22 2.63.11 2.91.7.77 1.13 1.75 1.13 2.95 0 4.22-2.57 5.15-5.02 5.42.39.34.74 1.01.74 2.04v3.03c0 .29.2.64.76.53a10.52 10.52 0 0 0 7.51-10.46C23.01 5.24 18.27.5 12 .5Z"
          />
        </svg>
      </button>
      <button
        type="button"
        class="grid w-10 place-items-center text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        :title="locale.t('titlebar.changeTheme', { theme: themeMeta.label })"
        :aria-label="locale.t('titlebar.changeThemeAria', { theme: themeMeta.label })"
        @click="cycleTheme"
      >
        <Transition name="theme-icon" mode="out-in">
          <component :is="themeMeta.icon" :key="theme.mode" :size="14" />
        </Transition>
      </button>
      <button
        v-if="props.showSettings"
        type="button"
        class="grid w-10 place-items-center text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        :class="props.settingsActive ? 'bg-accent text-foreground' : ''"
        :aria-label="locale.t('titlebar.settings')"
        :title="locale.t('titlebar.settings')"
        @click="emit('settings')"
      >
        <SettingsIcon :size="14" />
      </button>
      <span class="my-2 w-px bg-border" />
      <button type="button" class="grid w-[46px] place-items-center text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="locale.t('titlebar.minimize')" :title="locale.t('titlebar.minimize')" @click="minimize">
        <Minus :size="14" />
      </button>
      <button
        type="button"
        class="grid w-[46px] place-items-center text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        :aria-label="locale.t(maximized ? 'titlebar.restore' : 'titlebar.maximize')"
        :title="locale.t(maximized ? 'titlebar.restore' : 'titlebar.maximize')"
        @click="toggleMaximize"
      >
        <Copy v-if="maximized" :size="12" />
        <Square v-else :size="12" />
      </button>
      <button type="button" class="grid w-[46px] place-items-center text-muted-foreground transition-colors hover:bg-danger-solid hover:text-white" :aria-label="locale.t('titlebar.close')" :title="locale.t('titlebar.close')" @click="close">
        <X :size="14" />
      </button>
    </div>
  </header>
</template>
