import { describe, expect, it } from "vitest";

import { knownAccountKey, normalizeAppleId, sortKnownAccounts, upsertKnownAccount } from "@/lib/accounts/directory";
import type { KnownAccount } from "@/types";

function account(overrides: Partial<KnownAccount> = {}): KnownAccount {
  return {
    key: "apple:person@example.com",
    appleId: "person@example.com",
    dsid: null,
    firstName: null,
    lastName: null,
    photosSessionId: "session-a",
    devicesSessionId: "session-a",
    lastUsedAt: 100,
    lastKnownStatus: "valid",
    ...overrides,
  };
}

describe("账号目录纯函数", () => {
  it("有 DSID 时使用 DSID 作为 key，否则使用规范化邮箱", () => {
    expect(knownAccountKey(" Person@Example.com ", "12345")).toBe("dsid:12345");
    expect(knownAccountKey(" Person@Example.com ", null)).toBe("apple:person@example.com");
    expect(normalizeAppleId("  PERSON@example.COM ")).toBe("person@example.com");
  });

  it("重复登录同一账号会合并而不是新增", () => {
    const first = account();
    const incoming = account({
      key: "dsid:123",
      dsid: "123",
      firstName: "三",
      lastUsedAt: 200,
      photosSessionId: "session-b",
      devicesSessionId: "session-b",
    });
    const result = upsertKnownAccount([first], incoming);

    expect(result.accounts).toHaveLength(1);
    expect(result.account.key).toBe("dsid:123");
    expect(result.account.firstName).toBe("三");
    expect(result.account.photosSessionId).toBe("session-b");
    expect(result.account.lastUsedAt).toBe(200);
  });

  it("两边都有不同 DSID 时即使邮箱相同也保持两个账号", () => {
    const first = account({ key: "dsid:111", dsid: "111" });
    const second = account({ key: "dsid:222", dsid: "222" });
    const result = upsertKnownAccount([first], second);

    expect(result.accounts).toHaveLength(2);
  });

  it("列表按最近使用时间倒序排列", () => {
    const older = account({ key: "apple:older@example.com", appleId: "older@example.com", lastUsedAt: 1 });
    const newer = account({ key: "apple:newer@example.com", appleId: "newer@example.com", lastUsedAt: 9 });
    const middle = account({ key: "apple:middle@example.com", appleId: "middle@example.com", lastUsedAt: 5 });

    expect(sortKnownAccounts([older, newer, middle]).map((item) => item.appleId)).toEqual(["newer@example.com", "middle@example.com", "older@example.com"]);
  });
});
