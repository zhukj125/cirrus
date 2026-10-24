import { describe, expect, it } from "vitest";

import { mapWithConcurrency } from "@/lib/photos/concurrency";

describe("照片分页 · 有界并发", () => {
  it("限制同时在飞的任务数，并保持输入顺序", async () => {
    let active = 0;
    let peak = 0;
    const results = await mapWithConcurrency([1, 2, 3, 4, 5, 6, 7], 3, async (value) => {
      active += 1;
      peak = Math.max(peak, active);
      await new Promise((resolve) => setTimeout(resolve, 2));
      active -= 1;
      return value * 10;
    });

    expect(peak).toBe(3);
    expect(results).toEqual([10, 20, 30, 40, 50, 60, 70]);
  });

  it("空输入不启动 worker", async () => {
    let calls = 0;
    const results = await mapWithConcurrency([], 4, async () => {
      calls += 1;
      return 1;
    });
    expect(results).toEqual([]);
    expect(calls).toBe(0);
  });
});
