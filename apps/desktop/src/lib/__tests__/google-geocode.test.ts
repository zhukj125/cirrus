import { beforeEach, describe, expect, it, vi } from "vitest";

import { StorageKey } from "@/constants";

const googleMapsMock = vi.hoisted(() => ({
  load: vi.fn(),
  geocode: vi.fn(),
}));

vi.mock("@/lib/maps/google-maps", () => ({
  loadGoogleMaps: googleMapsMock.load,
  hasEnvironmentGoogleMapsKey: () => false,
}));

import { reverseGeocode } from "@/lib/geocode";

describe("Google reverse geocoding", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem(StorageKey.mapProvider, "google");
    googleMapsMock.geocode.mockReset();
    googleMapsMock.load.mockReset();
    googleMapsMock.load.mockResolvedValue({
      Geocoder: class {
        geocode(request: unknown, callback: (results: unknown[], status: string) => void) {
          googleMapsMock.geocode(request, callback);
        }
      },
    });
  });

  it("uses Google's WGS84 location and address components", async () => {
    googleMapsMock.geocode.mockImplementation((_request, callback) =>
      callback(
        [
          {
            formatted_address: "1 Market St, San Francisco, CA 94105, USA",
            address_components: [
              { long_name: "1", types: ["street_number"] },
              { long_name: "Market Street", types: ["route"] },
              { long_name: "San Francisco", types: ["locality"] },
              { long_name: "California", types: ["administrative_area_level_1"] },
            ],
          },
        ],
        "OK",
      ),
    );

    await expect(reverseGeocode([-122.4194, 37.7749], "en")).resolves.toEqual({
      full: "1 Market St, San Francisco, CA 94105, USA",
      short: "1 Market Street, San Francisco",
    });
    expect(googleMapsMock.geocode).toHaveBeenCalledWith({ location: { lat: 37.7749, lng: -122.4194 }, language: "en" }, expect.any(Function));
  });
});
