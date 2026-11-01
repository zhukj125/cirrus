import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";

import arSA from "@/locales/ar-SA.json";
import deDE from "@/locales/de-DE.json";
import enUS from "@/locales/en-US.json";
import esES from "@/locales/es-ES.json";
import frFR from "@/locales/fr-FR.json";
import jaJP from "@/locales/ja-JP.json";
import koKR from "@/locales/ko-KR.json";
import ptBR from "@/locales/pt-BR.json";
import ruRU from "@/locales/ru-RU.json";
import zhCN from "@/locales/zh-CN.json";
import zhTW from "@/locales/zh-TW.json";
import { StorageKey } from "@/constants";
import { readStored, writeStored } from "@/lib/common/storage";

export type Locale = "zh-CN" | "zh-TW" | "en-US" | "ja-JP" | "ko-KR" | "es-ES" | "fr-FR" | "de-DE" | "pt-BR" | "ru-RU" | "ar-SA";

export interface LocaleOption {
  value: Locale;
  label: string;
  shortLabel: string;
}

export const LOCALE_OPTIONS: readonly LocaleOption[] = [
  { value: "zh-CN", label: "简体中文", shortLabel: "简" },
  { value: "zh-TW", label: "繁體中文", shortLabel: "繁" },
  { value: "en-US", label: "English", shortLabel: "EN" },
  { value: "ja-JP", label: "日本語", shortLabel: "日" },
  { value: "ko-KR", label: "한국어", shortLabel: "한" },
  { value: "es-ES", label: "Español", shortLabel: "ES" },
  { value: "fr-FR", label: "Français", shortLabel: "FR" },
  { value: "de-DE", label: "Deutsch", shortLabel: "DE" },
  { value: "pt-BR", label: "Português (Brasil)", shortLabel: "PT" },
  { value: "ru-RU", label: "Русский", shortLabel: "RU" },
  { value: "ar-SA", label: "العربية", shortLabel: "AR" },
];

const MESSAGES: Record<Locale, Record<string, string>> = {
  "zh-CN": zhCN,
  "zh-TW": zhTW,
  "en-US": enUS,
  "ja-JP": jaJP,
  "ko-KR": koKR,
  "es-ES": esES,
  "fr-FR": frFR,
  "de-DE": deDE,
  "pt-BR": ptBR,
  "ru-RU": ruRU,
  "ar-SA": arSA,
};

const FALLBACK_LOCALE: Locale = "en-US";
const RTL_LOCALES = new Set<Locale>(["ar-SA"]);

function matchLocale(value: string): Locale | null {
  const normalized = value.trim().replaceAll("_", "-").toLowerCase();
  if (!normalized) return null;
  if (/^zh-(tw|hk|mo)\b/.test(normalized) || normalized.includes("hant")) return "zh-TW";
  if (/^zh\b/.test(normalized)) return "zh-CN";
  if (/^en\b/.test(normalized)) return "en-US";
  if (/^ja\b/.test(normalized)) return "ja-JP";
  if (/^ko\b/.test(normalized)) return "ko-KR";
  if (/^es\b/.test(normalized)) return "es-ES";
  if (/^fr\b/.test(normalized)) return "fr-FR";
  if (/^de\b/.test(normalized)) return "de-DE";
  if (/^pt\b/.test(normalized)) return "pt-BR";
  if (/^ru\b/.test(normalized)) return "ru-RU";
  if (/^ar\b/.test(normalized)) return "ar-SA";
  return null;
}

function detectLocale(): Locale {
  if (typeof navigator === "undefined") return FALLBACK_LOCALE;
  const languages = navigator.languages?.length ? navigator.languages : [navigator.language];
  for (const language of languages) {
    const matched = matchLocale(language);
    if (matched) return matched;
  }
  return FALLBACK_LOCALE;
}

function initialLocale(): Locale {
  const stored = readStored(StorageKey.locale);
  return (stored ? matchLocale(stored) : null) ?? detectLocale();
}

function applyDocumentLocale(value: Locale): void {
  if (typeof document === "undefined") return;
  document.documentElement.lang = value;
  document.documentElement.dir = RTL_LOCALES.has(value) ? "rtl" : "ltr";
}

export function isLocale(value: string): value is Locale {
  return LOCALE_OPTIONS.some((option) => option.value === value);
}

export const useLocaleStore = defineStore("locale", () => {
  const locale = ref<Locale>(initialLocale());
  const option = computed(() => LOCALE_OPTIONS.find((item) => item.value === locale.value) ?? LOCALE_OPTIONS[1]);
  const formatLocale = computed(() => locale.value);
  const isEnglish = computed(() => locale.value === "en-US");
  const isChinese = computed(() => locale.value === "zh-CN" || locale.value === "zh-TW");
  const label = computed(() => option.value.shortLabel);

  applyDocumentLocale(locale.value);
  watch(locale, applyDocumentLocale);

  function setLocale(next: Locale): void {
    locale.value = next;
    writeStored(StorageKey.locale, next);
  }

  function toggle(): void {
    const current = LOCALE_OPTIONS.findIndex((item) => item.value === locale.value);
    setLocale(LOCALE_OPTIONS[(current + 1) % LOCALE_OPTIONS.length]?.value ?? FALLBACK_LOCALE);
  }

  function t(key: string, params?: Record<string, string | number>): string {
    const message = MESSAGES[locale.value][key] ?? MESSAGES[FALLBACK_LOCALE][key] ?? key;
    if (!params) return message;
    return Object.entries(params).reduce((value, [name, replacement]) => value.replaceAll(`{${name}}`, String(replacement)), message);
  }

  return { locale, formatLocale, isEnglish, isChinese, label, setLocale, toggle, t };
});
