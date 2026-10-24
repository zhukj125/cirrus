import { describe, expect, it } from "vitest";

import { isSessionGoneMessage } from "@/lib/session-errors";

describe("isSessionGoneMessage", () => {
  it("detects expired-session responses", () => {
    expect(isSessionGoneMessage("Session expired. Please sign in again.")).toBe(true);
    expect(isSessionGoneMessage("HTTP 421 invalid global session")).toBe(true);
    expect(isSessionGoneMessage("Missing session ID. Please sign in again.")).toBe(true);
  });

  it("ignores unrelated failures", () => {
    expect(isSessionGoneMessage("Network request failed")).toBe(false);
    expect(isSessionGoneMessage(null)).toBe(false);
  });
});
