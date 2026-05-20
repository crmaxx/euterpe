import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  usePatchUiPreferences,
  useServerInfo,
  useUiPreferences,
} from "@/api/hooks";
import type { UiPreferences, UiPreferencesPatch } from "@/api/client";
import {
  applyTheme,
  initTheme,
  type Theme,
} from "@/lib/theme";
import {
  QUALITY_OPTIONS,
  type QualityId,
} from "@/lib/quality";
import {
  type Locale,
  translate,
} from "@/i18n/translate";
import { PreferencesContext } from "@/providers/preferences-context";

const LEGACY_THEME = "euterpe.theme";
const LEGACY_LOCALE = "euterpe.locale";
const LEGACY_QUALITY = "euterpe.defaultQuality";

function themeFromApi(theme: UiPreferences["theme"]): Theme {
  return theme;
}

function localeFromApi(locale: UiPreferences["locale"]): Locale {
  return locale;
}

function qualityFromApi(q: number): QualityId {
  if (QUALITY_OPTIONS.some((o) => o.value === q)) {
    return q as QualityId;
  }
  return 6;
}

function readLegacyPatch(): UiPreferencesPatch {
  const patch: UiPreferencesPatch = {};
  try {
    const theme = localStorage.getItem(LEGACY_THEME);
    if (theme === "light" || theme === "dark" || theme === "system") {
      patch.theme = theme;
    }
    const locale = localStorage.getItem(LEGACY_LOCALE);
    if (locale === "en" || locale === "ru") {
      patch.locale = locale;
    }
    const rawQ = localStorage.getItem(LEGACY_QUALITY);
    const q = rawQ ? Number(rawQ) : NaN;
    if (QUALITY_OPTIONS.some((o) => o.value === q)) {
      patch.default_quality = q as UiPreferencesPatch["default_quality"];
    }
  } catch {
    /* ignore */
  }
  return patch;
}

function clearLegacyStorage() {
  try {
    localStorage.removeItem(LEGACY_THEME);
    localStorage.removeItem(LEGACY_LOCALE);
    localStorage.removeItem(LEGACY_QUALITY);
  } catch {
    /* ignore */
  }
}

function applyUiToState(
  ui: UiPreferences,
  setThemeState: (t: Theme) => void,
  setLocaleState: (l: Locale) => void,
  setQualityState: (q: QualityId) => void,
) {
  const theme = themeFromApi(ui.theme);
  const locale = localeFromApi(ui.locale);
  const quality = qualityFromApi(ui.default_quality);
  setThemeState(theme);
  setLocaleState(locale);
  setQualityState(quality);
  applyTheme(theme);
  document.documentElement.lang = locale;
}

export function PreferencesProvider({ children }: { children: ReactNode }) {
  const { data: serverInfo } = useServerInfo();
  const { data: uiFromApi, isSuccess: uiLoaded } = useUiPreferences();
  const patchUi = usePatchUiPreferences();

  const [theme, setThemeState] = useState<Theme>("system");
  const [locale, setLocaleState] = useState<Locale>("en");
  const [defaultQuality, setDefaultQualityState] = useState<QualityId>(6);
  const migratedRef = useRef(false);

  useEffect(() => initTheme(), []);

  useEffect(() => {
    if (serverInfo?.ui) {
      applyUiToState(
        serverInfo.ui,
        setThemeState,
        setLocaleState,
        setDefaultQualityState,
      );
    }
  }, [serverInfo?.ui]);

  useEffect(() => {
    if (uiFromApi) {
      applyUiToState(
        uiFromApi,
        setThemeState,
        setLocaleState,
        setDefaultQualityState,
      );
    }
  }, [uiFromApi]);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  useEffect(() => {
    if (!uiLoaded || migratedRef.current) {
      return;
    }
    migratedRef.current = true;
    const legacy = readLegacyPatch();
    if (
      legacy.theme != null ||
      legacy.locale != null ||
      legacy.default_quality != null
    ) {
      void patchUi.mutateAsync(legacy).finally(() => clearLegacyStorage());
    }
  }, [uiLoaded, patchUi]);

  const setTheme = useCallback(
    (next: Theme) => {
      setThemeState(next);
      void patchUi.mutateAsync({ theme: next });
    },
    [patchUi],
  );

  const setLocale = useCallback(
    (next: Locale) => {
      setLocaleState(next);
      void patchUi.mutateAsync({ locale: next });
    },
    [patchUi],
  );

  const setDefaultQuality = useCallback(
    (next: QualityId) => {
      setDefaultQualityState(next);
      void patchUi.mutateAsync({ default_quality: next });
    },
    [patchUi],
  );

  const t = useCallback(
    (key: string, params?: Record<string, string | number>) =>
      translate(locale, key, params),
    [locale],
  );

  const value = useMemo(
    () => ({
      theme,
      setTheme,
      locale,
      setLocale,
      defaultQuality,
      setDefaultQuality,
      t,
    }),
    [theme, setTheme, locale, setLocale, defaultQuality, setDefaultQuality, t],
  );

  return (
    <PreferencesContext.Provider value={value}>
      {children}
    </PreferencesContext.Provider>
  );
}
