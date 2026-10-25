<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { storeToRefs } from "pinia";
import { RefreshCw } from "@lucide/vue";
import { defaultWindowIcon } from "@tauri-apps/api/app";
import { isTauri } from "@tauri-apps/api/core";
import { Menu } from "@tauri-apps/api/menu";
import { TrayIcon } from "@tauri-apps/api/tray";
import { getCurrentWindow } from "@tauri-apps/api/window";

import AppShell from "@/components/layout/AppShell.vue";
import TitleBar from "@/components/layout/TitleBar.vue";
import DownloadManagerDialog from "@/components/photos/DownloadManagerDialog.vue";
import Spinner from "@/components/ui/Spinner.vue";
import Toaster from "@/components/ui/Toaster.vue";
import { Button } from "@/components/ui/button";
import MapView from "@/views/MapView.vue";
import PhotosGalleryView from "@/views/PhotosGalleryView.vue";
import PhotosLoginView from "@/views/PhotosLoginView.vue";
import SettingsView from "@/views/SettingsView.vue";
import { useAppStore } from "@/stores/app";
import { useDownloadsStore } from "@/stores/downloads";
import { useLocaleStore } from "@/stores/locale";
import { useThemeStore } from "@/stores/theme";
import { devMockEnabled, mockDevicesResponse } from "@/dev/mock";
import { readStored, writeStored } from "@/lib/common/storage";
import type { DevicesResponse } from "@/types";
import { StorageKey, type ShellView } from "@/constants";

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
const settingsInitialCategory = ref("appearance");

function toggleSettings(): void {
  if (view.value === "settings") {
    store.goTo(settingsReturnView);
    return;
  }
  settingsReturnView = view.value;
  settingsInitialCategory.value = "appearance";
  store.goTo("settings");
}

function openMapSettings(): void {
  if (view.value !== "settings") settingsReturnView = view.value;
  settingsInitialCategory.value = "maps";
  store.goTo("settings");
}

let unlistenClose: (() => void) | null = null;
let closeFlushing = false;
let tray: TrayIcon | null = null;
const trayAvailable = ref(false);
const closePromptOpen = ref(false);
const inTauri = isTauri();

type CloseBehavior = "background" | "quit";

function configuredCloseBehavior(): CloseBehavior {
  return readStored(StorageKey.closeBehavior) === "quit" ? "quit" : "background";
}

function shouldPromptCloseChoice(): boolean {
  return readStored(StorageKey.closeBehaviorPrompted) !== "1";
}

async function showMainWindow(): Promise<void> {
  const window = getCurrentWindow();
  await window.show();
  await window.unminimize();
  await window.setFocus();
}

async function requestQuit(): Promise<void> {
  closePromptOpen.value = false;
  closeFlushing = true;
  try {
    await flushBeforeExit();
    await getCurrentWindow().destroy();
  } catch {
    closeFlushing = false;
  }
}

async function minimizeToTray(): Promise<void> {
  closePromptOpen.value = false;
  closeFlushing = true;
  if (!trayAvailable.value) {
    await requestQuit();
    return;
  }
  try {
    await flushBeforeExit();
    await getCurrentWindow().hide();
    closeFlushing = false;
  } catch {
    closeFlushing = false;
  }
}

function rememberCloseChoice(action: CloseBehavior): void {
  writeStored(StorageKey.closeBehavior, action);
  writeStored(StorageKey.closeBehaviorPrompted, "1");
}

async function chooseCloseAction(action: CloseBehavior): Promise<void> {
  rememberCloseChoice(action);
  if (action === "background") await minimizeToTray();
  else await requestQuit();
}

function cancelClosePrompt(): void {
  closePromptOpen.value = false;
  closeFlushing = false;
}

async function setupTray(): Promise<void> {
  if (!inTauri || tray) return;
  try {
    const menu = await Menu.new({
      items: [
        { id: "show", text: i18n.t("tray.show"), action: () => void showMainWindow() },
        { id: "quit", text: i18n.t("tray.quit"), action: () => void requestQuit() },
      ],
    });
    tray = await TrayIcon.new({
      id: "main-tray",
      icon: (await defaultWindowIcon()) ?? undefined,
      menu,
      tooltip: i18n.t("tray.tooltip"),
      showMenuOnLeftClick: false,
      action: (event) => {
        if (event.type === "Click" && event.button === "Left") void showMainWindow();
      },
    });
    trayAvailable.value = true;
  } catch (error) {
    trayAvailable.value = false;
    console.warn("Failed to create system tray", error);
  }
}

async function flushBeforeExit(): Promise<void> {
  await Promise.race([Promise.all([downloads.flushPersistence(), store.flushAccountPersistence()]), new Promise<void>((resolve) => setTimeout(resolve, 3000))]);
}

onMounted(async () => {
  void downloads.initDownloadTracking();
  void setupTray();
  if (!devMockEnabled) void store.restore();
  try {
    const window = getCurrentWindow();
    unlistenClose = await window.onCloseRequested(async (event) => {
      if (closeFlushing) return;

      event.preventDefault();
      const behavior = configuredCloseBehavior();
      if (shouldPromptCloseChoice() && trayAvailable.value) {
        closeFlushing = true;
        closePromptOpen.value = true;
        return;
      }
      if (behavior === "background" && trayAvailable.value) {
        await minimizeToTray();
        return;
      }
      await requestQuit();
    });
  } catch {}
});

onUnmounted(() => {
  unlistenClose?.();
  downloads.disposeDownloadTracking();
  void tray?.close();
  tray = null;
  trayAvailable.value = false;
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

      <SettingsView v-else-if="view === 'settings'" :version="runtime?.version ?? '1.1.0'" :ttl-minutes="runtime?.ttlMinutes ?? null" :vault-enabled="runtime?.vaultEnabled ?? null" :initial-category="settingsInitialCategory" @back="toggleSettings" />

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

        <MapView v-else-if="currentDevices" :data="currentDevices" @play-sound="store.playSoundOn" @open-settings="openMapSettings" />

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

    <div v-if="closePromptOpen" class="fixed inset-0 z-[4000] grid place-items-center bg-black/45 p-6 backdrop-blur-[2px]">
      <div class="w-full max-w-[440px] overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-2xl" role="dialog" aria-modal="true" aria-labelledby="close-prompt-title" @keydown.esc="cancelClosePrompt">
        <div class="px-5 pt-5 pb-4">
          <h2 id="close-prompt-title" class="text-base font-semibold">{{ i18n.t("closePrompt.title") }}</h2>
          <p class="mt-1.5 text-sm leading-6 text-muted-foreground">{{ i18n.t("closePrompt.description") }}</p>
        </div>
        <div class="flex items-center gap-2 border-t border-border bg-muted/20 px-4 py-3">
          <button type="button" class="mr-auto rounded-md px-2.5 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" @click="cancelClosePrompt">{{ i18n.t("closePrompt.cancel") }}</button>
          <Button v-if="trayAvailable" type="button" variant="outline" class="h-8 px-3" @click="chooseCloseAction('background')">{{ i18n.t("closePrompt.background") }}</Button>
          <Button type="button" class="h-8 px-3" @click="chooseCloseAction('quit')">{{ i18n.t("closePrompt.quit") }}</Button>
        </div>
      </div>
    </div>

    <Toaster />
  </div>
</template>
