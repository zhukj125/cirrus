import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import vm from "node:vm";
import { afterEach, describe, expect, it } from "vitest";

import { isBlockingCompatFailure } from "../ui/webview-compat";

const indexHtml = readFileSync(resolve(process.cwd(), "apps/desktop/index.html"), "utf8");
const globalsCss = readFileSync(resolve(process.cwd(), "apps/desktop/src/styles/globals.css"), "utf8");

function inlineScript(marker: string): string {
  const start = indexHtml.indexOf(marker);
  expect(start).toBeGreaterThan(-1);
  const contentStart = indexHtml.indexOf(">", start) + 1;
  const end = indexHtml.indexOf("</script>", contentStart);
  expect(end).toBeGreaterThan(contentStart);
  return indexHtml.slice(contentStart, end);
}

const probeSource = inlineScript("<script data-cirrus-compat-probe>");
const polyfillSource = inlineScript("<script data-cirrus-polyfills>");

type Capabilities = {
  layer: boolean;
  oklch: boolean;
  colorMix: boolean;
  mediaRange: boolean;
  has: boolean;
  focusVisible: boolean;
};

const SAFARI_15_2: Capabilities = { layer: false, oklch: false, colorMix: false, mediaRange: false, has: false, focusVisible: false };
const SAFARI_16_2: Capabilities = { layer: true, oklch: true, colorMix: true, mediaRange: false, has: true, focusVisible: true };
const CHROME_120: Capabilities = { layer: true, oklch: true, colorMix: true, mediaRange: true, has: true, focusVisible: true };
const SAFARI_15_2_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/15.2 Safari/605.1.15";
const SAFARI_16_2_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.2 Safari/605.1.15";
const CHROME_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

class FakeElement {
  tagName: string;
  style: Record<string, string> = {};
  textContent = "";
  parentNode: FakeElement | null = null;
  children: FakeElement[] = [];
  attributes: Record<string, string> = {};
  className = "";
  private classes = new Set<string>();
  classList = {
    contains: (name: string) => this.classes.has(name),
    add: (name: string) => this.classes.add(name),
    remove: (name: string) => this.classes.delete(name),
    toggle: (name: string, force?: boolean) => {
      if (force === false) this.classes.delete(name);
      else if (force === true) this.classes.add(name);
      else if (this.classes.has(name)) this.classes.delete(name);
      else this.classes.add(name);
    },
  };

  constructor(tagName: string) {
    this.tagName = tagName;
  }

  appendChild(node: FakeElement): FakeElement {
    node.parentNode = this;
    this.children.push(node);
    return node;
  }

  removeChild(node: FakeElement): FakeElement {
    const index = this.children.indexOf(node);
    if (index >= 0) this.children.splice(index, 1);
    node.parentNode = null;
    return node;
  }

  remove(): void {
    this.parentNode?.removeChild(this);
  }

  addEventListener(): void {}

  setAttribute(name: string, value: string): void {
    this.attributes[name] = String(value);
  }

  removeAttribute(name: string): void {
    delete this.attributes[name];
  }
}

function textOf(node: FakeElement): string {
  return [node.textContent, ...node.children.map(textOf)].join(" ");
}

function storageStub(initial: Record<string, string> = {}) {
  const values = { ...initial };
  return {
    values,
    getItem: (key: string) => (key in values ? values[key] : null),
    setItem: (key: string, value: string) => {
      values[key] = String(value);
    },
  };
}

type ProbeOptions = {
  capabilities: Capabilities;
  userAgent: string;
  desktop?: boolean;
  session?: Record<string, string>;
  local?: Record<string, string>;
  languages?: string[];
  throwOnStyle?: boolean;
};

function runProbe(options: ProbeOptions) {
  const documentElement = new FakeElement("html");
  const head = new FakeElement("head");
  const body = new FakeElement("body");
  const session = storageStub(options.session);
  const local = storageStub(options.local);
  const warnings: string[] = [];

  const windowStub: Record<string, any> = {
    location: { search: "", reload: () => {} },
    sessionStorage: session,
    localStorage: local,
    matchMedia: (query: string) => ({ matches: options.capabilities.mediaRange && query.includes(">=") }),
    getComputedStyle: () => {
      if (options.throwOnStyle) throw new Error("style probe failed");
      return { getPropertyValue: (name: string) => (name === "--cirrus-layer-probe" && options.capabilities.layer ? "1" : "") };
    },
  };
  if (options.desktop) windowStub.__TAURI_INTERNALS__ = {};

  const css = {
    supports: (propertyOrCondition: string, value?: string) => {
      if (value === undefined) {
        if (propertyOrCondition.includes(":has(")) return options.capabilities.has;
        if (propertyOrCondition.includes(":focus-visible")) return options.capabilities.focusVisible;
        return true;
      }
      if (value.includes("oklch(")) return options.capabilities.oklch;
      if (value.includes("color-mix(")) return options.capabilities.colorMix;
      return true;
    },
  };

  const context = vm.createContext({
    window: windowStub,
    document: {
      documentElement,
      head,
      body,
      createElement: (tag: string) => new FakeElement(tag),
    },
    navigator: {
      userAgent: options.userAgent,
      platform: options.userAgent.includes("Mac") ? "MacIntel" : "Win32",
      language: options.languages?.[0] ?? "en",
      languages: options.languages ?? ["en"],
    },
    CSS: css,
    URLSearchParams,
    console: { warn: (...args: unknown[]) => warnings.push(args.map(String).join(" ")) },
  });

  vm.runInContext(probeSource, context, { filename: "index.html#cirrus-compat-probe" });

  const report = windowStub.__CIRRUS_WEBVIEW_COMPAT__;
  const notices = body.children.filter((child) => "data-cirrus-compat-gate" in child.attributes || "data-cirrus-compat-banner" in child.attributes);
  return {
    report,
    blocked: windowStub.__CIRRUS_COMPAT_BLOCKED__,
    legacyClass: documentElement.classList.contains("cirrus-legacy-webview"),
    recorded: documentElement.attributes["data-cirrus-compat"],
    engine: documentElement.attributes["data-cirrus-engine"],
    notice: notices.length === 0 ? "none" : "data-cirrus-compat-gate" in notices[0].attributes ? "gate" : "banner",
    noticeText: notices.length === 0 ? "" : textOf(notices[0]),
    warnings,
  };
}

afterEach(() => {
  delete window.__CIRRUS_WEBVIEW_COMPAT__;
  delete window.__CIRRUS_COMPAT_BLOCKED__;
});

describe("WebView compatibility guard", () => {
  it("blocks an engine that cannot apply the layered stylesheet", () => {
    const result = runProbe({ capabilities: SAFARI_15_2, userAgent: SAFARI_15_2_UA, desktop: true, languages: ["zh-CN"] });
    expect(result.report.tier, result.warnings.join("\n")).toBe("fatal");
    expect(result.notice).toBe("gate");
    expect(result.noticeText).toContain("WebKit 版本过旧");
    expect(result.noticeText).toContain("Safari");
    expect(result.legacyClass).toBe(true);
    expect(result.blocked).toBe(true);
  });

  it("shows a dismissible banner when only optional CSS capabilities are missing", () => {
    const result = runProbe({ capabilities: SAFARI_16_2, userAgent: SAFARI_16_2_UA, desktop: true, languages: ["en"] });
    expect(result.report.tier, result.warnings.join("\n")).toBe("degraded");
    expect(result.notice).toBe("banner");
    expect(result.noticeText).toContain("Safari 16.2");
    expect(result.noticeText).toContain("Some styles may be incomplete");
    expect(result.legacyClass).toBe(true);
    expect(result.blocked).toBe(false);
  });

  it("stays out of the way on a supported engine", () => {
    const result = runProbe({ capabilities: CHROME_120, userAgent: CHROME_UA, desktop: true });
    expect(result.report.tier).toBe("ok");
    expect(result.notice).toBe("none");
    expect(result.recorded).toBe("ok");
    expect(result.legacyClass).toBe(false);
    expect(result.blocked).toBe(false);
  });

  it("allows a session-only bypass without hiding the compatibility report", () => {
    const result = runProbe({
      capabilities: SAFARI_15_2,
      userAgent: SAFARI_15_2_UA,
      desktop: true,
      session: { "cirrus.compat.bypass": "1" },
    });
    expect(result.report.tier, result.warnings.join("\n")).toBe("fatal");
    expect(result.report.bypassed).toBe(true);
    expect(result.notice).toBe("none");
    expect(result.legacyClass).toBe(true);
    expect(result.blocked).toBe(false);
  });

  it("fails open when a probe throws", () => {
    const result = runProbe({ capabilities: CHROME_120, userAgent: CHROME_UA, throwOnStyle: true });
    expect(result.report.tier).toBe("ok");
    expect(result.notice).toBe("none");
  });

  it("reads blocking state from the startup report", () => {
    window.__CIRRUS_WEBVIEW_COMPAT__ = { tier: "fatal", missing: ["layer"], engine: "Safari 15.2", platform: "macos", desktop: true, bypassed: false };
    expect(isBlockingCompatFailure()).toBe(true);

    window.__CIRRUS_WEBVIEW_COMPAT__ = { tier: "fatal", missing: ["layer"], engine: "Safari 15.2", platform: "macos", desktop: true, bypassed: true };
    expect(isBlockingCompatFailure()).toBe(false);

    window.__CIRRUS_WEBVIEW_COMPAT__ = { tier: "degraded", missing: ["media-query-range"], engine: "Safari 16.2", platform: "macos", desktop: true, bypassed: false };
    expect(isBlockingCompatFailure()).toBe(false);
  });
});

describe("startup compatibility assets", () => {
  it("runs the probe and polyfills before the application bundle", () => {
    const bundle = indexHtml.indexOf('src="/src/main.ts"');
    expect(indexHtml.indexOf("data-cirrus-startup-guard")).toBeLessThan(bundle);
    expect(indexHtml.indexOf("data-cirrus-polyfills")).toBeLessThan(bundle);
    expect(indexHtml.indexOf("data-cirrus-compat-probe")).toBeLessThan(bundle);
  });

  it("keeps the compatibility notice independent of the application stylesheet", () => {
    expect(probeSource).toContain("style.cssText");
    expect(probeSource).not.toContain("var(--");
    expect(probeSource).not.toContain("@media");
  });

  it("provides the modern built-ins used by the frontend bundle", () => {
    const context = vm.createContext({});
    vm.runInContext("delete Object.hasOwn; delete Array.prototype.findLast; delete Array.prototype.toReversed; delete Array.prototype.toSorted; delete Array.prototype.toSpliced; delete String.prototype.replaceAll;", context);
    vm.runInContext(polyfillSource, context, { filename: "index.html#cirrus-polyfills" });
    const result = JSON.parse(
      vm.runInContext(
        `JSON.stringify({
          hasOwn: Object.hasOwn({ value: 1 }, "value"),
          findLast: [1, 2, 3].findLast((value) => value < 3),
          replaceAll: "a-b-c".replaceAll("-", "_"),
          reversed: [1, 2, 3].toReversed(),
          sorted: [3, 1, 2].toSorted((a, b) => a - b),
          spliced: [1, 2, 3].toSpliced(1, 1, 9)
        })`,
        context,
      ),
    );
    expect(result).toEqual({ hasOwn: true, findLast: 2, replaceAll: "a_b_c", reversed: [3, 2, 1], sorted: [1, 2, 3], spliced: [1, 9, 3] });
  });

  it("ships solid token fallbacks for legacy WebKit", () => {
    expect(globalsCss).toContain("html.cirrus-legacy-webview {");
    expect(globalsCss).toContain("--border: rgba(0, 0, 0, 0.09)");
    expect(globalsCss).toContain("html.cirrus-legacy-webview.dark");
  });
});
