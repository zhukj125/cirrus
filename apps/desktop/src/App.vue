<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { storeToRefs } from "pinia";
import { RefreshCw, ShieldCheck } from "@lucide/vue";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import AppShell from "@/components/layout/AppShell.vue";
import AppTabBar from "@/components/layout/AppTabBar.vue";
import TitleBar from "@/components/layout/TitleBar.vue";
import DownloadManagerDialog from "@/components/photos/DownloadManagerDialog.vue";
import Spinner from "@/components/ui/Spinner.vue";
import Toaster from "@/components/ui/Toaster.vue";
import { Button } from "@/components/ui/button";
import CalendarView from "@/views/CalendarView.vue";
import ContactsView from "@/views/ContactsView.vue";
import MapView from "@/views/MapView.vue";
import NotesView from "@/views/NotesView.vue";
import PhotosGalleryView from "@/views/PhotosGalleryView.vue";
import PhotosLoginView from "@/views/PhotosLoginView.vue";
import SettingsView from "@/views/SettingsView.vue";
import { useAppStore } from "@/stores/app";
import { useDownloadsStore } from "@/stores/downloads";
import { useLocaleStore } from "@/stores/locale";
import { useThemeStore } from "@/stores/theme";
import { useUpdaterStore } from "@/stores/updater";
import { devMockEnabled, mockDevicesResponse } from "@/dev/mock";
import { isTrayAvailable, mcpAuthorize, mcpCancelLogin, mcpCompleteLogin, mcpPendingAuthorization, quitApp, setTrayLabels } from "@/lib/api";
import { readStored, writeStored } from "@/lib/common/storage";
import { toast } from "@/lib/common/toast";
import type { DevicesResponse, McpAuthorizationRequest } from "@/types";
import { StorageKey, isShellView, type ShellView, type View, type WorkspaceTab } from "@/constants";

const store = useAppStore();

const downloads = useDownloadsStore();
const i18n = useLocaleStore();
const updater = useUpdaterStore();

useThemeStore();

if (devMockEnabled) {
  store.devices = mockDevicesResponse;
  store.view = "findmy";
  store.restoring = false;
}

const { devices, devicesError, view, photosSession, restoring, devicesLoading, accounts, activeAccountKey, accountsLoading, loginContext, pendingReloginKey, accountAvatars, runtime } = storeToRefs(store);

const currentDevices = computed<DevicesResponse | null>(() => devices.value);

const signedIn = computed(() => view.value !== "photos-login" && (photosSession.value !== null || devices.value !== null));

const shellView = computed<ShellView>(() => (view.value === "photos-gallery" || view.value === "notes" || view.value === "contacts" || view.value === "calendar" ? view.value : "findmy"));
function isWorkspaceTab(value: unknown): value is WorkspaceTab {
  return typeof value === "string" && (isShellView(value) || value === "settings");
}

function storedWorkspaceTabs(): WorkspaceTab[] {
  const raw = readStored(StorageKey.workspaceTabs);
  if (!raw) return ["findmy"];
  try {
    const parsed: unknown = JSON.parse(raw);
    const tabs = Array.isArray(parsed) ? [...new Set(parsed.filter(isWorkspaceTab))] : [];
    if (!tabs.some(isShellView)) tabs.unshift("findmy");
    return tabs;
  } catch {
    return ["findmy"];
  }
}

const openWorkspaceTabs = ref<WorkspaceTab[]>(storedWorkspaceTabs());
const lastShellTab = ref<ShellView>(openWorkspaceTabs.value.find(isShellView) ?? "findmy");
const activeWorkspaceTab = computed<WorkspaceTab | null>(() => {
  if (view.value === "settings") return "settings";
  return isShellView(view.value) ? view.value : null;
});

watch(
  view,
  (next) => {
    if (!signedIn.value || !isWorkspaceTab(next)) return;
    if (!openWorkspaceTabs.value.includes(next)) openWorkspaceTabs.value.push(next);
    if (isShellView(next)) lastShellTab.value = next;
  },
  { immediate: true },
);

watch(
  openWorkspaceTabs,
  (tabs) => {
    writeStored(StorageKey.workspaceTabs, JSON.stringify(tabs));
  },
  { deep: true },
);

const headerTitle = computed(() => {
  if (shellView.value === "photos-gallery") return i18n.t("nav.photos");
  if (shellView.value === "notes") return i18n.t("nav.notes");
  if (shellView.value === "contacts") return i18n.t("nav.contacts");
  if (shellView.value === "calendar") return i18n.t("nav.calendar");
  return i18n.t("nav.devices");
});

const headerSubtitle = computed(() => {
  if (shellView.value === "photos-gallery") return i18n.t("header.photosLibrary");
  if (shellView.value === "notes") return i18n.t("header.notes");
  if (shellView.value === "contacts") return i18n.t("header.contacts");
  if (shellView.value === "calendar") return i18n.t("header.calendar");
  const count = devices.value?.result.deviceCount;
  return count ? i18n.t("header.deviceCount", { count }) : null;
});
const titleBarContext = computed(() => {
  if (view.value === "settings") return i18n.t("settings.title");
  if (view.value === "photos-login") return i18n.t("login.title");
  return null;
});

async function activateWorkspaceTab(next: WorkspaceTab): Promise<void> {
  if (next !== "settings" && view.value === next) return;
  if (next === "settings") {
    store.goTo("settings");
    return;
  }
  if (next === "photos-gallery") {
    await store.openPhotos();
    return;
  }
  if (next === "notes") {
    await store.openNotes();
    return;
  }
  if (next === "contacts") {
    await store.openContacts();
    return;
  }
  if (next === "calendar") {
    await store.openCalendar();
    return;
  }
  store.goTo("findmy");
  void store.ensureDevices();
}

function onNavigate(next: ShellView): void {
  void activateWorkspaceTab(next);
}

let settingsReturnView: View = "photos-login";
const settingsInitialCategory = ref("appearance");

function settingsFallback(): View {
  if (!signedIn.value) return settingsReturnView;
  if (isShellView(settingsReturnView) && openWorkspaceTabs.value.includes(settingsReturnView)) return settingsReturnView;
  return openWorkspaceTabs.value.find(isShellView) ?? lastShellTab.value;
}

function activateSettingsFallback(): void {
  const fallback = settingsFallback();
  if (isShellView(fallback)) {
    void activateWorkspaceTab(fallback);
    return;
  }
  store.goTo(fallback);
}

function closeSettingsTab(): void {
  openWorkspaceTabs.value = openWorkspaceTabs.value.filter((tab) => tab !== "settings");
  activateSettingsFallback();
}

function closeWorkspaceTab(tab: WorkspaceTab): void {
  if (tab === "settings") {
    closeSettingsTab();
    return;
  }
  if (openWorkspaceTabs.value.filter(isShellView).length <= 1) return;

  const index = openWorkspaceTabs.value.indexOf(tab);
  if (index < 0) return;
  openWorkspaceTabs.value = openWorkspaceTabs.value.filter((item) => item !== tab);
  if (view.value !== tab) return;

  const shellTabs = openWorkspaceTabs.value.filter(isShellView);
  const fallback = shellTabs[Math.min(index, shellTabs.length - 1)];
  if (fallback) void activateWorkspaceTab(fallback);
}

function toggleSettings(): void {
  if (view.value === "settings") {
    closeSettingsTab();
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
let unlistenTrayQuit: (() => void) | null = null;
let closeFlushing = false;
const trayAvailable = ref(false);
const closePromptOpen = ref(false);
const inTauri = isTauri();

type CloseBehavior = "background" | "quit";

const mcpAuthorization = ref<McpAuthorizationRequest | null>(null);
const mcpAuthorizing = ref(false);
const mcpLoginRequestId = ref<string | null>(null);
let unlistenMcpAuthorization: (() => void) | null = null;
let unlistenMcpAuthorizationFinished: (() => void) | null = null;
let unlistenMcpLoginRequest: (() => void) | null = null;
let unlistenMcpLoginFinished: (() => void) | null = null;
let unlistenMcpAccountsChanged: (() => void) | null = null;
let unlistenMcpDownloadCompleted: (() => void) | null = null;

function clearMcpLoginRequest(requestId?: string): void {
  if (requestId && mcpLoginRequestId.value !== requestId) return;
  mcpLoginRequestId.value = null;
  if (store.loginContext.mode === "mcp") store.resetLoginContext();
}

async function handleMcpAuthorization(approved: boolean): Promise<void> {
  const request = mcpAuthorization.value;
  if (!request || mcpAuthorizing.value) return;
  mcpAuthorizing.value = true;
  try {
    const result = await mcpAuthorize({ requestId: request.requestId, approved });
    mcpAuthorization.value = null;
    if (result.approved) {
      mcpLoginRequestId.value = result.requestId;
      toast.info(i18n.t("mcp.loginRequested", { client: result.clientName }));
    } else {
      toast.info(i18n.t("mcp.authorizationDenied"));
    }
  } catch (error) {
    mcpAuthorization.value = null;
    toast.error(i18n.t("mcp.authorizationFailed", { error: error instanceof Error ? error.message : i18n.t("app.unknownError") }));
  } finally {
    mcpAuthorizing.value = false;
  }
}

async function handlePhotosSuccess(sessionId: string): Promise<boolean> {
  const requestId = mcpLoginRequestId.value;
  const accepted = await store.acceptPhotosSession(sessionId);
  if (requestId) {
    if (!accepted) {
      await mcpCancelLogin(requestId).catch(() => undefined);
      clearMcpLoginRequest(requestId);
      return false;
    }
    try {
      const account = await mcpCompleteLogin({ requestId, sessionId });
      toast.success(i18n.t("mcp.loginCompleted", { account: account.displayName }));
      if (!account.mcpEnabled) toast.warning(i18n.t("mcp.accountAccessDisabled", { account: account.displayName }));
    } catch (error) {
      await mcpCancelLogin(requestId).catch(() => undefined);
      toast.error(i18n.t("mcp.loginCompleteFailed", { error: error instanceof Error ? error.message : i18n.t("app.unknownError") }));
    } finally {
      clearMcpLoginRequest(requestId);
    }
  }
  return accepted;
}

async function cancelMcpLogin(): Promise<void> {
  const requestId = mcpLoginRequestId.value;
  if (requestId) {
    await mcpCancelLogin(requestId).catch(() => undefined);
    toast.info(i18n.t("mcp.loginCancelled"));
  }
  clearMcpLoginRequest();
  store.beginSwitchAccount();
}

function configuredCloseBehavior(): CloseBehavior {
  return readStored(StorageKey.closeBehavior) === "quit" ? "quit" : "background";
}

function shouldPromptCloseChoice(): boolean {
  return readStored(StorageKey.closeBehaviorPrompted) !== "1";
}

async function requestQuit(): Promise<void> {
  closePromptOpen.value = false;
  closeFlushing = true;
  try {
    await flushBeforeExit();
    await quitApp();
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

async function refreshTrayAvailability(): Promise<void> {
  trayAvailable.value = inTauri && (await isTrayAvailable().catch(() => false));
}

async function checkForAvailableUpdate(): Promise<void> {
  if (!inTauri || devMockEnabled) return;
  const update = await updater.check();
  if (!update) return;
  const version = update.version.startsWith("v") ? update.version : `v${update.version}`;
  toast.info(i18n.t("updates.availableToast", { version }), 8000);
}
async function syncTrayLabels(): Promise<void> {
  if (!trayAvailable.value) return;
  await setTrayLabels({
    show: i18n.t("tray.show"),
    quit: i18n.t("tray.quit"),
    tooltip: i18n.t("tray.tooltip"),
  }).catch(() => undefined);
}

watch(
  () => i18n.locale,
  () => void syncTrayLabels(),
);

async function flushBeforeExit(): Promise<void> {
  await Promise.race([Promise.all([downloads.flushPersistence(), store.flushAccountPersistence()]), new Promise<void>((resolve) => setTimeout(resolve, 3000))]);
}

onMounted(async () => {
  void downloads.initDownloadTracking();
  void refreshTrayAvailability().then(syncTrayLabels);
  if (!devMockEnabled) void store.restore();
  try {
    unlistenTrayQuit = await listen("tray-quit-requested", () => void requestQuit());
  } catch {}
  try {
    unlistenMcpAuthorization = await listen<McpAuthorizationRequest>("mcp-authorization-request", (event) => {
      mcpAuthorization.value = event.payload;
    });
    unlistenMcpAuthorizationFinished = await listen<{ requestId: string }>("mcp-authorization-finished", (event) => {
      if (mcpAuthorization.value?.requestId === event.payload.requestId) mcpAuthorization.value = null;
    });
    unlistenMcpLoginRequest = await listen<{ requestId: string; clientName: string }>("mcp-login-request", (event) => {
      mcpLoginRequestId.value = event.payload.requestId;
      store.beginMcpLogin(event.payload.clientName);
    });
    unlistenMcpLoginFinished = await listen<{ requestId: string }>("mcp-login-finished", (event) => {
      clearMcpLoginRequest(event.payload.requestId);
    });
    unlistenMcpAccountsChanged = await listen<{ accountKey: string; reason: string }>("mcp-accounts-changed", (event) => {
      if (event.payload.reason === "logout") toast.info(i18n.t("mcp.accountSignedOut"));
      void store.restore();
    });
    unlistenMcpDownloadCompleted = await listen<{ accountKey: string; path: string; fileName: string; sizeBytes: number }>("mcp-download-completed", (event) => {
      toast.success(i18n.t("mcp.downloadCompleted", { file: event.payload.fileName }));
    });
  } catch {}
  try {
    mcpAuthorization.value = await mcpPendingAuthorization();
  } catch {}
  void checkForAvailableUpdate();
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
  unlistenTrayQuit?.();
  unlistenMcpAuthorization?.();
  unlistenMcpAuthorizationFinished?.();
  unlistenMcpLoginRequest?.();
  unlistenMcpLoginFinished?.();
  unlistenMcpAccountsChanged?.();
  unlistenMcpDownloadCompleted?.();
  downloads.disposeDownloadTracking();
  trayAvailable.value = false;
});
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <TitleBar :show-settings="true" :settings-active="view === 'settings'" :context-label="titleBarContext" @settings="toggleSettings" />

    <div class="flex min-h-0 flex-1 flex-col">
      <AppTabBar v-if="signedIn && !restoring" :tabs="openWorkspaceTabs" :active="activeWorkspaceTab" @select="activateWorkspaceTab" @close="closeWorkspaceTab" @add="activateWorkspaceTab" />

      <div class="min-h-0 flex-1">
        <div v-if="restoring" class="grid h-full place-items-center">
          <div class="flex flex-col items-center gap-2.5">
            <Spinner :size="22" />
            <p class="text-sm text-muted-foreground">{{ i18n.t("app.restoring") }}</p>
          </div>
        </div>

        <SettingsView v-else-if="view === 'settings'" :version="runtime?.version ?? '1.4.2'" :ttl-minutes="runtime?.ttlMinutes ?? null" :vault-enabled="runtime?.vaultEnabled ?? null" :initial-category="settingsInitialCategory" @back="toggleSettings" />

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

          <div v-if="openWorkspaceTabs.includes('findmy')" v-show="shellView === 'findmy'" class="h-full">
            <MapView v-if="currentDevices" :data="currentDevices" @play-sound="store.playSoundOn" @open-settings="openMapSettings" />

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
          </div>

          <div v-if="openWorkspaceTabs.includes('photos-gallery')" v-show="shellView === 'photos-gallery'" class="h-full">
            <PhotosGalleryView v-if="photosSession" :key="photosSession" :session-id="photosSession" @logout="store.logoutCurrentAccount()" />
          </div>

          <div v-if="openWorkspaceTabs.includes('notes')" v-show="shellView === 'notes'" class="h-full">
            <NotesView v-if="photosSession" :key="'notes-' + photosSession" :session-id="photosSession" @logout="store.logoutCurrentAccount()" />
          </div>

          <div v-if="openWorkspaceTabs.includes('contacts')" v-show="shellView === 'contacts'" class="h-full">
            <ContactsView v-if="photosSession" :key="'contacts-' + photosSession" :session-id="photosSession" @logout="store.logoutCurrentAccount()" />
          </div>

          <div v-if="openWorkspaceTabs.includes('calendar')" v-show="shellView === 'calendar'" class="h-full">
            <CalendarView v-if="photosSession" :key="'calendar-' + photosSession" :session-id="photosSession" @logout="store.logoutCurrentAccount()" />
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
          @success="handlePhotosSuccess"
          @select-account="store.selectAccountFromLogin"
          @switch-other="store.beginSwitchAccount"
          @cancel-mcp="cancelMcpLogin"
        />
      </div>
    </div>

    <DownloadManagerDialog v-if="downloads.dialogOpen" />

    <div v-if="mcpAuthorization" class="fixed inset-0 z-[4100] grid place-items-center bg-black/45 p-6 backdrop-blur-[2px]">
      <div class="w-full max-w-[460px] overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-2xl" role="dialog" aria-modal="true" aria-labelledby="mcp-authorization-title" @keydown.esc="handleMcpAuthorization(false)">
        <div class="px-5 pt-5 pb-4">
          <div class="mb-3 flex items-center gap-3">
            <span class="grid size-10 shrink-0 place-items-center rounded-full bg-primary/10 text-primary"><ShieldCheck :size="19" /></span>
            <div class="min-w-0">
              <h2 id="mcp-authorization-title" class="text-base font-semibold">{{ i18n.t("mcp.authorizationTitle") }}</h2>
              <p class="mt-0.5 truncate text-xs text-muted-foreground">
                {{ mcpAuthorization.clientName }}<span v-if="mcpAuthorization.clientVersion"> · {{ mcpAuthorization.clientVersion }}</span>
              </p>
            </div>
          </div>
          <p class="text-sm leading-6 text-muted-foreground">{{ i18n.t("mcp.authorizationDescription", { client: mcpAuthorization.clientName }) }}</p>
          <div class="mt-4 rounded-lg border border-border bg-secondary/45 px-3.5 py-3 text-xs leading-5 text-muted-foreground">
            {{ i18n.t("mcp.authorizationPrivacy") }}
          </div>
          <p class="mt-3 text-2xs text-muted-foreground">
            {{ i18n.t("mcp.authorizationTool") }}: <span class="font-mono">{{ mcpAuthorization.tool }}</span>
          </p>
        </div>
        <div class="flex items-center gap-2 border-t border-border bg-muted/20 px-4 py-3">
          <button type="button" class="mr-auto rounded-md px-2.5 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50" :disabled="mcpAuthorizing" @click="handleMcpAuthorization(false)">{{ i18n.t("common.cancel") }}</button>
          <Button type="button" class="h-8 px-3" :disabled="mcpAuthorizing" @click="handleMcpAuthorization(true)">
            <Spinner v-if="mcpAuthorizing" :size="12" />
            {{ mcpAuthorizing ? i18n.t("mcp.authorizing") : i18n.t("mcp.authorize") }}
          </Button>
        </div>
      </div>
    </div>

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
