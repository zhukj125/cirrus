import { describe, expect, it } from "vitest";

import { recentTimeLabel, relativeTimeLabel, staleTimeLabel } from "@/lib/time";

const NOW = new Date(2026, 9, 24, 15, 30, 0).getTime();

describe("time labels", () => {
  it("formats recent locations", () => {
    expect(relativeTimeLabel(NOW - 30_000, NOW)).toBe("Now");
    expect(relativeTimeLabel(NOW - 5 * 60_000, NOW)).toBe("5 min ago");
    expect(relativeTimeLabel(NOW - 2 * 3_600_000, NOW)).toBe("2 hr ago");
  });

  it("formats account recency", () => {
    expect(recentTimeLabel(NOW - 10_000, NOW)).toBe("Just now");
    expect(recentTimeLabel(NOW - 3 * 86_400_000, NOW)).toBe("3 days ago");
  });

  it("labels stale locations by day", () => {
    const today = new Date(2026, 9, 24, 9, 5).getTime();
    const yesterday = new Date(2026, 9, 23, 8, 10).getTime();
    expect(staleTimeLabel(today, new Date(NOW))).toBe("Today 09:05");
    expect(staleTimeLabel(yesterday, new Date(NOW))).toBe("Yesterday 08:10");
  });
});
