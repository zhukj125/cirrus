import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const geocodeMock = vi.hoisted(() => ({ getAddress: vi.fn() }));

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

function response(body: unknown): Response {
  return { ok: true, json: async () => body } as Response;
}

describe("reverseGeocode", () => {
  beforeEach(() => {
    geocodeMock.getAddress.mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("retries with AMap Geocoder after a failed lookup instead of caching null forever", async () => {
    geocodeMock.getAddress
      .mockImplementationOnce((_lnglat, callback) => callback("error", null))
      .mockImplementationOnce((_lnglat, callback) =>
        callback("complete", {
          regeocode: {
            formattedAddress: "浙江省杭州市西湖区",
            addressComponent: { province: "浙江省", city: "杭州市", district: "西湖区" },
          },
        }),
      );
    const fetchMock = vi.fn().mockResolvedValue(response({ status: "0", info: "NETWORK_ERROR", infocode: "00000" }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(reverseGeocode([120.111111, 30.222222], "zh")).resolves.toBeNull();
    await expect(reverseGeocode([120.111111, 30.222222], "zh")).resolves.toEqual({
      full: "浙江省杭州市西湖区",
      short: "西湖区，杭州市",
    });
    expect(geocodeMock.getAddress).toHaveBeenCalledTimes(2);
  });

  it("caches a successful AMap Geocoder lookup", async () => {
    geocodeMock.getAddress.mockImplementation((_lnglat, callback) =>
      callback("complete", {
        regeocode: {
          formattedAddress: "浙江省杭州市上城区",
          addressComponent: { province: "浙江省", city: "杭州市", district: "上城区" },
        },
      }),
    );

    await reverseGeocode([120.333333, 30.444444], "zh");
    await reverseGeocode([120.333333, 30.444444], "zh");
    expect(geocodeMock.getAddress).toHaveBeenCalledTimes(1);
  });
});
