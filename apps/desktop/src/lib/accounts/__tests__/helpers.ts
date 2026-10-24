import { vi } from "vitest";

import { accountStoreLoad } from "@/lib/api";
import type { KnownAccount } from "@/types";

export function mockAccountStore(accounts: KnownAccount[], activeAccountKey: string | null = null, avatars: Record<string, string> = {}): void {
  vi.mocked(accountStoreLoad).mockResolvedValueOnce({ accounts, activeAccountKey, avatars });
}
