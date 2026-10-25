import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";

import type {
  AccountAvatar,
  AccountStoreSnapshot,
  DeviceInfoResult,
  DownloadProgressEvent,
  LibraryTotal,
  LoadedDownloadTasks,
  Photos2faOptions,
  PhotosListRequest,
  PhotosLoginResult,
  PhotosPage,
  PhotosStatus,
  PhotosTimeline,
  PlaySoundResult,
  RuntimeInfo,
  SavedPhotoResult,
  SessionStateResponse,
} from "@/types";

function describeError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

async function call<T>(command: string, payload?: unknown): Promise<T> {
  try {
    return await invoke<T>(command, payload === undefined ? undefined : { payload });
  } catch (error) {
    throw new Error(describeError(error));
  }
}

export function runtimeInfo(): Promise<RuntimeInfo> {
  return call<RuntimeInfo>("runtime_info");
}

export function accountStoreLoad(): Promise<AccountStoreSnapshot> {
  return call<AccountStoreSnapshot>("account_store_load");
}

export function accountStoreSave(snapshot: AccountStoreSnapshot): Promise<void> {
  return call<void>("account_store_save", snapshot);
}
export function accountAvatar(sessionId: string): Promise<AccountAvatar> {
  return call<AccountAvatar>("session_avatar", { sessionId });
}

export function refreshDevices(sessionId: string): Promise<DeviceInfoResult> {
  return call<DeviceInfoResult>("refresh_devices", { sessionId });
}

export function playSound(sessionId: string, deviceId: string): Promise<PlaySoundResult> {
  return call<PlaySoundResult>("play_sound", { sessionId, deviceId });
}

export function photosLogin(payload: { appleId: string; password: string; chinaMainland: boolean }): Promise<PhotosLoginResult> {
  return call<PhotosLoginResult>("photos_login", payload);
}

export function photosVerify2fa(payload: { sessionId: string; code: string }): Promise<PhotosLoginResult> {
  return call<PhotosLoginResult>("photos_verify_2fa", payload);
}

export function photosFinish2fa(payload: { sessionId: string; trustBrowser: boolean }): Promise<PhotosLoginResult> {
  return call<PhotosLoginResult>("photos_finish_2fa", payload);
}

export function photosSendCode(sessionId: string): Promise<PhotosLoginResult> {
  return call<PhotosLoginResult>("photos_send_code", { sessionId });
}

export function photosList(payload: PhotosListRequest): Promise<PhotosPage> {
  return call<PhotosPage>("photos_list", payload);
}

export function photosTotal(sessionId: string): Promise<LibraryTotal> {
  return call<LibraryTotal>("photos_total", { sessionId });
}

export function photosAssetDistribution(sessionId: string): Promise<PhotosTimeline> {
  return call<PhotosTimeline>("photos_asset_distribution", { sessionId });
}

export function sessionState(sessionId: string): Promise<SessionStateResponse> {
  return call<SessionStateResponse>("session_state", { sessionId });
}

export async function sessionLogout(sessionId: string): Promise<void> {
  await call<string>("session_logout", { sessionId });
}

export function photos2faOptions(sessionId: string): Promise<Photos2faOptions> {
  return call<Photos2faOptions>("photos_2fa_options", { sessionId });
}

export function photosCheckApproval(sessionId: string): Promise<PhotosLoginResult> {
  return call<PhotosLoginResult>("photos_check_approval", { sessionId });
}

export function photosStatus(sessionId: string): Promise<PhotosStatus> {
  return call<PhotosStatus>("photos_status", { sessionId });
}

export async function downloadPhoto(url: string, name: string, title = "Save to"): Promise<string | null> {
  const targetPath = await save({ title, defaultPath: name });
  if (!targetPath) return null;
  const result = await call<SavedPhotoResult>("save_photo", { url, name, targetPath });
  return result.path;
}

export async function pickDownloadDirectory(title = "Choose save folder"): Promise<string | null> {
  const picked = await open({ title, directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}

export function savePhotoTo(url: string, name: string, targetPath: string, transferId?: string, partialKey?: string): Promise<SavedPhotoResult> {
  return call<SavedPhotoResult>("save_photo", { url, name, targetPath, transferId, partialKey });
}

export function setDownloadSpeedLimit(bytesPerSec: number): Promise<void> {
  return call<void>("download_set_speed_limit", { bytesPerSec });
}

export function cancelDownloadTransfer(transferId: string): Promise<void> {
  return call<void>("download_cancel_transfer", { transferId });
}

export function listenDownloadProgress(handler: (progress: DownloadProgressEvent) => void): Promise<UnlistenFn> {
  return listen<DownloadProgressEvent>("download-progress", (event) => handler(event.payload));
}

export function loadDownloadTasks(accountKey: string): Promise<LoadedDownloadTasks> {
  return call<LoadedDownloadTasks>("download_tasks_load", { accountKey });
}

export function saveDownloadTasks(accountKey: string, tasks: unknown[]): Promise<void> {
  return call<void>("download_tasks_save", { accountKey, tasks });
}

export function rekeyDownloadTasks(fromKey: string, toKey: string): Promise<void> {
  return call<void>("download_tasks_rekey", { fromKey, toKey });
}

export function deleteDownloadTasks(accountKey: string): Promise<void> {
  return call<void>("download_tasks_delete_account", { accountKey });
}

export async function openExternal(target: string): Promise<void> {
  await openUrl(target);
}

export async function openDownloadFolder(path: string): Promise<void> {
  await call<void>("open_download_folder", { path });
}
