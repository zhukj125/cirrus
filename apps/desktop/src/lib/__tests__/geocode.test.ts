import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const geocodeMock = vi.hoisted(() => ({ getAddress: vi.fn() }));
const backendMock = vi.hoisted(() => vi.fn());

vi.mock("@/lib/api", () => ({
  reverseGeocodeBackend: backendMock,
}));
vi.mock("@/lib/amap", () => ({
  getAmapConfig: vi.fn(() => ({ key: "test-key", securityCode: "" })),
  loadAmap: vi.fn(async () => ({
    Geocoder: class {
      getAddress(lnglat: [number, number], callback: (status: string, result: unknown) => void) {
        geocodeMock.getAddress(lnglat, callback);
      }
    },
  })),
}));

import { reverseGeocode } from "@/lib/geocode";

describe("reverseGeocode", () => {
  beforeEach(() => {
    geocodeMock.getAddress.mockReset();
    backendMock.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("uses the backend provider before waiting for the AMap JS callback", async () => {
    backendMock.mockResolvedValue({ full: "浙江省杭州市拱墅区", short: "拱墅区，杭州市" });

    await expect(reverseGeocode([120.555555, 30.666666], "zh")).resolves.toEqual({
      full: "浙江省杭州市拱墅区",
      short: "拱墅区，杭州市",
    });
    expect(backendMock).toHaveBeenCalledOnce();
    expect(geocodeMock.getAddress).not.toHaveBeenCalled();
  });

  it("falls back to AMap JS when the backend has no address", async () => {
    backendMock.mockResolvedValueOnce(null);
    geocodeMock.getAddress.mockImplementation((_lnglat, callback) =>
      callback("complete", {
        regeocode: {
          formattedAddress: "浙江省杭州市西湖区",
          addressComponent: { province: "浙江省", city: "杭州市", district: "西湖区" },
        },
      }),
    );

    await expect(reverseGeocode([120.111111, 30.222222], "zh")).resolves.toEqual({
      full: "浙江省杭州市西湖区",
      short: "西湖区，杭州市",
    });
    expect(backendMock).toHaveBeenCalledOnce();
    expect(geocodeMock.getAddress).toHaveBeenCalledOnce();
  });

  it("does not hang when the AMap JS callback never returns", async () => {
    vi.useFakeTimers();
    backendMock.mockResolvedValueOnce(null);
    geocodeMock.getAddress.mockImplementation(() => undefined);

    const request = reverseGeocode([120.777777, 30.888888], "zh");
    await vi.runAllTicks();
    await vi.advanceTimersByTimeAsync(8_000);

    await expect(request).resolves.toBeNull();
  });

  it("caches a successful backend lookup", async () => {
    backendMock.mockResolvedValue({ full: "浙江省杭州市上城区", short: "上城区，杭州市" });

    await reverseGeocode([120.333333, 30.444444], "zh");
    await reverseGeocode([120.333333, 30.444444], "zh");

    expect(backendMock).toHaveBeenCalledOnce();
  });
});
