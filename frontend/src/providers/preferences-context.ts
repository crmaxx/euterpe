import { createContext } from "react";
import type { Locale } from "@/i18n/translate";
import type { Theme } from "@/lib/theme";

export type PreferencesContextValue = {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
};

export const PreferencesContext =
  createContext<PreferencesContextValue | null>(null);
