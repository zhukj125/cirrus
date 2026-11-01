const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

type LocaleInput = string | boolean;

function localeValue(value: LocaleInput = "en-US"): string {
  if (typeof value === "boolean") return value ? "en-US" : "zh-CN";
  return value || "en-US";
}

function isEnglish(locale: string): boolean {
  return locale.startsWith("en");
}

function isChinese(locale: string): boolean {
  return locale.startsWith("zh");
}

function relativeFormatter(locale: string): Intl.RelativeTimeFormat {
  return new Intl.RelativeTimeFormat(locale, { numeric: "auto", style: "short" });
}

function shortElapsed(diff: number, locale: string): string | null {
  if (isEnglish(locale)) {
    if (diff < MINUTE) return "Now";
    if (diff < HOUR) return `${Math.floor(diff / MINUTE)} min ago`;
    if (diff < DAY) return `${Math.floor(diff / HOUR)} hr ago`;
    return null;
  }
  if (isChinese(locale)) {
    if (diff < MINUTE) return "刚刚";
    if (diff < HOUR) return `${Math.floor(diff / MINUTE)} 分钟前`;
    if (diff < DAY) return `${Math.floor(diff / HOUR)} 小时前`;
    return null;
  }
  const formatter = relativeFormatter(locale);
  if (diff < MINUTE) return formatter.format(0, "second");
  if (diff < HOUR) return formatter.format(-Math.floor(diff / MINUTE), "minute");
  if (diff < DAY) return formatter.format(-Math.floor(diff / HOUR), "hour");
  return null;
}

function shortDate(date: Date, locale: string): string {
  return date.toLocaleDateString(locale, { month: "short", day: "numeric" });
}

function shortTime(date: Date, locale: string): string {
  return date.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit", hour12: false });
}

export function relativeTimeLabel(timestamp: number, now = Date.now(), locale: LocaleInput = "en-US"): string {
  const value = localeValue(locale);
  const elapsed = shortElapsed(Math.max(0, now - timestamp), value);
  if (elapsed) return elapsed;
  return `${shortDate(new Date(timestamp), value)} ${shortTime(new Date(timestamp), value)}`;
}

export function recentTimeLabel(timestamp: number, now = Date.now(), locale: LocaleInput = "en-US"): string {
  const value = localeValue(locale);
  const diff = Math.max(0, now - timestamp);
  const elapsed = shortElapsed(diff, value);
  if (elapsed) return isEnglish(value) && elapsed === "Now" ? "Just now" : elapsed;
  if (diff < 7 * DAY) {
    const days = Math.floor(diff / DAY);
    if (isChinese(value)) return `${days} 天前`;
    if (!isEnglish(value)) return relativeFormatter(value).format(-days, "day");
    return `${days} days ago`;
  }
  return shortDate(new Date(timestamp), value);
}

export function staleTimeLabel(timestamp: number, now = new Date(), locale: LocaleInput = "en-US"): string {
  const value = localeValue(locale);
  const date = new Date(timestamp);
  const time = shortTime(date, value);
  const dayDifference = Math.round((new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime() - new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime()) / DAY);
  if (isEnglish(value)) {
    if (dayDifference === 0) return `Today ${time}`;
    if (dayDifference === 1) return `Yesterday ${time}`;
  } else if (isChinese(value)) {
    if (dayDifference === 0) return `今天 ${time}`;
    if (dayDifference === 1) return `昨天 ${time}`;
  } else {
    const formatter = new Intl.RelativeTimeFormat(value, { numeric: "auto" });
    if (dayDifference === 0) return `${formatter.format(0, "day")} ${time}`;
    if (dayDifference === 1) return `${formatter.format(-1, "day")} ${time}`;
  }
  return `${shortDate(date, value)} ${time}`;
}
