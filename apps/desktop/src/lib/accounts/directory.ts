import type { KnownAccount } from "@/types";

export function normalizeAppleId(appleId: string): string {
  return appleId.trim().toLowerCase();
}

export function knownAccountKey(appleId: string, dsid: string | null | undefined): string {
  const normalizedDsid = dsid?.trim();
  if (normalizedDsid) return `dsid:${normalizedDsid}`;
  return `apple:${normalizeAppleId(appleId)}`;
}

function hasUsableAppleId(account: KnownAccount): boolean {
  return normalizeAppleId(account.appleId).length > 0;
}

function sameAccount(a: KnownAccount, b: KnownAccount): boolean {
  if (a.key === b.key) return true;
  const aDsid = a.dsid?.trim() ?? "";
  const bDsid = b.dsid?.trim() ?? "";
  if (aDsid && bDsid) return aDsid === bDsid;
  return hasUsableAppleId(a) && hasUsableAppleId(b) && normalizeAppleId(a.appleId) === normalizeAppleId(b.appleId);
}

export function sortKnownAccounts(accounts: readonly KnownAccount[]): KnownAccount[] {
  return [...accounts].sort((a, b) => b.lastUsedAt - a.lastUsedAt || normalizeAppleId(a.appleId).localeCompare(normalizeAppleId(b.appleId)));
}

export function upsertKnownAccount(accounts: readonly KnownAccount[], incoming: KnownAccount): { accounts: KnownAccount[]; account: KnownAccount; replacedKey: string | null } {
  const existingIndex = accounts.findIndex((account) => sameAccount(account, incoming));
  if (existingIndex < 0) {
    const next = sortKnownAccounts([...accounts, incoming]);
    return { accounts: next, account: next.find((account) => sameAccount(account, incoming)) ?? incoming, replacedKey: null };
  }

  const existing = accounts[existingIndex];
  const merged: KnownAccount = {
    key: incoming.dsid?.trim() ? incoming.key : existing.key,
    appleId: incoming.appleId.trim() || existing.appleId,
    dsid: incoming.dsid?.trim() || existing.dsid,
    firstName: incoming.firstName ?? existing.firstName,
    lastName: incoming.lastName ?? existing.lastName,
    photosSessionId: incoming.photosSessionId ?? existing.photosSessionId,
    devicesSessionId: incoming.devicesSessionId ?? existing.devicesSessionId,
    lastUsedAt: Math.max(existing.lastUsedAt, incoming.lastUsedAt),
    lastKnownStatus: incoming.lastKnownStatus,
  };
  const next = [...accounts];
  next[existingIndex] = merged;
  return { accounts: sortKnownAccounts(next), account: merged, replacedKey: existing.key };
}
