<script setup lang="ts">
import { computed, onMounted, onUnmounted } from "vue";
import { storeToRefs } from "pinia";
import { RefreshCw } from "@lucide/vue";
import { getCurrentWindow } from "@tauri-apps/api/window";

import AppShell from "@/components/layout/AppShell.vue";
import TitleBar from "@/components/layout/TitleBar.vue";
import DownloadManagerDialog from "@/components/photos/DownloadManagerDialog.vue";
import Spinner from "@/components/ui/Spinner.vue";
import Toaster from "@/components/ui/Toaster.vue";
import MapView from "@/views/MapView.vue";
import PhotosGalleryView from "@/views/PhotosGalleryView.vue";
import PhotosLoginView from "@/views/PhotosLoginView.vue";
import SettingsView from "@/views/SettingsView.vue";
import { useAppStore } from "@/stores/app";
import { useDownloadsStore } from "@/stores/downloads";
import { useLocaleStore } from "@/stores/locale";
import { useThemeStore } from "@/stores/theme";
import { devMockEnabled, mockDevicesResponse } from "@/dev/mock";
import type { DevicesResponse } from "@/types";
import type { ShellView } from "@/constants";

const store = useAppStore();

const downloads = useDownloadsStore();
const i18n = useLocaleStore();

useThemeStore();

if (devMockEnabled) {
  store.devices = mockDevicesResponse;
  store.view = "findmy";
  store.restoring = false;
}

const { devices, devicesError, view, photosSession, restoring, devicesLoading, accounts, activeAccountKey, accountsLoading, loginContext, pendingReloginKey, accountAvatars, runtime } = storeToRefs(store);

const currentDevices = computed<DevicesResponse | null>(() => devices.value);

const signedIn = computed(() => view.value !== "photos-login" && (photosSession.value !== null || devices.value !== null));

const shellView = computed<ShellView>(() => (view.value === "photos-gallery" ? "photos-gallery" : "findmy"));

const headerTitle = computed(() => (shellView.value === "photos-gallery" ? i18n.t("nav.photos") : i18n.t("nav.devices")));

const headerSubtitle = computed(() => {
  if (shellView.value === "photos-gallery") return i18n.t("header.photosLibrary");
  const count = devices.value?.result.deviceCount;
  return count ? i18n.t("header.deviceCount", { count }) : null;
});
const titleBarContext = computed(() => {
  if (view.value === "settings") return i18n.t("settings.title");
  if (view.value === "photos-login") return i18n.t("login.title");
  return null;
});

function onNavigate(next: ShellView): void {
  if (next === "photos-gallery") {
    void store.openPhotos();
    return;
  }
  void store.ensureDevices();
  store.goTo("findmy");
}

let settingsReturnView: "photos-login" | "findmy" | "photos-gallery" = "photos-login";

function toggleSettings(): void {
  if (view.value === "settings") {
    store.goTo(settingsReturnView);
    return;
  }
  settingsReturnView = view.value;
  store.goTo("settings");
}

let unlistenClose: (() => void) | null = null;
let closeFlushing = false;

onMounted(async () => {
  void downloads.initDownloadTracking();
  if (!devMockEnabled) void store.restore();
  try {
    const window = getCurrentWindow();
    unlistenClose = await window.onCloseRequested(async (event) => {
      if (closeFlushing) return;

      event.preventDefault();
      closeFlushing = true;
      await Promise.race([Promise.all([downloads.flushPersistence(), store.flushAccountPersistence()]), new Promise<void>((resolve) => setTimeout(resolve, 3000))]);
      await window.close();
    });
  } catch {}
});

onUnmounted(() => {
  unlistenClose?.();
  downloads.disposeDownloadTracking();
});
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <TitleBar :show-settings="true" :settings-active="view === 'settings'" :context-label="titleBarContext" @settings="toggleSettings" />

    <div class="min-h-0 flex-1">
      <div v-if="restoring" class="grid h-full place-items-center">
        <div class="flex flex-col items-center gap-2.5">
          <Spinner :size="22" />
          <p class="text-sm text-muted-foreground">{{ i18n.t("app.restoring") }}</p>
        </div>
      </div>

      <SettingsView v-else-if="view === 'settings'" :version="runtime?.version ?? '1.1.0'" :ttl-minutes="runtime?.ttlMinutes ?? null" :vault-enabled="runtime?.vaultEnabled ?? null" @back="toggleSettings" />

      <AppShell
        v-else-if="signedIn"
        :view="shellView"
        :title="headerTitle"
        :subtitle="headerSubtitle"
        :account="store.account"
        :device-count="devices?.result.deviceCount ?? null"
        :accounts="accounts"
        :active-account-key="activeAccountKey"
        :accounts-loading="accountsLoading"
        :account-avatars="accountAvatars"
        @navigate="onNavigate"
        @logout="store.logoutCurrentAccount()"
        @switch-account="store.switchAccount"
        @add-account="store.beginSwitchAccount"
        @validate-accounts="store.validateAccounts"
      >
        <template #actions>
          <button v-if="shellView === 'findmy'" type="button" class="flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50" :disabled="devicesLoading" @click="store.refreshActiveDevices()">
            <RefreshCw :size="13" :class="devicesLoading ? 'animate-spin' : ''" />
            {{ i18n.t("app.refresh") }}
          </button>
        </template>

        <PhotosGalleryView v-if="shellView === 'photos-gallery' && photosSession" :key="photosSession" :session-id="photosSession" @logout="store.logoutCurrentAccount()" />

        <MapView v-else-if="currentDevices" :data="currentDevices" @play-sound="store.playSoundOn" @open-settings="toggleSettings" />

        <div v-else class="grid h-full place-items-center">
          <div class="flex flex-col items-center gap-2.5">
            <Spinner v-if="devicesLoading" :size="22" />
            <p class="max-w-md px-4 text-center text-sm text-muted-foreground">{{ devicesLoading ? i18n.t("app.loadingDevices") : devicesError || i18n.t("app.loadDevicesFailed") }}</p>
            <div v-if="!devicesLoading" class="flex items-center gap-2">
              <button type="button" class="flex h-7 items-center gap-1.5 rounded-md border border-border px-2.5 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" @click="store.ensureDevices()">
                <RefreshCw :size="13" />
                {{ i18n.t("common.retry") }}
              </button>
              <button v-if="activeAccountKey" type="button" class="flex h-7 items-center gap-1.5 rounded-md border border-border px-2.5 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" @click="store.beginRelogin(activeAccountKey)">
                {{ i18n.t("app.verifyAgain") }}
              </button>
            </div>
          </div>
        </div>
      </AppShell>

      <PhotosLoginView
        v-else
        :accounts="accounts"
        :active-account-key="activeAccountKey"
        :avatars="accountAvatars"
        :login-context="loginContext"
        :pending-relogin-key="pendingReloginKey"
        :accounts-loading="accountsLoading"
        @success="store.acceptPhotosSession"
        @select-account="store.selectAccountFromLogin"
        @switch-other="store.beginSwitchAccount"
      />
    </div>

    <DownloadManagerDialog v-if="downloads.dialogOpen" />

    <Toaster />
  </div>
</template>
