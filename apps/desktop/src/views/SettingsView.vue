<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { ArrowLeft, Check, ChevronDown, CircleCheck, Clock3, Download as DownloadIcon, ExternalLink, Eye, EyeOff, GitBranch, HardDrive, Images, Info, KeyRound, MapPin, NotebookPen, Palette, RefreshCw, RotateCcw, Search, Settings2, ShieldCheck } from "@lucide/vue";

import BrandMark from "@/components/layout/BrandMark.vue";
import McpIntegrationSettings from "@/components/mcp/McpIntegrationSettings.vue";
import BrandWordmark from "@/components/layout/BrandWordmark.vue";
import DownloadSettingsPanel from "@/components/photos/DownloadSettingsPanel.vue";
import { Button } from "@/components/ui/button";
import Spinner from "@/components/ui/Spinner.vue";
import { Input } from "@/components/ui/input";
import { StorageKey, themeUsesDarkBase, type ThemeMode } from "@/constants";
import { openExternal } from "@/lib/api";
import { DEFAULT_MAP_PROVIDER_ID, MAP_PROVIDERS, getMapProvider } from "@/lib/maps/providers";
import { readStored, writeStored } from "@/lib/common/storage";
import { toast } from "@/lib/common/toast";
import { LOCALE_OPTIONS, isLocale, useLocaleStore } from "@/stores/locale";
import { useInterfaceScaleStore } from "@/stores/interfaceScale";
import { useThemeStore } from "@/stores/theme";
import { useUpdaterStore } from "@/stores/updater";

const props = withDefaults(
  defineProps<{
    version?: string;
    ttlMinutes?: number | null;
    vaultEnabled?: boolean | null;
    initialCategory?: string;
  }>(),
  {
    version: "1.4.9",
    ttlMinutes: null,
    vaultEnabled: null,
    initialCategory: "appearance",
  },
);

const emit = defineEmits<{ back: [] }>();
const theme = useThemeStore();
const locale = useLocaleStore();
const interfaceScale = useInterfaceScaleStore();
const updater = useUpdaterStore();
const query = ref("");
const active = ref(props.initialCategory);
const closeBehavior = ref(readStored(StorageKey.closeBehavior) === "quit" ? "quit" : "background");

watch(closeBehavior, (value) => {
  writeStored(StorageKey.closeBehavior, value);
  writeStored(StorageKey.closeBehaviorPrompted, "1");
});

function resetClosePrompt(): void {
  writeStored(StorageKey.closeBehaviorPrompted, "0");
  toast.success(locale.t("settings.closePromptResetDone"));
}

function onScaleChange(event: Event): void {
  void interfaceScale.setScale((event.target as HTMLSelectElement).value);
}

watch(
  () => props.initialCategory,
  (value) => {
    active.value = value;
  },
);

type ThemeOption = {
  value: ThemeMode;
  labelKey: string;
  preview: string;
  accent: string;
};

const THEME_OPTIONS: ThemeOption[] = [
  { value: "light", labelKey: "theme.light", preview: "linear-gradient(135deg, #e8e8ec 0%, #fbfbfc 100%)", accent: "#0071e3" },
  { value: "dark", labelKey: "theme.dark", preview: "linear-gradient(135deg, #111114 0%, #2a2a2e 100%)", accent: "#0a84ff" },
  { value: "system", labelKey: "theme.system", preview: "linear-gradient(90deg, #f2f2f4 0 50%, #252528 50% 100%)", accent: "#0071e3" },
  {
    value: "breeze",
    labelKey: "theme.breeze",
    preview: "radial-gradient(circle at 72% 24%, rgba(255,255,255,0.92) 0 7%, transparent 24%), linear-gradient(145deg, #e8f7ee 0%, #c8e7b5 48%, #8fc077 100%)",
    accent: "#3d8b59",
  },
  {
    value: "cloud",
    labelKey: "theme.cloud",
    preview: "radial-gradient(circle at 28% 22%, rgba(255,255,255,0.94) 0 10%, transparent 28%), linear-gradient(145deg, #cbe8ff 0%, #95c8f2 52%, #e4d9bb 100%)",
    accent: "#347bb8",
  },
  {
    value: "amber",
    labelKey: "theme.amber",
    preview: "radial-gradient(circle at 74% 20%, #ffe5a1 0 8%, transparent 26%), linear-gradient(150deg, #f5ce7e 0%, #dc944d 50%, #8d4d2f 100%)",
    accent: "#b56a24",
  },
  {
    value: "blossom",
    labelKey: "theme.blossom",
    preview: "radial-gradient(circle at 18% 24%, #ffe7f2 0 8%, transparent 24%), radial-gradient(circle at 82% 72%, #d8b7ff 0 13%, transparent 32%), linear-gradient(145deg, #fff0f6 0%, #efd5f5 52%, #c9dcff 100%)",
    accent: "#b85881",
  },
  {
    value: "tide",
    labelKey: "theme.tide",
    preview: "radial-gradient(circle at 68% 28%, rgba(255,255,255,0.82) 0 5%, transparent 19%), repeating-radial-gradient(circle at 22% 62%, rgba(255,255,255,0.32) 0 2px, transparent 3px 18px), linear-gradient(145deg, #dff8ff 0%, #8bd7ee 50%, #4e9fce 100%)",
    accent: "#0c858d",
  },
  {
    value: "pine",
    labelKey: "theme.pine",
    preview: "radial-gradient(circle at 78% 16%, #c8f5d9 0 4%, transparent 17%), linear-gradient(155deg, #10231a 0%, #1b4934 52%, #0b1713 100%)",
    accent: "#65d292",
  },
];

const CATEGORIES = [
  { id: "appearance", labelKey: "settings.appearance", icon: Palette, keywords: "appearance theme light dark language interface" },
  { id: "maps", labelKey: "settings.maps", icon: MapPin, keywords: "map amap google maps api key security code" },
  { id: "downloads", labelKey: "settings.downloads", icon: DownloadIcon, keywords: "downloads concurrency originals videos" },
  { id: "mcp", labelKey: "settings.mcp", icon: Settings2, keywords: "mcp model context protocol ai agent login automation" },
  { id: "about", labelKey: "settings.about", icon: Info, keywords: "about version session cirrus" },
] as const;

const filteredCategories = computed(() => {
  const keyword = query.value.trim().toLowerCase();
  if (!keyword) return CATEGORIES;
  return CATEGORIES.filter((item) => `${locale.t(item.labelKey)} ${item.keywords}`.toLowerCase().includes(keyword));
});

watch(filteredCategories, (items) => {
  if (items.length > 0 && !items.some((item) => item.id === active.value)) active.value = items[0]?.id ?? "appearance";
});

const selectedMapProviderId = ref(getMapProvider(readStored(StorageKey.mapProvider) ?? DEFAULT_MAP_PROVIDER_ID).id);
const mapProvider = computed(() => getMapProvider(selectedMapProviderId.value));
const mapCredentialValues = ref<Record<string, string>>({});
const visibleMapCredentials = ref<Set<string>>(new Set());

function loadMapCredentialValues(): void {
  mapCredentialValues.value = Object.fromEntries(mapProvider.value.fields.map((field) => [field.id, (readStored(field.storageKey) ?? "").trim()]));
}

loadMapCredentialValues();

watch(selectedMapProviderId, (value) => {
  writeStored(StorageKey.mapProvider, value);
  visibleMapCredentials.value = new Set();
  loadMapCredentialValues();
});

const savedMapKey = computed(() => Boolean(mapCredentialValues.value[mapProvider.value.primaryFieldId]?.trim()));
const mapKeyState = computed(() => (savedMapKey.value ? "custom" : mapProvider.value.environmentConfigured?.() ? "environment" : "missing"));
const mapKeyStatus = computed(() => locale.t(mapKeyState.value === "custom" ? "settings.mapConfigured" : mapKeyState.value === "environment" ? "settings.mapEnvironment" : "settings.mapMissing"));
const mapKeyDotClass = computed(() => (mapKeyState.value === "missing" ? "bg-warning" : mapKeyState.value === "environment" ? "bg-primary" : "bg-success"));

function saveMapSettings(): void {
  for (const field of mapProvider.value.fields) {
    writeStored(field.storageKey, mapCredentialValues.value[field.id]?.trim() || null);
  }
  toast.success(locale.t(savedMapKey.value ? "settings.mapSaved" : "settings.mapCleared"));
}

function clearLocalMapCredentials(): void {
  mapCredentialValues.value = Object.fromEntries(mapProvider.value.fields.map((field) => [field.id, ""]));
  saveMapSettings();
}

function toggleMapCredential(fieldId: string): void {
  const next = new Set(visibleMapCredentials.value);
  if (next.has(fieldId)) next.delete(fieldId);
  else next.add(fieldId);
  visibleMapCredentials.value = next;
}

function openMapConsole(): void {
  void openExternal(mapProvider.value.consoleUrl).catch(() => toast.error(locale.t("settings.mapConsoleOpenFailed")));
}
const updateReleaseDate = computed(() => {
  const raw = updater.info?.date;
  if (!raw) return null;
  const date = new Date(`${raw.slice(0, 10)}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return raw;
  return new Intl.DateTimeFormat(locale.locale, { dateStyle: "medium", timeZone: "UTC" }).format(date);
});

const updateProgressLabel = computed(() => (updater.progress === null ? locale.t("updates.progressUnknown") : locale.t("updates.progress", { percent: updater.progress })));

function checkForUpdates(): void {
  void updater.check();
}

function downloadUpdate(): void {
  void updater.download();
}

function installUpdate(): void {
  void updater.install();
}

function restartAfterUpdate(): void {
  void updater.restart();
}
function openProjectGithub(): void {
  void openExternal("https://github.com/zhukj125/cirrus").catch(() => toast.error(locale.t("settings.aboutGithubOpenFailed")));
}

function onLocaleChange(event: Event): void {
  const value = (event.target as HTMLSelectElement).value;
  if (isLocale(value)) locale.setLocale(value);
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") emit("back");
}
onMounted(() => window.addEventListener("keydown", onKeydown));
onUnmounted(() => window.removeEventListener("keydown", onKeydown));
</script>

<template>
  <div class="flex h-full min-h-0">
    <aside class="flex w-48 shrink-0 flex-col border-r border-border bg-sidebar text-sidebar-foreground select-none">
      <div class="flex h-11 items-center gap-1.5 border-b border-sidebar-border px-2.5 text-sm font-semibold">
        <button type="button" class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground" :aria-label="locale.t('settings.back')" :title="locale.t('settings.back')" @click="emit('back')">
          <ArrowLeft :size="15" />
        </button>
        <Settings2 :size="15" />
        {{ locale.t("settings.title") }}
      </div>
      <nav class="flex flex-1 flex-col gap-1 overflow-y-auto p-2">
        <button
          v-for="item in filteredCategories"
          :key="item.id"
          type="button"
          class="flex h-9 w-full items-center gap-2.5 rounded-lg px-3 text-left text-sm transition-colors"
          :class="active === item.id ? 'bg-foreground font-medium text-background shadow-sm' : 'text-muted-foreground hover:bg-sidebar-accent hover:text-foreground'"
          @click="active = item.id"
        >
          <component :is="item.icon" :size="15" />
          {{ locale.t(item.labelKey) }}
        </button>
        <p v-if="filteredCategories.length === 0" class="px-3 py-2 text-xs text-muted-foreground">{{ locale.t("settings.none") }}</p>
      </nav>
    </aside>

    <div class="min-w-0 flex-1 overflow-x-hidden overflow-y-auto bg-background">
      <div :class="active === 'mcp' ? 'mx-auto w-full min-w-0 max-w-[min(100%,64rem)] px-6 py-7 sm:px-8' : 'mx-auto w-full min-w-0 max-w-[min(100%,42rem)] px-6 py-7 sm:px-8'">
        <label class="relative mb-8 block">
          <Search :size="16" class="pointer-events-none absolute top-1/2 left-4 -translate-y-1/2 text-muted-foreground" />
          <input v-model="query" type="search" class="h-10 w-full rounded-lg border border-border bg-card pr-4 pl-10 text-sm outline-none transition-colors placeholder:text-muted-foreground focus:border-ring" :placeholder="locale.t('settings.search')" />
        </label>

        <section v-if="active === 'appearance'" class="flex flex-col gap-7">
          <div>
            <h2 class="mb-5 text-lg font-semibold tracking-tight">{{ locale.t("settings.appearance") }}</h2>
            <div class="grid grid-cols-[repeat(auto-fit,minmax(210px,1fr))] gap-x-5 gap-y-5">
              <div>
                <label for="settings-language" class="mb-2 block text-sm font-medium">{{ locale.t("settings.language") }}</label>
                <div class="relative">
                  <select id="settings-language" :value="locale.locale" class="h-10 w-full appearance-none rounded-lg border border-border bg-card pr-10 pl-3 text-sm text-foreground outline-none transition-colors focus:border-ring" @change="onLocaleChange">
                    <option v-for="option in LOCALE_OPTIONS" :key="option.value" :value="option.value">{{ option.label }}</option>
                  </select>
                  <ChevronDown :size="15" class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-muted-foreground" />
                </div>
              </div>
              <div>
                <label for="settings-scale" class="mb-2 block text-sm font-medium">{{ locale.t("settings.zoom") }}</label>
                <div class="relative">
                  <select id="settings-scale" :value="String(interfaceScale.scale)" class="h-10 w-full appearance-none rounded-lg border border-border bg-card pr-10 pl-3 text-sm text-foreground outline-none transition-colors focus:border-ring" @change="onScaleChange">
                    <option v-for="option in interfaceScale.options" :key="option.value" :value="String(option.value)">{{ option.label }}</option>
                  </select>
                  <ChevronDown :size="15" class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-muted-foreground" />
                </div>
                <p class="mt-2 text-xs text-muted-foreground">{{ locale.t("settings.zoomHint") }}</p>
              </div>
            </div>
          </div>

          <div>
            <div class="mb-3 flex items-center justify-between gap-3">
              <p class="text-sm font-medium">{{ locale.t("settings.theme") }}</p>
              <span class="text-2xs font-semibold tracking-[0.08em] text-muted-foreground uppercase">{{ locale.t("settings.themeGallery") }}</span>
            </div>
            <div class="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-3">
              <button v-for="option in THEME_OPTIONS" :key="option.value" type="button" class="group min-w-0 text-left" :aria-label="locale.t(option.labelKey)" :aria-pressed="theme.mode === option.value" @click="theme.setMode(option.value, $event)">
                <div
                  class="relative aspect-[16/10] overflow-hidden rounded-xl border transition-[border-color,box-shadow,transform] duration-200 group-hover:-translate-y-0.5 group-hover:shadow-md"
                  :class="theme.mode === option.value ? 'border-foreground ring-2 ring-foreground/25 ring-offset-2 ring-offset-background' : 'border-border group-hover:border-foreground/25'"
                  :style="{ backgroundImage: option.preview }"
                >
                  <div v-if="option.value === 'system'" class="absolute inset-0 flex">
                    <div class="flex w-1/2 items-center justify-center bg-[#f2f2f4] p-3">
                      <div class="h-[72%] w-[76%] rounded-md border border-black/5 bg-white p-2 shadow-sm">
                        <span class="mb-1.5 block h-1 w-2/3 rounded-full bg-[#c8c8cf]" />
                        <span class="mb-1 block h-1 w-5/6 rounded-full bg-[#e2e2e7]" />
                        <span class="block h-1 w-1/2 rounded-full bg-[#e2e2e7]" />
                      </div>
                    </div>
                    <div class="flex w-1/2 items-center justify-center bg-[#252528] p-3">
                      <div class="h-[72%] w-[76%] rounded-md border border-white/10 bg-[#343439] p-2 shadow-sm">
                        <span class="mb-1.5 block h-1 w-2/3 rounded-full bg-white/50" />
                        <span class="mb-1 block h-1 w-5/6 rounded-full bg-white/15" />
                        <span class="block h-1 w-1/2 rounded-full bg-white/15" />
                      </div>
                    </div>
                  </div>

                  <div v-else class="absolute inset-x-[9%] top-[16%] bottom-0 overflow-hidden rounded-t-lg border shadow-[0_8px_16px_rgba(0,0,0,0.14)]" :class="themeUsesDarkBase(option.value) ? 'border-white/10 bg-[#25252a]/95' : 'border-black/5 bg-white/80'">
                    <div class="flex h-4 items-center gap-1 border-b px-2" :class="themeUsesDarkBase(option.value) ? 'border-white/10' : 'border-black/5'">
                      <span class="size-1 rounded-full bg-[#ff6159]" />
                      <span class="size-1 rounded-full bg-[#ffbd2e]" />
                      <span class="size-1 rounded-full bg-[#28c840]" />
                    </div>
                    <div class="flex h-[calc(100%-1rem)]">
                      <div class="w-[27%] border-r p-1.5" :class="themeUsesDarkBase(option.value) ? 'border-white/10 bg-white/5' : 'border-black/5 bg-black/[0.03]'">
                        <span class="mb-1.5 block h-1 w-3/4 rounded-full" :class="themeUsesDarkBase(option.value) ? 'bg-white/25' : 'bg-black/15'" />
                        <span class="mb-1 block h-1 w-full rounded-full" :class="themeUsesDarkBase(option.value) ? 'bg-white/10' : 'bg-black/10'" />
                        <span class="block h-1 w-2/3 rounded-full" :class="themeUsesDarkBase(option.value) ? 'bg-white/10' : 'bg-black/10'" />
                      </div>
                      <div class="min-w-0 flex-1 p-2">
                        <span class="mb-1.5 block h-1.5 w-1/2 rounded-full" :style="{ backgroundColor: option.accent }" />
                        <span class="mb-1 block h-1 w-5/6 rounded-full" :class="themeUsesDarkBase(option.value) ? 'bg-white/15' : 'bg-black/10'" />
                        <span class="block h-1 w-2/3 rounded-full" :class="themeUsesDarkBase(option.value) ? 'bg-white/10' : 'bg-black/10'" />
                        <div class="mt-2 grid grid-cols-3 gap-1">
                          <span class="aspect-[4/3] rounded-sm" :class="themeUsesDarkBase(option.value) ? 'bg-white/10' : 'bg-black/[0.06]'" />
                          <span class="aspect-[4/3] rounded-sm" :class="themeUsesDarkBase(option.value) ? 'bg-white/10' : 'bg-black/[0.06]'" />
                          <span class="aspect-[4/3] rounded-sm" :class="themeUsesDarkBase(option.value) ? 'bg-white/10' : 'bg-black/[0.06]'" />
                        </div>
                      </div>
                    </div>
                  </div>

                  <span v-if="theme.mode === option.value" class="absolute top-2 right-2 grid size-5 place-items-center rounded-full bg-foreground text-background shadow-sm">
                    <Check :size="12" :stroke-width="3" />
                  </span>
                </div>
                <div class="mt-2 flex items-center justify-between gap-2 px-0.5">
                  <span class="truncate text-sm" :class="theme.mode === option.value ? 'font-medium text-foreground' : 'text-muted-foreground'">{{ locale.t(option.labelKey) }}</span>
                  <span v-if="theme.mode === option.value" class="size-1.5 shrink-0 rounded-full bg-primary" />
                </div>
              </button>

              <div aria-disabled="true">
                <div class="grid aspect-[16/10] place-items-center rounded-xl border border-dashed border-border bg-secondary/35 text-xs text-muted-foreground">{{ locale.t("settings.themeComingSoon") }}</div>
                <p class="mt-2 px-0.5 text-sm text-muted-foreground">{{ locale.t("settings.themeMore") }}</p>
              </div>
            </div>
            <p class="mt-3 text-xs text-muted-foreground">{{ locale.t("settings.themeSaved") }}</p>
          </div>

          <div class="flex items-start justify-between gap-4 rounded-xl border border-border bg-card p-4">
            <div class="min-w-0">
              <p class="text-sm font-medium">{{ locale.t("settings.closeBehavior") }}</p>
              <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ locale.t("settings.closeBehaviorTip") }}</p>
            </div>
            <div class="flex shrink-0 flex-col items-end gap-1.5">
              <select v-model="closeBehavior" class="h-9 rounded-lg border border-border bg-background px-3 text-sm text-foreground outline-none transition-colors focus:border-ring" :aria-label="locale.t('settings.closeBehavior')">
                <option value="background">{{ locale.t("settings.closeBackground") }}</option>
                <option value="quit">{{ locale.t("settings.closeQuit") }}</option>
              </select>
              <button type="button" class="text-2xs text-primary transition-opacity hover:opacity-75" @click="resetClosePrompt">{{ locale.t("settings.closePromptReset") }}</button>
            </div>
          </div>
        </section>

        <section v-else-if="active === 'maps'" class="flex flex-col gap-7">
          <div>
            <h2 class="mb-2 text-lg font-semibold tracking-tight">{{ locale.t("settings.maps") }}</h2>
            <p class="text-sm leading-6 text-muted-foreground">{{ locale.t("settings.mapsDesc") }}</p>
          </div>

          <div>
            <label for="map-provider" class="mb-2 block text-sm font-medium">{{ locale.t("settings.mapProvider") }}</label>
            <div class="relative">
              <select id="map-provider" v-model="selectedMapProviderId" class="h-10 w-full appearance-none rounded-lg border border-border bg-card pr-10 pl-3 text-sm text-foreground outline-none transition-colors focus:border-ring">
                <option v-for="provider in MAP_PROVIDERS" :key="provider.id" :value="provider.id">{{ provider.name }}</option>
              </select>
              <ChevronDown :size="15" class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-muted-foreground" />
            </div>
            <p class="mt-2 text-xs leading-5 text-muted-foreground">{{ locale.t(mapProvider.descriptionKey) }}</p>
          </div>

          <div class="overflow-hidden rounded-xl border border-border bg-card">
            <div class="flex items-start justify-between gap-4 border-b border-border p-5">
              <div class="min-w-0">
                <p class="text-sm font-medium">{{ locale.t("settings.mapCredentials") }}</p>
                <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ locale.t("settings.mapCredentialsDesc", { provider: mapProvider.name }) }}</p>
              </div>
              <span class="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
                <span class="size-1.5 rounded-full" :class="mapKeyDotClass" />
                {{ mapKeyStatus }}
              </span>
            </div>

            <div class="space-y-5 p-5">
              <div v-for="field in mapProvider.fields" :key="field.id" class="space-y-2">
                <label :for="`map-credential-${field.id}`" class="flex items-center gap-2 text-sm font-medium">
                  {{ locale.t(field.labelKey) }}
                  <span v-if="field.optionalKey" class="text-xs font-normal text-muted-foreground">{{ locale.t(field.optionalKey) }}</span>
                </label>
                <div class="relative">
                  <KeyRound v-if="field.icon === 'key'" :size="15" class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-muted-foreground" />
                  <ShieldCheck v-else :size="15" class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-muted-foreground" />
                  <Input :id="`map-credential-${field.id}`" v-model="mapCredentialValues[field.id]" class="h-10 pr-10 pl-9 font-mono text-sm" :type="visibleMapCredentials.has(field.id) ? 'text' : 'password'" autocomplete="off" :placeholder="locale.t(field.placeholderKey)" />
                  <button
                    type="button"
                    class="absolute top-1/2 right-1 grid size-8 -translate-y-1/2 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                    :aria-label="locale.t(visibleMapCredentials.has(field.id) ? 'settings.credentialHide' : 'settings.credentialShow')"
                    @click="toggleMapCredential(field.id)"
                  >
                    <Eye v-if="visibleMapCredentials.has(field.id)" :size="15" />
                    <EyeOff v-else :size="15" />
                  </button>
                </div>
                <p v-for="helpKey in field.helpKeys ?? []" :key="helpKey" class="text-xs leading-5 text-muted-foreground">{{ locale.t(helpKey) }}</p>
              </div>
            </div>

            <div class="flex items-center justify-end gap-2 border-t border-border bg-muted/20 px-5 py-3">
              <Button v-if="savedMapKey" variant="ghost" size="sm" class="mr-auto" @click="clearLocalMapCredentials">{{ locale.t("settings.mapClear") }}</Button>
              <Button @click="saveMapSettings">{{ locale.t("settings.mapSave") }}</Button>
            </div>
          </div>

          <details class="group text-sm">
            <summary class="flex cursor-pointer list-none items-center gap-1.5 text-muted-foreground transition-colors hover:text-foreground">
              {{ locale.t("settings.mapHelpTitle") }}
              <ChevronDown :size="14" class="transition-transform group-open:rotate-180" />
            </summary>
            <ol class="mt-3 list-decimal space-y-1.5 pl-5 text-xs leading-5 text-muted-foreground">
              <li v-for="stepKey in mapProvider.helpStepKeys" :key="stepKey">{{ locale.t(stepKey) }}</li>
            </ol>
            <Button variant="link" size="sm" class="mt-2 h-auto px-0 text-xs" @click="openMapConsole">
              {{ locale.t("settings.mapOpenConsole") }}
              <ExternalLink :size="12" />
            </Button>
          </details>
        </section>

        <section v-else-if="active === 'downloads'" class="flex flex-col gap-6">
          <div>
            <h2 class="mb-2 text-lg font-semibold tracking-tight">{{ locale.t("settings.downloads") }}</h2>
            <p class="text-sm text-muted-foreground">{{ locale.t("settings.downloadDesc") }}</p>
          </div>
          <DownloadSettingsPanel />
        </section>

        <section v-else-if="active === 'mcp'" class="flex flex-col gap-6">
          <McpIntegrationSettings />
        </section>

        <section v-else-if="active === 'about'" class="flex flex-col gap-5">
          <div class="overflow-hidden rounded-xl border border-border bg-card">
            <div class="relative overflow-hidden border-b border-border bg-gradient-to-br from-primary/10 via-card to-card p-6">
              <div class="pointer-events-none absolute -top-24 -right-20 size-64 rounded-full bg-primary/10 blur-3xl" />
              <div class="relative flex flex-col gap-5 sm:flex-row sm:items-start">
                <span class="grid size-16 shrink-0 place-items-center rounded-[20px] bg-secondary text-foreground ring-1 ring-border">
                  <BrandMark class="size-9" />
                </span>
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center gap-2">
                    <BrandWordmark size="lg" />
                    <span class="rounded-full bg-secondary px-2 py-0.5 text-xs font-medium tabular text-muted-foreground">v{{ props.version }}</span>
                  </div>
                  <p class="mt-3 max-w-2xl text-xl font-semibold leading-8 tracking-tight text-foreground">{{ locale.t("settings.tagline") }}</p>
                  <p class="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">{{ locale.t("settings.aboutDescription") }}</p>
                </div>
              </div>

              <div class="relative mt-5 flex flex-wrap items-center gap-3">
                <span class="rounded-full border border-border/70 bg-background/70 px-2.5 py-1 text-xs text-muted-foreground">{{ locale.t("settings.aboutLocalFirst") }}</span>
                <span class="rounded-full border border-border/70 bg-background/70 px-2.5 py-1 text-xs text-muted-foreground">{{ locale.t("settings.aboutOriginalDownloads") }}</span>
                <span class="rounded-full border border-border/70 bg-background/70 px-2.5 py-1 text-xs text-muted-foreground">{{ locale.t("settings.aboutAccounts") }}</span>
                <Button class="ml-auto" variant="outline" size="sm" @click="openProjectGithub">
                  <GitBranch :size="14" />
                  {{ locale.t("settings.aboutGithub") }}
                  <ExternalLink :size="11" />
                </Button>
              </div>
            </div>

            <div class="grid gap-px bg-border sm:grid-cols-3">
              <article class="bg-card p-5">
                <span class="grid size-9 place-items-center rounded-lg bg-primary/10 text-primary"><Images :size="17" /></span>
                <h3 class="mt-3 text-sm font-semibold">{{ locale.t("settings.aboutPhotosTitle") }}</h3>
                <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ locale.t("settings.aboutPhotosDesc") }}</p>
              </article>
              <article class="bg-card p-5">
                <span class="grid size-9 place-items-center rounded-lg bg-primary/10 text-primary"><MapPin :size="17" /></span>
                <h3 class="mt-3 text-sm font-semibold">{{ locale.t("settings.aboutFindMyTitle") }}</h3>
                <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ locale.t("settings.aboutFindMyDesc") }}</p>
              </article>
              <article class="bg-card p-5">
                <span class="grid size-9 place-items-center rounded-lg bg-primary/10 text-primary"><NotebookPen :size="17" /></span>
                <h3 class="mt-3 text-sm font-semibold">{{ locale.t("settings.aboutLibraryTitle") }}</h3>
                <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ locale.t("settings.aboutLibraryDesc") }}</p>
              </article>
            </div>
          </div>

          <div class="overflow-hidden rounded-xl border border-border bg-card">
            <div class="border-b border-border px-5 py-4">
              <h3 class="text-sm font-semibold">{{ locale.t("settings.aboutRuntime") }}</h3>
            </div>
            <dl class="grid divide-y divide-border sm:grid-cols-3 sm:divide-x sm:divide-y-0">
              <div class="flex items-center gap-3 px-5 py-4">
                <Info :size="16" class="shrink-0 text-muted-foreground" />
                <div>
                  <dt class="text-xs text-muted-foreground">{{ locale.t("settings.version") }}</dt>
                  <dd class="mt-0.5 tabular text-sm font-medium">v{{ props.version }}</dd>
                </div>
              </div>
              <div class="flex items-center gap-3 px-5 py-4">
                <Clock3 :size="16" class="shrink-0 text-muted-foreground" />
                <div>
                  <dt class="text-xs text-muted-foreground">{{ locale.t("settings.sessionTtl") }}</dt>
                  <dd class="mt-0.5 tabular text-sm font-medium">{{ props.ttlMinutes ? locale.t("settings.hours", { count: props.ttlMinutes / 60 }) : "—" }}</dd>
                </div>
              </div>
              <div class="flex items-center gap-3 px-5 py-4">
                <HardDrive :size="16" class="shrink-0 text-muted-foreground" />
                <div>
                  <dt class="text-xs text-muted-foreground">{{ locale.t("settings.vault") }}</dt>
                  <dd class="mt-0.5 text-sm font-medium">{{ props.vaultEnabled === null ? "—" : props.vaultEnabled ? locale.t("settings.vaultOn") : locale.t("settings.vaultOff") }}</dd>
                </div>
              </div>
            </dl>
          </div>

          <div class="overflow-hidden rounded-xl border border-border bg-card" data-testid="app-update-card">
            <div class="flex items-start justify-between gap-4 border-b border-border px-5 py-4">
              <div class="flex min-w-0 items-start gap-3">
                <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/10 text-primary"><RefreshCw :size="17" /></span>
                <div>
                  <h3 class="text-sm font-semibold">{{ locale.t("updates.title") }}</h3>
                  <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ locale.t("updates.description") }}</p>
                </div>
              </div>
              <Button type="button" variant="outline" size="sm" class="shrink-0" :disabled="updater.checking || updater.downloading || updater.installing" @click="checkForUpdates">
                <Spinner v-if="updater.checking" :size="12" />
                <RefreshCw v-else :size="13" />
                {{ updater.checking ? locale.t("updates.checking") : locale.t("updates.check") }}
              </Button>
            </div>

            <div class="space-y-4 p-5">
              <div class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                <span
                  >{{ locale.t("settings.version") }} <span class="tabular font-medium text-foreground">v{{ props.version }}</span></span
                >
                <span v-if="updateReleaseDate">{{ locale.t("updates.releaseDate", { date: updateReleaseDate }) }}</span>
              </div>

              <div v-if="updater.error" class="rounded-lg border border-destructive/25 bg-destructive/5 px-3.5 py-3 text-xs leading-5 text-destructive" role="alert">
                {{ locale.t("updates.error", { error: updater.error }) }}
              </div>

              <div v-else-if="updater.checked && !updater.hasUpdate" class="flex items-center gap-2 rounded-lg border border-success/25 bg-success/5 px-3.5 py-3 text-sm text-success">
                <CircleCheck :size="15" />
                {{ locale.t("updates.upToDate") }}
              </div>

              <template v-if="updater.info">
                <div class="flex flex-wrap items-center gap-2">
                  <span class="rounded-full bg-primary/10 px-2.5 py-1 text-xs font-medium text-primary">{{ locale.t("updates.available") }}</span>
                  <span class="text-sm font-semibold">{{ locale.t("updates.latestVersion", { version: updater.info.version }) }}</span>
                </div>

                <div>
                  <p class="mb-1.5 text-xs font-medium text-muted-foreground">{{ locale.t("updates.notes") }}</p>
                  <div class="max-h-44 overflow-y-auto rounded-lg bg-secondary/45 px-3.5 py-3 text-xs leading-5 whitespace-pre-line text-muted-foreground">{{ updater.info.body || locale.t("updates.noNotes") }}</div>
                </div>

                <div v-if="updater.downloading" class="space-y-2">
                  <div class="flex items-center justify-between gap-3 text-xs text-muted-foreground">
                    <span>{{ updateProgressLabel }}</span>
                    <span class="tabular">{{ updater.progress === null ? "—" : `${updater.progress}%` }}</span>
                  </div>
                  <div class="h-1.5 overflow-hidden rounded-full bg-secondary">
                    <div class="h-full rounded-full bg-primary transition-[width] duration-200" :class="updater.progress === null ? 'w-1/3 animate-pulse' : ''" :style="updater.progress === null ? undefined : { width: `${updater.progress}%` }" />
                  </div>
                </div>

                <div class="flex flex-wrap items-center gap-2">
                  <Button v-if="updater.downloading" type="button" variant="outline" size="sm" @click="updater.cancelDownload()">
                    {{ locale.t("updates.cancelDownload") }}
                  </Button>
                  <Button v-else-if="updater.installed" type="button" size="sm" :disabled="updater.installing" @click="restartAfterUpdate">
                    <RotateCcw :size="13" />
                    {{ locale.t("updates.restart") }}
                  </Button>
                  <Button v-else-if="updater.downloaded" type="button" size="sm" :disabled="updater.installing" @click="installUpdate">
                    <Spinner v-if="updater.installing" :size="12" />
                    <DownloadIcon v-else :size="13" />
                    {{ updater.installing ? locale.t("updates.installing") : locale.t("updates.install") }}
                  </Button>
                  <Button v-else type="button" size="sm" @click="downloadUpdate">
                    <DownloadIcon :size="13" />
                    {{ locale.t("updates.download") }}
                  </Button>
                  <span v-if="updater.downloaded" class="text-xs text-success">{{ locale.t("updates.downloaded") }}</span>
                  <span v-if="updater.installed" class="text-xs text-success">{{ locale.t("updates.installed") }}</span>
                </div>
              </template>
            </div>
          </div>

          <div class="flex items-start gap-3 rounded-xl border border-primary/20 bg-primary/5 p-5">
            <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-primary/10 text-primary"><ShieldCheck :size="17" /></span>
            <div>
              <h3 class="text-sm font-semibold">{{ locale.t("settings.aboutPrivacyTitle") }}</h3>
              <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ locale.t("settings.aboutPrivacyNote") }}</p>
            </div>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>
