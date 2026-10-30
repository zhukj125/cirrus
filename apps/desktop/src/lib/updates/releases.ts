export interface AppRelease {
  tag: string;
  version: string;
  name: string;
  date: string;
  body: string;
  url: string;
}

interface GitHubRelease {
  tag_name?: unknown;
  name?: unknown;
  published_at?: unknown;
  created_at?: unknown;
  body?: unknown;
  html_url?: unknown;
  draft?: unknown;
  prerelease?: unknown;
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

const PRIVATE_REFERENCE = String.fromCharCode(100, 98, 120);
const PRIVATE_REFERENCE_TOKEN = new RegExp(`\\b${PRIVATE_REFERENCE}\\b`, "gi");
const PRIVATE_REFERENCE_LINE = new RegExp(`^.*\\b${PRIVATE_REFERENCE}\\b.*(?:\\r?\\n|$)`, "gim");

function publicReleaseText(value: unknown): string {
  return text(value)
    .replace(PRIVATE_REFERENCE_TOKEN, "")
    .replace(/\s{2,}/g, " ")
    .trim();
}

export function sanitizeReleaseBody(value: unknown): string {
  return text(value)
    .replace(PRIVATE_REFERENCE_LINE, "")
    .replace(/\r\n/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export async function fetchAppReleases(): Promise<AppRelease[]> {
  const response = await fetch("https://api.github.com/repos/zhukj125/cirrus/releases?per_page=20", {
    headers: { Accept: "application/vnd.github+json" },
    signal: AbortSignal.timeout(12_000),
  });
  if (!response.ok) throw new Error(`GitHub releases returned HTTP ${response.status}`);
  const data = (await response.json()) as unknown;
  if (!Array.isArray(data)) return [];
  return data
    .filter((release): release is GitHubRelease => release != null && typeof release === "object")
    .filter((release) => release.draft !== true && release.prerelease !== true && text(release.tag_name))
    .map((release) => {
      const tag = text(release.tag_name);
      const version = tag.replace(/^v/, "");
      return {
        tag,
        version,
        name: publicReleaseText(release.name) || tag,
        date: text(release.published_at) || text(release.created_at),
        body: sanitizeReleaseBody(release.body),
        url: text(release.html_url) || `https://github.com/zhukj125/cirrus/releases/tag/${tag}`,
      };
    });
}

function numericVersionParts(version: string): number[] {
  return version
    .replace(/^v/, "")
    .split(/[.+-]/)
    .map((part) => Number.parseInt(part, 10))
    .map((part) => (Number.isFinite(part) ? part : 0));
}

export function compareVersions(left: string, right: string): number {
  const a = numericVersionParts(left);
  const b = numericVersionParts(right);
  const length = Math.max(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
}

export function isOlderVersion(version: string, currentVersion: string): boolean {
  return compareVersions(version, currentVersion) < 0;
}
