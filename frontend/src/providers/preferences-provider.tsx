import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  applyTheme,
  getStoredTheme,
  initTheme,
  setStoredTheme,
  type Theme,
} from "@/lib/theme";
import {
  getStoredLocale,
  setStoredLocale,
  translate,
  type Locale,
} from "@/i18n/translate";
import { PreferencesContext } from "@/providers/preferences-context";

export function PreferencesProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(() => getStoredTheme());
  const [locale, setLocaleState] = useState<Locale>(() => getStoredLocale());

  useEffect(() => initTheme(), []);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  const setTheme = useCallback((next: Theme) => {
    setStoredTheme(next);
    setThemeState(next);
  }, []);

  const setLocale = useCallback((next: Locale) => {
    setStoredLocale(next);
    setLocaleState(next);
  }, []);

  const t = useCallback(
    (key: string, params?: Record<string, string | number>) =>
      translate(locale, key, params),
    [locale],
  );

  const value = useMemo(
    () => ({ theme, setTheme, locale, setLocale, t }),
    [theme, setTheme, locale, setLocale, t],
  );

  return (
    <PreferencesContext.Provider value={value}>
      {children}
    </PreferencesContext.Provider>
  );
}
