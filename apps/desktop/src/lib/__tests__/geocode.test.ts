import { afterEach, describe, expect, it, vi } from "vitest";

import { StorageKey } from "@/constants";
import { reverseGeocode } from "@/lib/geocode";

function response(body: unknown): Response {
  return { ok: true, json: async () => body } as Response;
}

describe("reverseGeocode", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it("retries after a failed lookup instead of keeping a null result cached forever", async () => {
    localStorage.setItem(StorageKey.amapKey, "test-key");
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(response({ status: "0", info: "NETWORK_ERROR", infocode: "00000" }))
      .mockResolvedValueOnce(
        response({
          status: "1",
          info: "OK",
          regeocode: {
            formatted_address: "浙江省杭州市西湖区",
            addressComponent: { province: "浙江省", city: "杭州市", district: "西湖区" },
          },
        }),
      );
    vi.stubGlobal("fetch", fetchMock);

    await expect(reverseGeocode([120.111111, 30.222222], "zh")).resolves.toBeNull();
    await expect(reverseGeocode([120.111111, 30.222222], "zh")).resolves.toEqual({
      full: "浙江省杭州市西湖区",
      short: "西湖区，杭州市",
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("caches a successful lookup", async () => {
    localStorage.setItem(StorageKey.amapKey, "test-key");
    const fetchMock = vi.fn().mockResolvedValue(
      response({
        status: "1",
        info: "OK",
        regeocode: {
          formatted_address: "浙江省杭州市上城区",
          addressComponent: { province: "浙江省", city: "杭州市", district: "上城区" },
        },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    await reverseGeocode([120.333333, 30.444444], "zh");
    await reverseGeocode([120.333333, 30.444444], "zh");
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
