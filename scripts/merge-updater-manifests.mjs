import { readFile, readdir, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

export const REQUIRED_UPDATER_PLATFORMS = [
  "windows-x86_64",
  "darwin-aarch64",
  "darwin-x86_64",
  "linux-x86_64",
  "linux-aarch64",
];

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function pickNotes(manifests) {
  const notes = [...new Set(manifests.map(({ manifest }) => manifest.notes).filter((value) => typeof value === "string" && value.trim()))];
  assert(notes.length > 0, "No updater manifest contains release notes");
  assert(notes.length === 1, "Updater manifest fragments contain different release notes");
  return notes[0];
}

function pickLatestDate(manifests) {
  const timestamps = manifests
    .map(({ manifest }) => Date.parse(manifest.pub_date))
    .filter((value) => Number.isFinite(value));
  assert(timestamps.length > 0, "No updater manifest contains a valid pub_date");
  return new Date(Math.max(...timestamps)).toISOString();
}

export function mergeUpdaterManifests(fragments, expectedVersion, expectedTag) {
  assert(Array.isArray(fragments) && fragments.length > 0, "No updater manifest fragments found");

  const manifests = fragments.map(({ source, manifest }) => {
    assert(manifest && typeof manifest === "object", `${source} is not an updater manifest`);
    assert(manifest.version === expectedVersion, `${source} has version ${manifest.version}, expected ${expectedVersion}`);
    assert(manifest.platforms && typeof manifest.platforms === "object", `${source} has no platforms object`);
    return { source, manifest };
  });

  const platforms = new Map();
  const downloadPrefix = `/releases/download/${expectedTag}/`;

  for (const { source, manifest } of manifests) {
    for (const [platform, entry] of Object.entries(manifest.platforms)) {
      assert(entry && typeof entry === "object", `${source}: platform ${platform} is invalid`);
      const signature = entry.signature;
      const url = entry.url;
      assert(typeof signature === "string" && signature.length > 0, `${source}: platform ${platform} has no signature`);
      assert(typeof url === "string" && url.length > 0, `${source}: platform ${platform} has no URL`);

      let parsed;
      try {
        parsed = new URL(url);
      } catch {
        throw new Error(`${source}: platform ${platform} has an invalid URL`);
      }
      assert(parsed.protocol === "https:", `${source}: platform ${platform} must use HTTPS`);
      assert(parsed.pathname.includes(downloadPrefix), `${source}: platform ${platform} does not point to tag ${expectedTag}`);

      const next = { signature, url };
      const previous = platforms.get(platform);
      if (previous) {
        assert(previous.signature === next.signature, `${source}: platform ${platform} has a conflicting signature`);
        assert(previous.url === next.url, `${source}: platform ${platform} has a conflicting URL`);
      } else {
        platforms.set(platform, next);
      }
    }
  }

  const missing = REQUIRED_UPDATER_PLATFORMS.filter((platform) => !platforms.has(platform));
  assert(missing.length === 0, `Updater manifest is missing required platforms: ${missing.join(", ")}`);

  return {
    version: expectedVersion,
    notes: pickNotes(manifests),
    pub_date: pickLatestDate(manifests),
    platforms: Object.fromEntries([...platforms.entries()].sort(([left], [right]) => left.localeCompare(right))),
  };
}

async function findManifestFiles(directory) {
  const found = [];
  const entries = await readdir(directory, { withFileTypes: true });
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      found.push(...(await findManifestFiles(path)));
    } else if (entry.isFile() && basename(entry.name) === "latest.json") {
      found.push(path);
    }
  }
  return found.sort();
}

async function main() {
  const [fragmentsDirectory, outputPath, tag] = process.argv.slice(2);
  assert(fragmentsDirectory && outputPath && tag, "Usage: merge-updater-manifests.mjs <fragments-dir> <output-file> <tag>");

  const files = await findManifestFiles(resolve(fragmentsDirectory));
  assert(files.length > 0, `No latest.json fragments found in ${fragmentsDirectory}`);

  const fragments = await Promise.all(
    files.map(async (source) => ({ source, manifest: JSON.parse(await readFile(source, "utf8")) })),
  );
  const expectedVersion = tag.replace(/^v/, "");
  const merged = mergeUpdaterManifests(fragments, expectedVersion, tag);
  await writeFile(resolve(outputPath), `${JSON.stringify(merged, null, 2)}\n`, "utf8");

  console.log(`Merged ${files.length} updater fragments into ${outputPath}`);
  console.log(`Verified ${REQUIRED_UPDATER_PLATFORMS.length} required platforms for ${expectedVersion}`);
  console.log(`Manifest platforms: ${Object.keys(merged.platforms).join(", ")}`);
}

const isMain = process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url;
if (isMain) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exit(1);
  });
}
