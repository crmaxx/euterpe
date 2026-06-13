import { useMemo, useState } from "react";
import {
  useBrowseStorage,
  useListSmbShares,
  usePatchStorageSettings,
  useStorageSettings,
  useTestStorageSettings,
} from "@/api/hooks";
import type {
  StorageBrowseEntry,
  StorageLocationPatch,
  StorageSettingsView,
} from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";
import { cn } from "@/lib/utils";
import { ApiClientError } from "@/api/errors";
import {
  activePresetId,
  applyShareToSmbLocation,
  childBrowsePath,
  formatSmbLocation,
  joinBrowsePath,
  parseSmbLocation,
  storageLabel,
} from "@/features/settings/storageLocation";
import { ArrowUp, Check, Folder, RefreshCw, Server, Wifi, XCircle } from "lucide-react";

type StorageKind = "local" | "smb";

function watchStatusText(
  library: StorageSettingsView["library"],
  t: (key: string) => string,
): string | null {
  if (!library || library.kind !== "smb") return null;
  const status = library.watch_status;
  const base =
    status.state === "connected"
      ? t("settings.storage.watchConnected")
      : status.state === "reconnecting"
        ? t("settings.storage.watchReconnecting")
        : status.state === "degraded"
          ? t("settings.storage.watchDegraded")
          : t("settings.storage.watchDisabled");
  return status.degraded_reason ? `${base}: ${status.degraded_reason}` : base;
}

function watchStatusClass(library: StorageSettingsView["library"]): string {
  if (!library || library.kind !== "smb") return "";
  if (library.watch_status.state === "connected") return "text-emerald-600";
  if (library.watch_status.state === "reconnecting") return "text-amber-600";
  if (library.watch_status.state === "degraded") return "text-destructive";
  return "text-muted-foreground";
}

function parentPath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  parts.pop();
  return parts.join("/");
}

function smbPatchFromForm(
  smbLocation: string,
  username: string,
  password: string,
  workgroup: string,
  clearPassword: boolean,
): Extract<StorageLocationPatch, { kind: "smb" }> | null {
  const parsed = parseSmbLocation(smbLocation);
  if (!parsed.host?.trim() || !parsed.share?.trim()) return null;
  const patch: Extract<StorageLocationPatch, { kind: "smb" }> = {
    kind: "smb",
    host: parsed.host.trim(),
    port: parsed.port ?? 445,
    share: parsed.share.trim(),
    path: (parsed.path ?? "").replace(/^\/+/, ""),
    username: username.trim() || null,
    workgroup: workgroup.trim() || null,
  };
  if (password) {
    patch.password = password;
  } else if (clearPassword) {
    patch.password = null;
  }
  return patch;
}

function savedLibraryKey(library: StorageSettingsView["library"]): string {
  if (!library) return "none";
  if (library.kind === "local") return `local:${library.path}`;
  return [
    "smb",
    library.host,
    library.port,
    library.share,
    library.path,
    library.username ?? "",
    library.workgroup ?? "",
    library.password_configured ? "password" : "anonymous",
  ].join(":");
}

function StorageSettingsForm({ settings }: { settings: StorageSettingsView }) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const patch = usePatchStorageSettings();
  const test = useTestStorageSettings();
  const shares = useListSmbShares();

  const current = settings.library;
  const [kind, setKind] = useState<StorageKind>(current?.kind ?? "local");
  const [localPath, setLocalPath] = useState(
    current?.kind === "local" ? current.path : "",
  );
  const [smbLocation, setSmbLocation] = useState(
    current?.kind === "smb" ? storageLabel(current) : "",
  );
  const [username, setUsername] = useState(
    current?.kind === "smb" ? (current.username ?? "") : "",
  );
  const [password, setPassword] = useState("");
  const passwordConfigured =
    current?.kind === "smb" ? current.password_configured : false;
  const [clearPassword, setClearPassword] = useState(false);
  const [workgroup, setWorkgroup] = useState(
    current?.kind === "smb" ? (current.workgroup ?? "") : "",
  );
  const [browsePath, setBrowsePath] = useState("");

  const locationPatch = useMemo<StorageLocationPatch | null>(() => {
    if (kind === "local") {
      return { kind: "local", path: localPath.trim() };
    }
    return smbPatchFromForm(smbLocation, username, password, workgroup, clearPassword);
  }, [clearPassword, kind, localPath, password, smbLocation, username, workgroup]);
  const browse = useBrowseStorage(locationPatch, browsePath);

  const save = async () => {
    if (!locationPatch) {
      toast({
        title: t("settings.storage.saveFailed"),
        description: t("settings.storage.invalidSmbLocation"),
        variant: "destructive",
      });
      return;
    }
    try {
      const res = await patch.mutateAsync({ library: locationPatch });
      setPassword("");
      toast({ title: t("settings.storage.saved") });
      if (res.recommend_full_scan) {
        toast({
          title: t("settings.storage.fullScanRecommended"),
          description:
            res.storage_migration_hint ?? t("settings.storage.fullScanRecommendedDetail"),
          variant: "default",
        });
      }
    } catch (e) {
      toast({
        title: t("settings.storage.saveFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const activatePreset = async (presetId: string) => {
    try {
      const res = await patch.mutateAsync({ activate_preset_id: presetId });
      toast({ title: t("settings.storage.presetActivated") });
      if (res.recommend_full_scan) {
        toast({
          title: t("settings.storage.fullScanRecommended"),
          description:
            res.storage_migration_hint ?? t("settings.storage.fullScanRecommendedDetail"),
        });
      }
    } catch (e) {
      toast({
        title: t("settings.storage.saveFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const testConnection = async () => {
    if (!locationPatch) {
      toast({
        title: t("settings.storage.testFailed"),
        description: t("settings.storage.invalidSmbLocation"),
        variant: "destructive",
      });
      return;
    }
    try {
      await test.mutateAsync({ location: locationPatch });
      toast({ title: t("settings.storage.testOk") });
    } catch (e) {
      toast({
        title: t("settings.storage.testFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const loadShares = async () => {
    const parsed = parseSmbLocation(smbLocation);
    if (!parsed.host?.trim()) {
      toast({
        title: t("settings.storage.sharesFailed"),
        description: t("settings.storage.invalidSmbLocation"),
        variant: "destructive",
      });
      return;
    }
    try {
      const res = await shares.mutateAsync({
        host: parsed.host.trim(),
        port: parsed.port ?? 445,
        username: username.trim() || null,
        password: password || null,
        workgroup: workgroup.trim() || null,
      });
      if (res.shares[0]) {
        setSmbLocation((current) => applyShareToSmbLocation(current, res.shares[0]));
      }
      toast({ title: t("settings.storage.sharesLoaded") });
    } catch (e) {
      toast({
        title: t("settings.storage.sharesFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const selectBrowsePath = () => {
    if (kind === "smb") {
      const parsed = parseSmbLocation(smbLocation);
      if (!parsed.host || !parsed.share) return;
      const libraryPath = (parsed.path ?? "").replace(/^\/+|\/+$/g, "");
      setSmbLocation(
        formatSmbLocation({
          host: parsed.host,
          port: parsed.port,
          share: parsed.share,
          path: joinBrowsePath(libraryPath, browsePath),
        }),
      );
    }
  };

  const browseErrorMessage =
    browse.error instanceof ApiClientError
      ? browse.error.message
      : browse.error instanceof Error
        ? browse.error.message
        : null;

  const currentPresetId = activePresetId(current);
  const statusText = watchStatusText(current, t);

  return (
    <section className="space-y-4 border-t border-border pt-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="font-medium">{t("settings.storage.title")}</h3>
          <p className="text-sm text-muted-foreground">{storageLabel(current)}</p>
          {statusText ? (
            <p className={cn("text-xs", watchStatusClass(current))}>{statusText}</p>
          ) : null}
        </div>
        <Select value={kind} onValueChange={(value) => setKind(value as StorageKind)}>
          <SelectTrigger className="w-36">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="local">{t("settings.storage.local")}</SelectItem>
            <SelectItem value="smb">SMB</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {(settings.presets ?? []).length > 0 ? (
        <div className="space-y-1">
          <Label htmlFor="storage-preset">{t("settings.storage.savedLocations")}</Label>
          <Select
            value={currentPresetId ?? ""}
            onValueChange={(value) => void activatePreset(value)}
          >
            <SelectTrigger id="storage-preset">
              <SelectValue placeholder={t("settings.storage.chooseSaved")} />
            </SelectTrigger>
            <SelectContent>
              {(settings.presets ?? []).map((preset) => (
                <SelectItem key={preset.id} value={preset.id}>
                  {preset.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      ) : null}

      {kind === "local" ? (
        <div className="space-y-1">
          <Label htmlFor="storage-local-path">{t("settings.storage.localPath")}</Label>
          <Input
            id="storage-local-path"
            value={localPath}
            onChange={(e) => setLocalPath(e.target.value)}
            placeholder="/mnt/music"
          />
        </div>
      ) : (
        <div className="space-y-3">
          <div className="space-y-1">
            <Label htmlFor="storage-smb-location">{t("settings.storage.networkLocation")}</Label>
            <div className="flex gap-2">
              <Input
                id="storage-smb-location"
                value={smbLocation}
                onChange={(e) => setSmbLocation(e.target.value)}
                placeholder="smb://host/share/Musik or \\\\host\\share\\Musik"
              />
              <Button
                type="button"
                variant="outline"
                onClick={() => void testConnection()}
                title={t("settings.storage.test")}
              >
                <Wifi className="h-4 w-4" />
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              {t("settings.storage.networkLocationHint")}
            </p>
          </div>
          <div className="grid gap-3 sm:grid-cols-3">
            <div className="space-y-1">
              <Label htmlFor="storage-smb-username">{t("settings.storage.username")}</Label>
              <Input
                id="storage-smb-username"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                autoComplete="username"
              />
            </div>
            <div className="space-y-1">
              <Label htmlFor="storage-smb-password">{t("settings.storage.password")}</Label>
              <Input
                id="storage-smb-password"
                type="password"
                value={password}
                onChange={(e) => {
                  setPassword(e.target.value);
                  if (e.target.value) {
                    setClearPassword(false);
                  }
                }}
                placeholder={
                  passwordConfigured && !clearPassword
                    ? t("settings.storage.passwordStored")
                    : undefined
                }
                autoComplete="current-password"
              />
              {passwordConfigured && !password ? (
                <Button
                  type="button"
                  variant={clearPassword ? "destructive" : "outline"}
                  size="sm"
                  aria-label="Clear stored password"
                  title="Clear stored password"
                  onClick={() => setClearPassword((value) => !value)}
                >
                  <XCircle className="h-4 w-4" />
                </Button>
              ) : null}
            </div>
            <div className="space-y-1">
              <Label htmlFor="storage-smb-workgroup">{t("settings.storage.workgroup")}</Label>
              <Input
                id="storage-smb-workgroup"
                value={workgroup}
                onChange={(e) => setWorkgroup(e.target.value)}
              />
            </div>
          </div>
          {shares.data?.shares.length ? (
            <div className="flex flex-wrap gap-2">
              {shares.data.shares.map((name) => (
                <Button
                  key={name}
                  size="sm"
                  variant="outline"
                  onClick={() =>
                    setSmbLocation((current) => applyShareToSmbLocation(current, name))
                  }
                >
                  <Server className="h-4 w-4" />
                  {name}
                </Button>
              ))}
            </div>
          ) : null}
        </div>
      )}

      <div className="flex flex-wrap gap-2">
        <Button onClick={() => void save()} disabled={patch.isPending}>
          <Check className="h-4 w-4" />
          {t("common.save")}
        </Button>
        <Button variant="outline" onClick={() => void testConnection()} disabled={test.isPending}>
          {t("settings.storage.test")}
        </Button>
        {kind === "smb" ? (
          <Button variant="outline" onClick={() => void loadShares()} disabled={shares.isPending}>
            {t("settings.storage.listShares")}
          </Button>
        ) : null}
      </div>

      <div className="space-y-2 border-t border-border pt-4">
        <div className="flex items-center justify-between gap-2">
          <Label>{t("settings.storage.folderListing")}</Label>
          <div className="flex gap-1">
            <Button
              size="sm"
              variant="ghost"
              title={t("settings.storage.refreshListing")}
              onClick={() => void browse.refetch()}
            >
              <RefreshCw className="h-4 w-4" />
            </Button>
            <Button
              size="sm"
              variant="ghost"
              title={t("settings.storage.parentFolder")}
              disabled={!browsePath}
              onClick={() => setBrowsePath(parentPath(browsePath))}
            >
              <ArrowUp className="h-4 w-4" />
            </Button>
            {kind === "smb" ? (
              <Button
                size="sm"
                variant="outline"
                title={t("settings.storage.selectFolderHint")}
                onClick={selectBrowsePath}
              >
                <Check className="h-4 w-4" />
                {t("settings.storage.selectFolder")}
              </Button>
            ) : null}
          </div>
        </div>
        {browsePath ? (
          <p className="text-xs text-muted-foreground">{browsePath}</p>
        ) : null}
        <div className="min-h-28 rounded-md border border-border">
          {browseErrorMessage ? (
            <div className="px-3 py-6 text-sm text-destructive">{browseErrorMessage}</div>
          ) : null}
          {!browseErrorMessage &&
            (browse.data?.entries ?? []).map((entry: StorageBrowseEntry) => (
            <button
              key={entry.path || entry.name}
              type="button"
              className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm hover:bg-accent"
              onClick={() => entry.is_dir && setBrowsePath(childBrowsePath(browsePath, entry.name))}
            >
              <span className="flex min-w-0 items-center gap-2">
                <Folder className="h-4 w-4 shrink-0 text-muted-foreground" />
                <span className="truncate">{entry.name}</span>
              </span>
              {entry.size != null ? (
                <span className="shrink-0 text-xs text-muted-foreground">{entry.size} B</span>
              ) : null}
            </button>
          ))}
          {!browseErrorMessage && !browse.data?.entries?.length ? (
            <div className="px-3 py-6 text-sm text-muted-foreground">
              {browse.isFetching ? t("common.loading") : t("settings.storage.empty")}
            </div>
          ) : null}
        </div>
      </div>
    </section>
  );
}

export function StorageSettingsSection() {
  const { data, isLoading } = useStorageSettings();

  if (isLoading || !data) {
    return null;
  }

  return <StorageSettingsForm key={savedLibraryKey(data.library)} settings={data} />;
}
