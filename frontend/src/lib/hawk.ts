import HawkCatcher from "@hawk.so/browser";
import type { EventContext } from "@hawk.so/types";

let hawk: HawkCatcher | null = null;

const SENSITIVE_KEY_PARTS = [
  "password",
  "secret",
  "token",
  "authorization",
  "cookie",
  "api_key",
  "apikey",
];

function isSensitiveKey(key: string): boolean {
  const lower = key.toLowerCase();
  return SENSITIVE_KEY_PARTS.some((part) => lower.includes(part));
}

function redactValue(value: unknown): void {
  if (!value || typeof value !== "object") return;
  if (Array.isArray(value)) {
    for (const item of value) redactValue(item);
    return;
  }
  const record = value as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if (isSensitiveKey(key)) {
      delete record[key];
    } else {
      redactValue(record[key]);
    }
  }
}

/** Initialize Hawk browser catcher when `VITE_HAWK_TOKEN` is set. */
export function initHawk(): HawkCatcher | null {
  const token = import.meta.env.VITE_HAWK_TOKEN?.trim();
  if (!token) {
    return null;
  }

  const release =
    import.meta.env.VITE_HAWK_RELEASE?.trim() ||
    import.meta.env.VITE_APP_VERSION?.trim() ||
    undefined;

  hawk = new HawkCatcher({
    token,
    release,
    context: {
      app: "euterpe-frontend",
    },
    beforeSend(event) {
      if (event.context) {
        redactValue(event.context);
      }
      if (event.user) {
        redactValue(event.user);
      }
      if (event.title.startsWith("Script error.")) {
        return false;
      }
      return event;
    },
  });

  if (import.meta.env.DEV) {
    (globalThis as { __euterpeHawk?: HawkCatcher }).__euterpeHawk = hawk;
  }

  return hawk;
}

export function getHawk(): HawkCatcher | null {
  return hawk;
}

/** Attach or clear admin session user on the Hawk client. */
export function syncHawkUser(authed: boolean): void {
  if (!hawk) return;
  if (authed) {
    hawk.setUser({ id: "admin", name: "Admin" });
  } else {
    hawk.clearUser();
  }
}

/** Manual error report (e.g. from catch blocks). */
export function reportToHawk(error: unknown, context?: EventContext): void {
  if (!hawk) return;
  if (error instanceof Error) {
    hawk.send(error, context);
    return;
  }
  hawk.send(String(error), context);
}
