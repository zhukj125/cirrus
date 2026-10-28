<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { ArrowLeft, ChevronDown, Clock3, Download as DownloadIcon, ExternalLink, Eye, EyeOff, HardDrive, Info, KeyRound, MapPin, Monitor, Moon, Palette, Search, Settings2, ShieldCheck, Sun } from "@lucide/vue";

import BrandMark from "@/components/layout/BrandMark.vue";
import BrandWordmark from "@/components/layout/BrandWordmark.vue";
import DownloadSettingsPanel from "@/components/photos/DownloadSettingsPanel.vue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { StorageKey, type ThemeMode } from "@/constants";
import { openExternal } from "@/lib/api";
import { DEFAULT_MAP_PROVIDER_ID, MAP_PROVIDERS, getMapProvider } from "@/lib/maps/providers";
import { readStored, writeStored } from "@/lib/common/storage";
import { toast } from "@/lib/common/toast";
import { useLocaleStore } from "@/stores/locale";
import { useThemeStore } from "@/stores/theme";

const props = withDefaults(
  defineProps<{
    version?: string;
    ttlMinutes?: number | null;
    vaultEnabled?: boolean | null;
    initialCategory?: string;
  }>(),
  {
    version: "1.3.0",
    ttlMinutes: null,
    vaultEnabled: null,
    initialCategory: "appearance",
  },
);

const emit = defineEmits<{ back: [] }>();
const theme = useThemeStore();
const locale = useLocaleStore();
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

watch(
  () => props.initialCategory,
  (value) => {
    active.value = value;
  },
);

const THEME_OPTIONS: Array<{ value: ThemeMode; labelKey: string; icon: typeof Sun }> = [
  { value: "system", labelKey: "theme.system", icon: Monitor },
  { value: "light", labelKey: "theme.light", icon: Sun },
  { value: "dark", labelKey: "theme.dark", icon: Moon },
];

const CATEGORIES = [
  { id: "appearance", labelKey: "settings.appearance", icon: Palette, keywords: "appearance theme light dark language interface" },
  { id: "maps", labelKey: "settings.maps", icon: MapPin, keywords: "map amap api key security code" },
  { id: "downloads", labelKey: "settings.downloads", icon: DownloadIcon, keywords: "downloads concurrency originals videos" },
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

function onLocaleChange(event: Event): void {
  const value = (event.target as HTMLSelectElement).value;
  if (value === "zh-CN" || value === "en-US") locale.setLocale(value);
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
      <div class="mx-auto w-full min-w-0 max-w-[min(100%,42rem)] px-6 py-7 sm:px-8">
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
                    <option value="zh-CN">{{ locale.t("settings.chinese") }}</option>
                    <option value="en-US">English</option>
                  </select>
                  <ChevronDown :size="15" class="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-muted-foreground" />
                </div>
              </div>
              <div>
                <p class="mb-2 text-sm font-medium">{{ locale.t("settings.zoom") }}</p>
                <div class="flex h-10 items-center rounded-lg border border-border bg-card px-3 text-sm text-muted-foreground">100%</div>
              </div>
            </div>
          </div>

          <div>
            <p class="mb-2 text-sm font-medium">{{ locale.t("settings.theme") }}</p>
            <div class="grid grid-cols-[repeat(auto-fit,minmax(160px,1fr))] gap-2.5">
              <button
                v-for="option in THEME_OPTIONS"
                :key="option.value"
                type="button"
                class="flex items-center justify-center gap-2 rounded-lg border px-4 py-3 text-sm transition-colors"
                :class="theme.mode === option.value ? 'border-primary bg-accent text-foreground' : 'border-border bg-card text-muted-foreground hover:bg-accent hover:text-foreground'"
                :aria-pressed="theme.mode === option.value"
                @click="theme.setMode(option.value, $event)"
              >
                <component :is="option.icon" :size="16" />
                {{ locale.t(option.labelKey) }}
              </button>
            </div>
            <p class="mt-2 text-xs text-muted-foreground">{{ locale.t("settings.themeSaved") }}</p>
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

        <section v-else-if="active === 'about'" class="flex flex-col gap-5">
          <div class="rounded-xl border border-border bg-card p-6">
            <div class="flex items-center gap-4">
              <span class="grid size-16 shrink-0 place-items-center rounded-[20px] bg-secondary text-foreground ring-1 ring-border">
                <BrandMark class="size-9" />
              </span>
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <BrandWordmark size="lg" />
                  <span class="rounded-full bg-secondary px-2 py-0.5 text-xs font-medium tabular text-muted-foreground">v{{ props.version }}</span>
                </div>
                <p class="mt-1 text-sm text-muted-foreground">{{ locale.t("settings.tagline") }}</p>
              </div>
            </div>
          </div>

          <dl class="divide-y divide-border rounded-xl border border-border bg-card px-5 text-sm">
            <div class="flex items-center gap-3 py-4">
              <Info :size="16" class="shrink-0 text-muted-foreground" />
              <dt class="text-muted-foreground">{{ locale.t("settings.version") }}</dt>
              <dd class="ml-auto tabular font-medium">v{{ props.version }}</dd>
            </div>
            <div class="flex items-center gap-3 py-4">
              <Clock3 :size="16" class="shrink-0 text-muted-foreground" />
              <dt class="text-muted-foreground">{{ locale.t("settings.sessionTtl") }}</dt>
              <dd class="ml-auto tabular font-medium">{{ props.ttlMinutes ? locale.t("settings.hours", { count: props.ttlMinutes / 60 }) : "—" }}</dd>
            </div>
            <div class="flex items-center gap-3 py-4">
              <HardDrive :size="16" class="shrink-0 text-muted-foreground" />
              <dt class="text-muted-foreground">{{ locale.t("settings.vault") }}</dt>
              <dd class="ml-auto font-medium">{{ props.vaultEnabled === null ? "—" : props.vaultEnabled ? locale.t("settings.vaultOn") : locale.t("settings.vaultOff") }}</dd>
            </div>
          </dl>
        </section>
      </div>
    </div>
  </div>
</template>
