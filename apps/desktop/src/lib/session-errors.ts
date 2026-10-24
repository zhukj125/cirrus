// Detects backend auth errors that require a new sign-in.
const SESSION_ERROR_PATTERNS = [
  /\bHTTP\s*421\b/i,
  /invalid global session/i,
  /unauthenticated|bad auth credentials/i,
  /initClient (?:failed|\u5931\u8d25): HTTP 450/i,
  /missing session id/i,
  /session (?:expired|does not exist|is unavailable|not found)/i,
  /please sign in again/i,
  /\u4f1a\u8bdd.*(?:\u8fc7\u671f|\u4e0d\u5b58\u5728)/,
];

export function isSessionGoneMessage(message: string | null | undefined): boolean {
  return message != null && SESSION_ERROR_PATTERNS.some((pattern) => pattern.test(message));
}
