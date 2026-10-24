// Shared English time labels.
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

function shortElapsed(diff: number): string | null {
  if (diff < MINUTE) return "Now";
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)} min ago`;
  if (diff < DAY) return `${Math.floor(diff / HOUR)} hr ago`;
  return null;
}

function shortDate(date: Date): string {
  return date.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

export function relativeTimeLabel(timestamp: number, now = Date.now()): string {
  const elapsed = shortElapsed(Math.max(0, now - timestamp));
  if (elapsed) return elapsed;
  const date = new Date(timestamp);
  return `${shortDate(date)} ${date.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit", hour12: false })}`;
}

export function recentTimeLabel(timestamp: number, now = Date.now()): string {
  const diff = Math.max(0, now - timestamp);
  const elapsed = shortElapsed(diff);
  if (elapsed) return elapsed === "Now" ? "Just now" : elapsed;
  if (diff < 7 * DAY) return `${Math.floor(diff / DAY)} days ago`;
  return shortDate(new Date(timestamp));
}

export function staleTimeLabel(timestamp: number, now = new Date()): string {
  const date = new Date(timestamp);
  const time = date.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit", hour12: false });
  if (date.toDateString() === now.toDateString()) return `Today ${time}`;
  const yesterday = new Date(now.getTime() - DAY);
  if (date.toDateString() === yesterday.toDateString()) return `Yesterday ${time}`;
  return `${shortDate(date)} ${time}`;
}
