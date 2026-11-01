import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { nextTick } from "vue";

import { StorageKey } from "@/constants";
import { LOCALE_OPTIONS, useLocaleStore } from "@/stores/locale";

describe("interface languages", () => {
  beforeEach(() => {
    localStorage.clear();
    setActivePinia(createPinia());
  });

  afterEach(() => {
    document.documentElement.lang = "en";
    document.documentElement.dir = "ltr";
  });

  it("provides translated navigation labels for every supported language", () => {
    const locale = useLocaleStore();
    for (const option of LOCALE_OPTIONS) {
      locale.setLocale(option.value);
      expect(locale.t("nav.devices"), option.value).not.toBe("nav.devices");
      expect(locale.t("settings.language"), option.value).not.toBe("settings.language");
    }
  });

  it("persists the selection and applies document language and direction", async () => {
    const locale = useLocaleStore();
    locale.setLocale("ar-SA");
    await nextTick();

    expect(localStorage.getItem(StorageKey.locale)).toBe("ar-SA");
    expect(document.documentElement.lang).toBe("ar-SA");
    expect(document.documentElement.dir).toBe("rtl");

    locale.setLocale("ja-JP");
    await nextTick();
    expect(document.documentElement.lang).toBe("ja-JP");
    expect(document.documentElement.dir).toBe("ltr");
  });

  it("keeps placeholders intact across translations", () => {
    const locale = useLocaleStore();
    locale.setLocale("fr-FR");
    expect(locale.t("tabs.close", { title: "Cirrus" })).toContain("Cirrus");
    expect(locale.t("unknown.key")).toBe("unknown.key");
  });

  it("keeps every locale complete and preserves placeholders", () => {
    const modules = import.meta.glob("../../locales/*.json", { eager: true, import: "default" }) as Record<string, Record<string, string>>;
    const messages = Object.fromEntries(Object.entries(modules).map(([path, value]) => [path.split("/").pop()?.replace(".json", "") ?? path, value]));
    const english = messages["en-US"];
    expect(english).toBeDefined();
    const keys = Object.keys(english ?? {}).sort();
    const placeholders = (value: string) => [...value.matchAll(/\{[A-Za-z0-9_]+\}/g)].map((match) => match[0]).sort();

    for (const [locale, values] of Object.entries(messages)) {
      expect(Object.keys(values).sort(), locale).toEqual(keys);
      for (const key of keys) expect(placeholders(values[key] ?? ""), `${locale}:${key}`).toEqual(placeholders(english?.[key] ?? ""));
    }
  });
});
