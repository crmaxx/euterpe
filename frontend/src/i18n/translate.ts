import { en, type Messages } from "./locales/en";
import { ru } from "./locales/ru";

export type Locale = "en" | "ru";

const catalogs: Record<Locale, Messages> = { en, ru };

const STORAGE_KEY = "euterpe.locale";

export function getStoredLocale(): Locale {
  const raw = localStorage.getItem(STORAGE_KEY);
  return raw === "ru" ? "ru" : "en";
}

export function setStoredLocale(locale: Locale) {
  localStorage.setItem(STORAGE_KEY, locale);
}

function resolve(obj: unknown, path: string): string | undefined {
  let cur: unknown = obj;
  for (const part of path.split(".")) {
    if (cur == null || typeof cur !== "object") {
      return undefined;
    }
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === "string" ? cur : undefined;
}

export function translate(
  locale: Locale,
  key: string,
  params?: Record<string, string | number>,
): string {
  let text =
    resolve(catalogs[locale], key) ?? resolve(catalogs.en, key) ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      text = text.replaceAll(`{${k}}`, String(v));
    }
  }
  return text;
}
