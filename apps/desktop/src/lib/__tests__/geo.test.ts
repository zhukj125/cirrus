import { describe, expect, it } from "vitest";

import { gcj02ToWgs84, wgs84ToGcj02 } from "@/lib/geo";

describe("wgs84ToGcj02", () => {
  it("境外坐标原样返回（不做任何偏移）", () => {
    expect(wgs84ToGcj02(-122.4194, 37.7749)).toEqual([-122.4194, 37.7749]);

    expect(wgs84ToGcj02(10, 45)).toEqual([10, 45]);
  });

  it("国内坐标会产生偏移，且量级在几百米内", () => {
    const [lng, lat] = wgs84ToGcj02(116.397, 39.909);

    expect(lng).not.toBeCloseTo(116.397, 5);
    expect(lat).not.toBeCloseTo(39.909, 5);

    const dLng = Math.abs(lng - 116.397);
    const dLat = Math.abs(lat - 39.909);
    expect(dLng).toBeGreaterThan(0.0005);
    expect(dLng).toBeLessThan(0.02);
    expect(dLat).toBeGreaterThan(0.0005);
    expect(dLat).toBeLessThan(0.02);
  });

  it("同一输入始终得到同一输出", () => {
    const first = wgs84ToGcj02(121.4737, 31.2304);
    const second = wgs84ToGcj02(121.4737, 31.2304);
    expect(first).toEqual(second);
  });

  it("GCJ-02 可以近似还原为 WGS84", () => {
    const original: [number, number] = [121.4737, 31.2304];
    const gcj = wgs84ToGcj02(...original);
    const restored = gcj02ToWgs84(...gcj);
    expect(restored[0]).toBeCloseTo(original[0], 5);
    expect(restored[1]).toBeCloseTo(original[1], 5);
  });
  it("相邻输入得到相邻输出（没有分段跳变）", () => {
    const [lngA, latA] = wgs84ToGcj02(121.4737, 31.2304);
    const [lngB, latB] = wgs84ToGcj02(121.4747, 31.2314);
    expect(Math.abs(lngB - lngA)).toBeLessThan(0.002);
    expect(Math.abs(latB - latA)).toBeLessThan(0.002);
  });
});
