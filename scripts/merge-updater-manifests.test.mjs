import assert from "node:assert/strict";
import test from "node:test";

import { mergeUpdaterManifests, REQUIRED_UPDATER_PLATFORMS } from "./merge-updater-manifests.mjs";

const TAG = "1.5.0";

function platformEntry(name) {
  return {
    signature: `signature-${name}`,
    url: `https://github.com/zhukj125/cirrus/releases/download/${TAG}/${name}.bundle`,
  };
}

function fragment(platforms, overrides = {}) {
  return {
    version: "1.5.0",
    notes: "Release notes",
    pub_date: "2026-11-01T00:00:00.000Z",
    platforms: Object.fromEntries(platforms.map((name) => [name, platformEntry(name)])),
    ...overrides,
  };
}

test("merges complete platform fragments and keeps bundle-specific entries", () => {
  const merged = mergeUpdaterManifests(
    [
      { source: "mac-arm", manifest: fragment(["darwin-aarch64", "darwin-aarch64-app"]) },
      { source: "mac-intel", manifest: fragment(["darwin-x86_64", "darwin-x86_64-app"], { pub_date: "2026-11-01T00:05:00.000Z" }) },
      { source: "windows", manifest: fragment(["windows-x86_64", "windows-x86_64-nsis"]) },
      { source: "linux-x64", manifest: fragment(["linux-x86_64", "linux-x86_64-appimage", "linux-x86_64-deb"]) },
      { source: "linux-arm64", manifest: fragment(["linux-aarch64", "linux-aarch64-appimage", "linux-aarch64-deb"]) },
    ],
    "1.5.0",
    TAG,
  );

  assert.equal(merged.version, "1.5.0");
  assert.equal(merged.notes, "Release notes");
  assert.equal(merged.pub_date, "2026-11-01T00:05:00.000Z");
  for (const platform of REQUIRED_UPDATER_PLATFORMS) assert.ok(merged.platforms[platform], platform);
  assert.ok(merged.platforms["windows-x86_64-nsis"]);
  assert.ok(merged.platforms["darwin-aarch64-app"]);
  assert.ok(merged.platforms["linux-x86_64-deb"]);
});

test("rejects a manifest missing a required platform", () => {
  const platforms = REQUIRED_UPDATER_PLATFORMS.filter((name) => name !== "linux-aarch64");
  assert.throws(
    () => mergeUpdaterManifests([{ source: "fragment", manifest: fragment(platforms) }], "1.5.0", TAG),
    /missing required platforms: linux-aarch64/,
  );
});

test("rejects conflicting signatures for the same platform", () => {
  const first = fragment(["windows-x86_64"]);
  const second = fragment(["windows-x86_64"]);
  second.platforms["windows-x86_64"].signature = "different-signature";

  assert.throws(
    () => mergeUpdaterManifests([{ source: "one", manifest: first }, { source: "two", manifest: second }], "1.5.0", TAG),
    /conflicting signature/,
  );
});

test("rejects assets that do not point at the release tag", () => {
  const manifest = fragment(["windows-x86_64"]);
  manifest.platforms["windows-x86_64"].url = "https://github.com/zhukj125/cirrus/releases/download/1.4.8/windows.bundle";

  assert.throws(
    () => mergeUpdaterManifests([{ source: "fragment", manifest }], "1.5.0", TAG),
    /does not point to tag 1.5.0/,
  );
});
