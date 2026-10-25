<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Monitor, Moon, Settings as SettingsIcon, Square, Sun, X } from "@lucide/vue";

import BrandMark from "@/components/layout/BrandMark.vue";
import BrandWordmark from "@/components/layout/BrandWordmark.vue";
import { useLocaleStore } from "@/stores/locale";
import { useThemeStore } from "@/stores/theme";

const props = withDefaults(
  defineProps<{
    showBrand?: boolean;
    showSettings?: boolean;
    settingsActive?: boolean;
    contextLabel?: string | null;
  }>(),
  {
    showBrand: true,
    showSettings: false,
    settingsActive: false,
    contextLabel: null,
  },
);
const emit = defineEmits<{ settings: [] }>();
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
