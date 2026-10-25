const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

function shortElapsed(diff: number, english: boolean): string | null {
  if (diff < MINUTE) return english ? "Now" : "刚刚";
  if (diff < HOUR) return english ? `${Math.floor(diff / MINUTE)} min ago` : `${Math.floor(diff / MINUTE)} 分钟前`;
  if (diff < DAY) return english ? `${Math.floor(diff / HOUR)} hr ago` : `${Math.floor(diff / HOUR)} 小时前`;
  return null;
}

function shortDate(date: Date, english: boolean): string {
  return date.toLocaleDateString(english ? "en-US" : "zh-CN", { month: english ? "short" : "numeric", day: "numeric" });
}

function shortTime(date: Date, english: boolean): string {
  return date.toLocaleTimeString(english ? "en-US" : "zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false });
}

export function relativeTimeLabel(timestamp: number, now = Date.now(), english = true): string {
  const elapsed = shortElapsed(Math.max(0, now - timestamp), english);
  if (elapsed) return elapsed;
  return `${shortDate(new Date(timestamp), english)} ${shortTime(new Date(timestamp), english)}`;
}

export function recentTimeLabel(timestamp: number, now = Date.now(), english = true): string {
  const diff = Math.max(0, now - timestamp);
  const elapsed = shortElapsed(diff, english);
  if (elapsed) return english && elapsed === "Now" ? "Just now" : elapsed;
  if (diff < 7 * DAY) return english ? `${Math.floor(diff / DAY)} days ago` : `${Math.floor(diff / DAY)} 天前`;
  return shortDate(new Date(timestamp), english);
}

export function staleTimeLabel(timestamp: number, now = new Date(), english = true): string {
  const date = new Date(timestamp);
  const time = shortTime(date, english);
  if (date.toDateString() === now.toDateString()) return english ? `Today ${time}` : `今天 ${time}`;
  const yesterday = new Date(now.getTime() - DAY);
  if (date.toDateString() === yesterday.toDateString()) return english ? `Yesterday ${time}` : `昨天 ${time}`;
  return `${shortDate(date, english)} ${time}`;
}
