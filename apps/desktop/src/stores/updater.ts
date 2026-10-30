import { computed, ref } from "vue";
import { defineStore } from "pinia";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { appUpdateCancelDownload, appUpdateCheck, appUpdateDownload, appUpdateInstall, appUpdateRestart, listenAppUpdateProgress } from "@/lib/api";
import { useAppStore } from "@/stores/app";
import { useDownloadsStore } from "@/stores/downloads";
import type { AppUpdateInfo, AppUpdateProgressEvent } from "@/types";

function messageOf(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return String(error);
}

function isCancellation(error: unknown): boolean {
  return messageOf(error).toLowerCase().includes("cancel");
}

export const useUpdaterStore = defineStore("updater", () => {
  const checking = ref(false);
  const checked = ref(false);
  const info = ref<AppUpdateInfo | null>(null);
  const downloading = ref(false);
  const downloaded = ref(false);
  const installing = ref(false);
  const installed = ref(false);
  const progress = ref<number | null>(null);
  const error = ref<string | null>(null);
  const lastCheckedAt = ref<number | null>(null);

  let unlistenProgress: UnlistenFn | null = null;
  let downloadAttempt = 0;

  const hasUpdate = computed(() => info.value !== null);

  async function check(): Promise<AppUpdateInfo | null> {
    if (checking.value || downloading.value || installing.value) return info.value;
    checking.value = true;
    error.value = null;
    try {
      const update = await appUpdateCheck();
      info.value = update;
      downloaded.value = false;
      installed.value = false;
      progress.value = null;
      return update;
    } catch (cause) {
      info.value = null;
      error.value = messageOf(cause);
      return null;
    } finally {
      checking.value = false;
      checked.value = true;
      lastCheckedAt.value = Date.now();
    }
  }

  async function download(): Promise<void> {
    if (!info.value || downloading.value || installing.value || downloaded.value) return;
    const attempt = ++downloadAttempt;
    downloading.value = true;
    progress.value = 0;
    error.value = null;
    unlistenProgress?.();
    unlistenProgress = null;
    let currentListener: UnlistenFn | null = null;
    try {
      currentListener = await listenAppUpdateProgress((event: AppUpdateProgressEvent) => {
        if (attempt !== downloadAttempt) return;
        progress.value = event.total && event.total > 0 ? Math.min(100, Math.round((event.downloaded / event.total) * 100)) : null;
      });
      if (attempt !== downloadAttempt) return;
      unlistenProgress = currentListener;
      await appUpdateDownload();
      if (attempt !== downloadAttempt) return;
      progress.value = 100;
      downloaded.value = true;
    } catch (cause) {
      if (attempt === downloadAttempt && !isCancellation(cause)) error.value = messageOf(cause);
      if (attempt === downloadAttempt) progress.value = 0;
    } finally {
      currentListener?.();
      if (unlistenProgress === currentListener) unlistenProgress = null;
      if (attempt === downloadAttempt) downloading.value = false;
    }
  }

  async function cancelDownload(): Promise<void> {
    if (!downloading.value) return;
    downloadAttempt += 1;
    downloading.value = false;
    progress.value = 0;
    error.value = null;
    unlistenProgress?.();
    unlistenProgress = null;
    await appUpdateCancelDownload().catch(() => undefined);
  }

  async function install(): Promise<void> {
    if (!downloaded.value || installing.value) return;
    installing.value = true;
    error.value = null;
    try {
      await appUpdateInstall();
      downloaded.value = false;
      installed.value = true;
      progress.value = 100;
    } catch (cause) {
      error.value = messageOf(cause);
    } finally {
      installing.value = false;
    }
  }

  async function restart(): Promise<void> {
    const app = useAppStore();
    const downloads = useDownloadsStore();
    error.value = null;
    try {
      await Promise.race([Promise.all([downloads.flushPersistence(), app.flushAccountPersistence()]), new Promise<void>((resolve) => setTimeout(resolve, 3000))]);
      await appUpdateRestart();
    } catch (cause) {
      error.value = messageOf(cause);
    }
  }

  function clearError(): void {
    error.value = null;
  }

  return {
    checking,
    checked,
    info,
    downloading,
    downloaded,
    installing,
    installed,
    progress,
    error,
    lastCheckedAt,
    hasUpdate,
    check,
    download,
    cancelDownload,
    install,
    restart,
    clearError,
  };
});
