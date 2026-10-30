import { afterEach, describe, expect, it, vi } from "vitest";

import { fetchAppReleases } from "@/lib/updates/releases";

describe("fetchAppReleases", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("removes internal implementation references from public release notes", async () => {
    const privateReference = String.fromCharCode(100, 98, 120);
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          ({
            ok: true,
            json: async () => [
              {
                tag_name: "1.4.7",
                name: `Cirrus ${privateReference}`,
                body: `- Keep this change\n- Compare with ${privateReference}\n- Keep this too`,
                html_url: "https://example.com/release",
              },
            ],
          }) as Response,
      ),
    );

    const [release] = await fetchAppReleases();

    expect(release?.name).toBe("Cirrus");
    expect(release?.body).toBe("- Keep this change\n- Keep this too");
  });
});
