import { useEffect, useState, type ReactNode } from "react";
import { useSearchParams } from "react-router-dom";
import {
  useQobuzConnection,
  useQobuzLogout,
  useQobuzOAuthStart,
  useServerInfo,
} from "@/api/hooks";
import { LogOut, Settings } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ConverterSettingsSection } from "@/features/settings/ConverterSettingsSection";
import { DownloadsSettingsSection } from "@/features/settings/DownloadsSettingsSection";
import { IntegrationsSection } from "@/features/settings/IntegrationsSection";
import { LibraryScanSettingsSection } from "@/features/settings/LibraryScanSettingsSection";
import { QobuzScheduledSyncSection } from "@/features/settings/QobuzScheduledSyncSection";
import { StorageSettingsSection } from "@/features/settings/StorageSettingsSection";
import { TorrentSettingsSection } from "@/features/settings/TorrentSettingsSection";
import { useToast } from "@/hooks/use-toast";
import { QUALITY_OPTIONS, type QualityId } from "@/lib/quality";
import { usePreferences } from "@/hooks/use-preferences";
import type { Locale } from "@/i18n/translate";
import type { Theme } from "@/lib/theme";

type SettingsTab =
  | "general"
  | "scheduled-sync"
  | "integrations"
  | "converter"
  | "library-scan"
  | "library-storage"
  | "downloads"
  | "torrent";

const SETTINGS_TABS: Array<{
  id: SettingsTab;
  labelKey: string;
  requiresTorrent?: boolean;
}> = [
  { id: "general", labelKey: "settings.tabs.general" },
  { id: "scheduled-sync", labelKey: "settings.qobuzScheduled.title" },
  { id: "integrations", labelKey: "integrations.title" },
  { id: "converter", labelKey: "settings.converter.title" },
  { id: "library-scan", labelKey: "settings.libraryScan.title" },
  { id: "library-storage", labelKey: "settings.storage.title" },
  { id: "downloads", labelKey: "settings.downloads.title" },
  { id: "torrent", labelKey: "settings.torrent.title", requiresTorrent: true },
];

function settingsTabId(tab: SettingsTab): string {
  return `settings-tab-${tab}`;
}

function settingsPanelId(tab: SettingsTab): string {
  return `settings-panel-${tab}`;
}

function SettingsTabButton({
  tab,
  activeTab,
  onSelect,
  children,
}: {
  tab: SettingsTab;
  activeTab: SettingsTab;
  onSelect: (tab: SettingsTab) => void;
  children: ReactNode;
}) {
  const active = activeTab === tab;
  return (
    <button
      type="button"
      role="tab"
      id={settingsTabId(tab)}
      aria-selected={active}
      aria-controls={settingsPanelId(tab)}
      className={
        active
          ? "rounded-md bg-background px-3 py-1.5 text-sm font-medium shadow-sm"
          : "rounded-md px-3 py-1.5 text-sm text-muted-foreground hover:text-foreground"
      }
      onClick={() => onSelect(tab)}
    >
      {children}
    </button>
  );
}

function qobuzUserLabel(
  connection: {
    display_name?: string | null;
    qobuz_user_id?: number | null;
  },
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  if (connection.display_name?.trim()) {
    return connection.display_name.trim();
  }
  if (connection.qobuz_user_id != null) {
    return t("settings.qobuz.user", { id: connection.qobuz_user_id });
  }
  return t("settings.qobuz.account");
}

export function SettingsPage() {
  const { t, theme, setTheme, locale, setLocale, defaultQuality, setDefaultQuality } =
    usePreferences();
  const { data: info } = useServerInfo();
  const { data: connection, refetch: refetchConnection } = useQobuzConnection();
  const oauthStart = useQobuzOAuthStart();
  const logout = useQobuzLogout();
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  const hasTorrentSettings = !!info?.torrent_incoming_dir;
  const tabs = SETTINGS_TABS.filter(
    (tab) => !tab.requiresTorrent || hasTorrentSettings,
  );
  const visibleActiveTab =
    activeTab === "torrent" && !hasTorrentSettings ? "general" : activeTab;

  useEffect(() => {
    const qobuz = searchParams.get("qobuz");
    if (qobuz === "connected") {
      toast({
        title: t("settings.toast.connected"),
        description: t("settings.toast.connectedDesc"),
      });
      void refetchConnection();
      searchParams.delete("qobuz");
      searchParams.delete("account_id");
      setSearchParams(searchParams, { replace: true });
    } else if (qobuz === "error") {
      toast({
        title: t("settings.toast.connectFailed"),
        description: t("settings.toast.connectFailedDesc"),
        variant: "destructive",
      });
      searchParams.delete("qobuz");
      setSearchParams(searchParams, { replace: true });
    }
  }, [searchParams, setSearchParams, toast, refetchConnection, t]);

  const connectQobuz = async () => {
    if (!connection?.master_key_configured) {
      toast({
        title: t("settings.toast.serverNotReady"),
        description: t("settings.toast.serverNotReadyDesc"),
        variant: "destructive",
      });
      return;
    }
    try {
      const { authorize_url } = await oauthStart.mutateAsync();
      window.location.href = authorize_url;
    } catch (e) {
      toast({
        title: t("settings.toast.oauthFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const logOutQobuz = async () => {
    try {
      await logout.mutateAsync();
      toast({
        title: t("settings.toast.signedOut"),
        description: t("settings.toast.signedOutDesc"),
      });
    } catch (e) {
      toast({
        title: t("settings.toast.signOutFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const connected = connection?.connected === true;

  return (
    <div className="mx-auto max-w-3xl space-y-8">
      <div>
        <div className="flex items-center gap-2">
          <Settings
            className="size-5 shrink-0 text-muted-foreground"
            aria-hidden
          />
          <h2 className="text-2xl font-semibold">{t("settings.title")}</h2>
        </div>
        <p className="text-sm text-muted-foreground">{t("settings.subtitle")}</p>
      </div>

      <div
        role="tablist"
        aria-label={t("settings.title")}
        className="flex flex-wrap gap-1 rounded-lg bg-muted p-1"
      >
        {tabs.map((tab) => (
          <SettingsTabButton
            key={tab.id}
            tab={tab.id}
            activeTab={visibleActiveTab}
            onSelect={setActiveTab}
          >
            {t(tab.labelKey)}
          </SettingsTabButton>
        ))}
      </div>

      <div
        role="tabpanel"
        id={settingsPanelId("general")}
        aria-labelledby={settingsTabId("general")}
        hidden={visibleActiveTab !== "general"}
        className="space-y-4"
      >
        <section className="space-y-4 rounded-lg border border-border bg-card p-4">
          <h3 className="font-medium">{t("settings.appearance.title")}</h3>
          <div className="space-y-2">
            <Label htmlFor="theme-select">{t("settings.appearance.theme")}</Label>
            <Select
              value={theme}
              onValueChange={(v) => setTheme(v as Theme)}
            >
              <SelectTrigger id="theme-select">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="light">{t("settings.appearance.light")}</SelectItem>
                <SelectItem value="dark">{t("settings.appearance.dark")}</SelectItem>
                <SelectItem value="system">
                  {t("settings.appearance.system")}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>
        </section>

        <section className="space-y-4 rounded-lg border border-border bg-card p-4">
          <h3 className="font-medium">{t("settings.language.title")}</h3>
          <div className="space-y-2">
            <Label htmlFor="locale-select">{t("settings.language.label")}</Label>
            <Select
              value={locale}
              onValueChange={(v) => setLocale(v as Locale)}
            >
              <SelectTrigger id="locale-select">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="en">{t("settings.language.en")}</SelectItem>
                <SelectItem value="ru">{t("settings.language.ru")}</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </section>

        <section className="space-y-4 rounded-lg border border-border bg-card p-4">
          <h3 className="font-medium">{t("settings.qobuz.title")}</h3>
          {connected && connection ? (
            <div className="space-y-3">
              <div className="space-y-1">
                <p className="text-sm font-medium text-foreground">
                  {qobuzUserLabel(connection, t)}
                </p>
                {connection.membership_label && (
                  <p className="text-sm text-muted-foreground">
                    {connection.membership_label}
                  </p>
                )}
              </div>
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  variant="outline"
                  disabled={oauthStart.isPending}
                  onClick={() => void connectQobuz()}
                >
                  {t("settings.qobuz.switchAccount")}
                </Button>
                <Button
                  type="button"
                  variant="destructive"
                  disabled={logout.isPending}
                  onClick={() => void logOutQobuz()}
                >
                  <LogOut className="size-4" aria-hidden />
                  {t("settings.qobuz.logOut")}
                </Button>
              </div>
            </div>
          ) : (
            <div className="space-y-3">
              <p className="text-sm text-muted-foreground">
                {t("settings.qobuz.notSignedIn")}
              </p>
              <Button
                type="button"
                disabled={oauthStart.isPending}
                onClick={() => void connectQobuz()}
              >
                {t("settings.qobuz.connect")}
              </Button>
            </div>
          )}
        </section>
      </div>

      <div
        role="tabpanel"
        id={settingsPanelId("scheduled-sync")}
        aria-labelledby={settingsTabId("scheduled-sync")}
        hidden={visibleActiveTab !== "scheduled-sync"}
      >
        <QobuzScheduledSyncSection />
      </div>

      <div
        role="tabpanel"
        id={settingsPanelId("integrations")}
        aria-labelledby={settingsTabId("integrations")}
        hidden={visibleActiveTab !== "integrations"}
      >
        <IntegrationsSection />
      </div>

      <div
        role="tabpanel"
        id={settingsPanelId("converter")}
        aria-labelledby={settingsTabId("converter")}
        hidden={visibleActiveTab !== "converter"}
      >
        <ConverterSettingsSection />
      </div>

      <div
        role="tabpanel"
        id={settingsPanelId("library-scan")}
        aria-labelledby={settingsTabId("library-scan")}
        hidden={visibleActiveTab !== "library-scan"}
      >
        <LibraryScanSettingsSection />
      </div>

      <div
        role="tabpanel"
        id={settingsPanelId("library-storage")}
        aria-labelledby={settingsTabId("library-storage")}
        hidden={visibleActiveTab !== "library-storage"}
        className="space-y-4 rounded-lg border border-border bg-card p-4"
      >
        <div className="space-y-1 text-sm">
          <Label>{t("settings.downloads.libraryStorage")}</Label>
          <p className="rounded-md border border-border bg-background px-3 py-2 font-mono text-xs">
            {info?.library_storage
              ? info.library_storage.kind === "local"
                ? `local:${info.library_storage.path}`
                : `smb://${info.library_storage.host}/${info.library_storage.share}/${info.library_storage.path}`.replace(/\/$/, "")
              : t("settings.storage.notConfigured")}
          </p>
        </div>
        <StorageSettingsSection />
      </div>

      <section
        role="tabpanel"
        id={settingsPanelId("downloads")}
        aria-labelledby={settingsTabId("downloads")}
        hidden={visibleActiveTab !== "downloads"}
        className="space-y-4 rounded-lg border border-border bg-card p-4"
      >
        <h3 className="font-medium">{t("settings.downloads.title")}</h3>
        <div className="space-y-2">
          <Label>{t("settings.downloads.defaultQuality")}</Label>
          <Select
            value={String(defaultQuality)}
            onValueChange={(v) => setDefaultQuality(Number(v) as QualityId)}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {QUALITY_OPTIONS.map((o) => (
                <SelectItem key={o.value} value={String(o.value)}>
                  {o.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <DownloadsSettingsSection />
      </section>

      {hasTorrentSettings ? (
        <div
          role="tabpanel"
          id={settingsPanelId("torrent")}
          aria-labelledby={settingsTabId("torrent")}
          hidden={visibleActiveTab !== "torrent"}
        >
          <TorrentSettingsSection />
        </div>
      ) : null}
    </div>
  );
}
