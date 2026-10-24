import { computed, ref, watch } from "vue";
import { defineStore } from "pinia";

import { Status, StorageKey, type View } from "@/constants";
import { knownAccountKey, sortKnownAccounts, upsertKnownAccount } from "@/lib/accounts/directory";

import { accountAvatar, accountStoreLoad, accountStoreSave, deleteDownloadTasks, playSound, refreshDevices, rekeyDownloadTasks, runtimeInfo, sessionLogout, sessionState } from "@/lib/api";
import { writeStored } from "@/lib/common/storage";
import { toast } from "@/lib/common/toast";
import { isSessionGoneMessage } from "@/lib/session-errors";
import { useDownloadsStore } from "@/stores/downloads";
import type { AccountInfo, DevicesResponse, KnownAccount, KnownAccountStatus, LoginContext, RuntimeInfo, SessionStateResponse } from "@/types";

function sessionIdOf(account: KnownAccount | null): string | null {
  return account?.photosSessionId ?? account?.devicesSessionId ?? null;
}

function accountInfoOf(account: KnownAccount | null): AccountInfo | null {
  if (!account) return null;
  return {
    appleId: account.appleId,
    dsid: account.dsid,
    firstName: account.firstName,
    lastName: account.lastName,
  };
}

function statusOf(state: SessionStateResponse): KnownAccountStatus {
  return state.valid && state.verified ? "valid" : "expired";
}

export const useAppStore = defineStore("app", () => {
  const downloads = useDownloadsStore();

  const devices = ref<DevicesResponse | null>(null);

  const view = ref<View>("photos-login");

  const restoring = ref(true);

  const devicesLoading = ref(false);

  const devicesError = ref<string | null>(null);

  const runtime = ref<RuntimeInfo | null>(null);

  const accounts = ref<KnownAccount[]>([]);

  const activeAccountKey = ref<string | null>(null);

  const accountsLoading = ref(false);

  const pendingReloginKey = ref<string | null>(null);

  const loginContext = ref<LoginContext>({ mode: "normal", appleId: null });

  let switchEpoch = 0;

  let pendingDownloadRekey: Promise<void> = Promise.resolve();

  const activeAccount = computed<KnownAccount | null>(() => accounts.value.find((account) => account.key === activeAccountKey.value) ?? null);

  const photosSession = computed<string | null>(() => activeAccount.value?.photosSessionId ?? null);

  const account = computed<AccountInfo | null>(() => accountInfoOf(activeAccount.value));

  const accountAvatars = ref<Record<string, string>>({});
  const avatarRequests = new Map<string, Promise<void>>();

  watch(view, (next) => writeStored(StorageKey.view, next));

  let pendingAccountPersist: Promise<void> = Promise.resolve();

  function persistAccountState(): void {
    const snapshot = {
      accounts: [...accounts.value],
      activeAccountKey: activeAccountKey.value,
      avatars: { ...accountAvatars.value },
    };
    pendingAccountPersist = pendingAccountPersist
      .then(() => accountStoreSave(snapshot))
      .catch((error) => {
        toast.error(`Could not save account data: ${error instanceof Error ? error.message : "Unknown error"}`);
      });
  }

  async function flushAccountPersistence(): Promise<void> {
    await pendingAccountPersist;
  }

  function setActiveAccountKey(key: string | null): void {
    activeAccountKey.value = key;
    persistAccountState();
  }

  async function loadAccountAvatar(key: string, sessionId: string): Promise<void> {
    if (accountAvatars.value[key]) return;
    const existing = avatarRequests.get(key);
    if (existing) return existing;
    const task = accountAvatar(sessionId)
      .then((result) => {
        if (!result.dataUrl || !accounts.value.some((item) => item.key === key)) return;
        accountAvatars.value = { ...accountAvatars.value, [key]: result.dataUrl };
        persistAccountState();
      })
      .catch(() => undefined)
      .finally(() => avatarRequests.delete(key));
    avatarRequests.set(key, task);
    return task;
  }

  function setAccountStatus(key: string, status: KnownAccountStatus): void {
    const index = accounts.value.findIndex((item) => item.key === key);
    if (index < 0) return;
    const next = [...accounts.value];
    next[index] = { ...next[index], lastKnownStatus: status };
    accounts.value = sortKnownAccounts(next);
    persistAccountState();
  }

  function upsertAccount(incoming: KnownAccount): KnownAccount {
    const result = upsertKnownAccount(accounts.value, incoming);
    accounts.value = result.accounts;
    if (result.replacedKey) {
      const fromKey = result.replacedKey;
      const toKey = result.account.key;
      if (fromKey !== toKey && accountAvatars.value[fromKey]) {
        const nextAvatars = { ...accountAvatars.value };
        nextAvatars[toKey] = nextAvatars[toKey] ?? nextAvatars[fromKey];
        delete nextAvatars[fromKey];
        accountAvatars.value = nextAvatars;
        persistAccountState();
      }
      downloads.rekeyAccount(fromKey, toKey);
      pendingDownloadRekey = rekeyDownloadTasks(fromKey, toKey).catch((error) => {
        toast.error(`Could not migrate download tasks: ${error instanceof Error ? error.message : "Unknown error"}`);
      });
    }
    if (activeAccountKey.value === incoming.key || activeAccountKey.value === result.replacedKey) {
      activeAccountKey.value = result.account.key;
    }
    persistAccountState();
    return result.account;
  }

  function removeAccount(key: string): void {
    const next = accounts.value.filter((account) => account.key !== key);
    if (next.length === accounts.value.length) return;
    accounts.value = next;
    if (activeAccountKey.value === key) {
      activeAccountKey.value = null;
    }
    if (accountAvatars.value[key]) {
      const nextAvatars = { ...accountAvatars.value };
      delete nextAvatars[key];
      accountAvatars.value = nextAvatars;
    }
    persistAccountState();
  }

  function ensureDeviceListCleared(): void {
    devices.value = null;
    devicesLoading.value = false;
    devicesError.value = null;
  }

  function beginRelogin(key: string): void {
    const target = accounts.value.find((item) => item.key === key);
    switchEpoch += 1;
    ensureDeviceListCleared();
    pendingReloginKey.value = key;
    loginContext.value = { mode: "relogin-expired", appleId: target?.appleId ?? null };
    if (target) {
      setActiveAccountKey(target.key);
      void downloads.loadAccountTasks(target.key, null);
    }
    view.value = "photos-login";
  }

  function beginSwitchAccount(): void {
    switchEpoch += 1;
    ensureDeviceListCleared();
    pendingReloginKey.value = null;
    loginContext.value = { mode: accounts.value.length > 0 ? "switch" : "normal", appleId: null };
    view.value = "photos-login";
  }

  function resetLoginContext(): void {
    pendingReloginKey.value = null;
    loginContext.value = { mode: "normal", appleId: null };
  }

  async function validateAccounts(): Promise<void> {
    if (accountsLoading.value) return;
    accountsLoading.value = true;
    try {
      const snapshot = [...accounts.value];
      const results = await Promise.all(
        snapshot.map(async (item) => {
          const sessionId = sessionIdOf(item);
          if (!sessionId) return { key: item.key, state: null, status: "expired" as KnownAccountStatus, sessionId: null };
          const state = await sessionState(sessionId).catch(() => null);
          return { key: item.key, state, status: state ? statusOf(state) : ("unknown" as KnownAccountStatus), sessionId };
        }),
      );

      for (const result of results) {
        const current = accounts.value.find((item) => item.key === result.key);
        if (!current) continue;
        const state = result.state;
        const next = accounts.value.map((item) =>
          item.key === result.key
            ? {
                ...item,
                firstName: state?.firstName ?? item.firstName,
                lastName: state?.lastName ?? item.lastName,
                dsid: state?.dsid ?? item.dsid,
                lastKnownStatus: result.status,
              }
            : item,
        );
        accounts.value = sortKnownAccounts(next);
        if (result.status === "valid" && result.sessionId) void loadAccountAvatar(result.key, result.sessionId);
      }
      persistAccountState();
    } finally {
      accountsLoading.value = false;
    }
  }

  async function loadDevicesWith(sessionId: string, expectedKey: string, expectedEpoch: number): Promise<boolean> {
    devicesLoading.value = true;
    devicesError.value = null;
    try {
      const state = await sessionState(sessionId).catch(() => null);
      if (!state?.valid || !state.verified) {
        devicesError.value = "Session expired. Please sign in again.";
        return false;
      }
      const result = await refreshDevices(sessionId).catch((error) => {
        devicesError.value = error instanceof Error ? error.message : "Could not load devices";
        return null;
      });
      if (expectedKey !== activeAccountKey.value || expectedEpoch !== switchEpoch) return false;
      if (result?.status !== Status.success) {
        devicesError.value = result?.message ?? "Could not load devices";
        return false;
      }

      const current = accounts.value.find((item) => item.key === expectedKey);
      if (current) {
        upsertAccount({
          ...current,
          firstName: result.firstName ?? state.firstName ?? current.firstName,
          lastName: result.lastName ?? state.lastName ?? current.lastName,
          dsid: result.dsid ?? state.dsid ?? current.dsid,
          lastKnownStatus: "valid",
        });
      }
      devices.value = { sessionId, result };
      return true;
    } finally {
      if (expectedKey === activeAccountKey.value && expectedEpoch === switchEpoch) devicesLoading.value = false;
    }
  }

  async function loadAccountStore(): Promise<void> {
    const stored = await accountStoreLoad();
    accounts.value = stored.accounts;
    activeAccountKey.value = stored.activeAccountKey;
    accountAvatars.value = stored.avatars;
  }
  async function restore(): Promise<void> {
    restoring.value = true;
    ensureDeviceListCleared();
    try {
      runtime.value = await runtimeInfo().catch(() => null);
      await loadAccountStore();

      const candidate = activeAccount.value ?? accounts.value[0] ?? null;
      if (!candidate) {
        downloads.activateAccount(null);
        resetLoginContext();
        view.value = "photos-login";
        return;
      }

      const sessionId = sessionIdOf(candidate);
      const state = sessionId ? await sessionState(sessionId).catch(() => null) : null;
      if (!state?.valid || !state.verified) {
        if (state) setAccountStatus(candidate.key, "expired");
        else setAccountStatus(candidate.key, "unknown");
        setActiveAccountKey(candidate.key);
        void downloads.loadAccountTasks(candidate.key, null);
        loginContext.value = { mode: "switch", appleId: null };
        view.value = "photos-login";
        return;
      }

      switchEpoch += 1;
      const epoch = switchEpoch;
      const restored = upsertAccount({
        ...candidate,
        appleId: state.appleId ?? candidate.appleId,
        dsid: state.dsid ?? candidate.dsid,
        firstName: state.firstName ?? candidate.firstName,
        lastName: state.lastName ?? candidate.lastName,
        photosSessionId: sessionId,
        devicesSessionId: sessionId,
        lastKnownStatus: "valid",
        lastUsedAt: Date.now(),
      });
      setActiveAccountKey(restored.key);
      await pendingDownloadRekey;
      await downloads.loadAccountTasks(restored.key, sessionId);
      void loadAccountAvatar(restored.key, sessionId as string);
      pendingReloginKey.value = null;
      loginContext.value = { mode: "normal", appleId: null };

      await loadDevicesWith(sessionId as string, restored.key, epoch);
      view.value = "findmy";
    } catch {
      if (accounts.value.length > 0) loginContext.value = { mode: "switch", appleId: null };
      else resetLoginContext();
      view.value = "photos-login";
    } finally {
      restoring.value = false;
    }
  }

  async function acceptPhotosSession(sessionId: string): Promise<boolean> {
    const state = await sessionState(sessionId).catch(() => null);
    const appleId = state?.appleId?.trim();
    if (!state?.valid || !state.verified || !appleId) {
      toast.error("Could not verify your sign-in. Please sign in again.");
      return false;
    }

    switchEpoch += 1;
    const epoch = switchEpoch;
    ensureDeviceListCleared();
    const now = Date.now();
    const incoming: KnownAccount = {
      key: knownAccountKey(appleId, state.dsid),
      appleId,
      dsid: state.dsid,
      firstName: state.firstName,
      lastName: state.lastName,
      photosSessionId: sessionId,
      devicesSessionId: sessionId,
      lastUsedAt: now,
      lastKnownStatus: "valid",
    };
    const saved = upsertAccount(incoming);
    setActiveAccountKey(saved.key);
    await pendingDownloadRekey;
    await downloads.loadAccountTasks(saved.key, sessionId);
    void loadAccountAvatar(saved.key, sessionId);
    pendingReloginKey.value = null;
    loginContext.value = { mode: "normal", appleId: null };
    view.value = "findmy";
    void loadDevicesWith(sessionId, saved.key, epoch);
    return true;
  }

  async function switchAccount(key: string): Promise<boolean> {
    const target = accounts.value.find((account) => account.key === key);
    if (!target) return false;

    switchEpoch += 1;
    const epoch = switchEpoch;
    const sessionId = sessionIdOf(target);
    if (!sessionId) {
      setAccountStatus(target.key, "expired");
      beginRelogin(target.key);
      return false;
    }

    const state = await sessionState(sessionId).catch(() => null);
    if (epoch !== switchEpoch) return false;
    if (!state) {
      setAccountStatus(target.key, "unknown");
      toast.error("Could not verify the account status. Try again later.");
      return false;
    }
    if (!state.valid || !state.verified) {
      setAccountStatus(target.key, "expired");
      beginRelogin(target.key);
      return false;
    }

    await downloads.flushPersistence();
    const active = upsertAccount({
      ...target,
      appleId: state.appleId ?? target.appleId,
      dsid: state.dsid ?? target.dsid,
      firstName: state.firstName ?? target.firstName,
      lastName: state.lastName ?? target.lastName,
      lastKnownStatus: "valid",
      lastUsedAt: Date.now(),
    });
    setActiveAccountKey(active.key);
    await pendingDownloadRekey;
    await downloads.loadAccountTasks(active.key, sessionId);
    void loadAccountAvatar(active.key, sessionId);
    pendingReloginKey.value = null;
    loginContext.value = { mode: "normal", appleId: null };
    ensureDeviceListCleared();
    view.value = view.value === "photos-login" ? "findmy" : view.value;
    void loadDevicesWith(sessionId, active.key, epoch);
    return true;
  }

  async function selectAccountFromLogin(key: string): Promise<boolean> {
    return switchAccount(key);
  }

  async function logoutCurrentAccount(): Promise<void> {
    const current = activeAccount.value;
    switchEpoch += 1;
    ensureDeviceListCleared();
    pendingReloginKey.value = null;

    if (!current) {
      setActiveAccountKey(null);
      loginContext.value = { mode: accounts.value.length > 0 ? "switch" : "normal", appleId: null };
      view.value = "photos-login";
      return;
    }

    const ids = [...new Set([current.photosSessionId, current.devicesSessionId].filter((id): id is string => Boolean(id)))];
    await downloads.flushPersistence();
    downloads.removeAccountTasks(current.key);
    await deleteDownloadTasks(current.key).catch((error) => {
      toast.error(`Could not clean up download tasks: ${error instanceof Error ? error.message : "Unknown error"}`);
    });
    removeAccount(current.key);
    view.value = "photos-login";
    loginContext.value = { mode: accounts.value.length > 0 ? "switch" : "normal", appleId: null };
    for (const id of ids) await sessionLogout(id).catch(() => undefined);
  }

  async function openPhotos(): Promise<void> {
    const current = activeAccount.value;
    const sessionId = sessionIdOf(current);
    if (!current || !sessionId) {
      beginSwitchAccount();
      return;
    }
    const state = await sessionState(sessionId).catch(() => null);
    if (state?.valid && state.verified) {
      view.value = "photos-gallery";
      return;
    }
    if (!state) {
      toast.error("Could not verify the Photos session. Try again later.");
      return;
    }
    setAccountStatus(current.key, "expired");
    beginRelogin(current.key);
    toast.info("Your Photos session has expired. Please sign in again.");
  }

  async function refreshActiveDevices(): Promise<void> {
    const sessionId = devices.value?.sessionId ?? sessionIdOf(activeAccount.value);
    const expectedKey = activeAccountKey.value;
    const expectedEpoch = switchEpoch;
    if (!sessionId || !expectedKey) return;
    devicesLoading.value = true;
    try {
      const result = await refreshDevices(sessionId);
      if (expectedKey !== activeAccountKey.value || expectedEpoch !== switchEpoch) return;
      if (result.status === Status.success) {
        devices.value = { sessionId, result };
        return;
      }
      if (isSessionGoneMessage(result.message)) {
        setAccountStatus(expectedKey, "expired");
        beginRelogin(expectedKey);
        toast.info("Your session has expired. Please sign in again.");
        return;
      }
      toast.error(result.message ?? "Refresh failed");
    } catch (error) {
      if (expectedKey === activeAccountKey.value && expectedEpoch === switchEpoch) toast.error(error instanceof Error ? error.message : "Refresh failed");
    } finally {
      if (expectedKey === activeAccountKey.value && expectedEpoch === switchEpoch) devicesLoading.value = false;
    }
  }

  async function ensureDevices(): Promise<void> {
    if (devices.value || devicesLoading.value) return;
    const current = activeAccount.value;
    const sessionId = sessionIdOf(current);
    if (!current || !sessionId) {
      beginSwitchAccount();
      return;
    }

    const state = await sessionState(sessionId).catch(() => null);
    if (!state) {
      toast.error("Could not verify the session. Try again later.");
      return;
    }
    if (!state.valid || !state.verified) {
      setAccountStatus(current.key, "expired");
      beginRelogin(current.key);
      return;
    }

    const loaded = await loadDevicesWith(sessionId, current.key, switchEpoch);
    if (loaded) return;
    if (devicesError.value && isSessionGoneMessage(devicesError.value)) {
      setAccountStatus(current.key, "expired");
      beginRelogin(current.key);
      toast.info("Your Devices session has expired. Please sign in again.");
      return;
    }
    toast.error("Could not load devices. Use Refresh in the top-right corner to try again.");
  }

  function goTo(next: View): void {
    view.value = next;
  }

  async function playSoundOn(deviceId: string): Promise<boolean> {
    const sessionId = devices.value?.sessionId ?? sessionIdOf(activeAccount.value);
    const expectedKey = activeAccountKey.value;
    const expectedEpoch = switchEpoch;
    if (!sessionId) {
      toast.error("Session expired. Please sign in again.");
      return false;
    }
    try {
      const result = await playSound(sessionId, deviceId);
      if (expectedKey !== activeAccountKey.value || expectedEpoch !== switchEpoch) return false;
      if (result.success) {
        toast.success(result.message ?? "Play Sound command sent");
        return true;
      }
      toast.error(result.message ?? "Could not play sound");
      return false;
    } catch (error) {
      if (expectedKey === activeAccountKey.value && expectedEpoch === switchEpoch) toast.error(error instanceof Error ? error.message : "Could not play sound");
      return false;
    }
  }

  return {
    devices,
    view,
    restoring,
    devicesLoading,
    devicesError,
    runtime,
    accounts,
    activeAccountKey,
    activeAccount,
    accountsLoading,
    pendingReloginKey,
    loginContext,
    photosSession,
    account,
    accountAvatars,
    loadAccountStore,
    flushAccountPersistence,
    restore,
    loadDevicesWith,
    upsertAccount,
    validateAccounts,
    switchAccount,
    selectAccountFromLogin,
    beginRelogin,
    beginSwitchAccount,
    resetLoginContext,
    removeAccount,
    acceptPhotosSession,
    logoutCurrentAccount,
    openPhotos,
    ensureDevices,
    refreshActiveDevices,
    playSoundOn,
    goTo,
  };
});
