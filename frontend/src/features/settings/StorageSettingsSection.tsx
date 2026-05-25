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
  StorageLocationView,
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
import { ArrowUp, Check, Folder, RefreshCw, Server, Wifi } from "lucide-react";

type StorageKind = "local" | "smb";

function storageLabel(location: StorageLocationView | null | undefined): string {
  if (!location) return "not configured";
  if (location.kind === "local") return `local:${location.path}`;
  const path = location.path ? `/${location.path}` : "";
  return `smb://${location.host}/${location.share}${path}`;
}

function watchStatusText(
  location: StorageLocationView | null | undefined,
  t: (key: string) => string,
): string | null {
  if (!location || location.kind !== "smb") return null;
  const status = location.watch_status;
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

function watchStatusClass(location: StorageLocationView | null | undefined): string {
  if (!location || location.kind !== "smb") return "";
  if (location.watch_status.state === "connected") return "text-emerald-600";
  if (location.watch_status.state === "reconnecting") return "text-amber-600";
  if (location.watch_status.state === "degraded") return "text-destructive";
  return "text-muted-foreground";
}

function parentPath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  parts.pop();
  return parts.join("/");
}

function parseSmbLocation(raw: string): Partial<Extract<StorageLocationPatch, { kind: "smb" }>> {
  const value = raw.trim();
  if (!value) return {};
  if (value.startsWith("\\\\")) {
    const [host, share, ...rest] = value.replace(/^\\\\/, "").split("\\").filter(Boolean);
    return { kind: "smb", host, share, path: rest.join("/") };
  }
  try {
    const url = new URL(value.startsWith("smb://") ? value : `smb://${value}`);
    const [share, ...rest] = url.pathname.split("/").filter(Boolean);
    return {
      kind: "smb",
      host: url.hostname,
      port: url.port ? Number(url.port) : 445,
      share,
      path: rest.join("/"),
    };
  } catch {
    return {};
  }
}

function StorageSettingsForm({ settings }: { settings: StorageSettingsView }) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const patch = usePatchStorageSettings();
  const test = useTestStorageSettings();
  const shares = useListSmbShares();

  const current = settings.library;
  const [kind, setKind] = useState<StorageKind>(settings.library?.kind ?? "local");
  const [localPath, setLocalPath] = useState(
    settings.library?.kind === "local" ? settings.library.path : "",
  );
  const [smbLocation, setSmbLocation] = useState(
    settings.library?.kind === "smb" ? storageLabel(settings.library) : "",
  );
  const [host, setHost] = useState(
    settings.library?.kind === "smb" ? settings.library.host : "",
  );
  const [port, setPort] = useState(
    settings.library?.kind === "smb" ? String(settings.library.port) : "445",
  );
  const [share, setShare] = useState(
    settings.library?.kind === "smb" ? settings.library.share : "",
  );
  const [remotePath, setRemotePath] = useState(
    settings.library?.kind === "smb" ? settings.library.path : "",
  );
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [workgroup, setWorkgroup] = useState(
    settings.library?.kind === "smb" ? (settings.library.workgroup ?? "") : "",
  );
  const [browsePath, setBrowsePath] = useState(
    settings.library?.kind === "smb" ? settings.library.path : "",
  );
  const browse = useBrowseStorage(browsePath);

  const locationPatch = useMemo<StorageLocationPatch>(() => {
    if (kind === "local") {
      return { kind: "local", path: localPath.trim() };
    }
    return {
      kind: "smb",
      host: host.trim(),
      port: Number(port || 445),
      share: share.trim(),
      path: remotePath.trim().replace(/^\/+/, ""),
      username: username.trim() || null,
      password: password || null,
      workgroup: workgroup.trim() || null,
    };
  }, [host, kind, localPath, password, port, remotePath, share, username, workgroup]);

  const applySmbLocation = () => {
    const parsed = parseSmbLocation(smbLocation);
    if (parsed.host) setHost(parsed.host);
    if (parsed.port) setPort(String(parsed.port));
    if (parsed.share) setShare(parsed.share);
    if (parsed.path != null) setRemotePath(parsed.path);
  };

  const save = async () => {
    try {
      const res = await patch.mutateAsync({ library: locationPatch });
      const next = res.settings.library;
      setBrowsePath(next?.kind === "smb" ? next.path : "");
      setPassword("");
      toast({ title: t("settings.storage.saved") });
    } catch (e) {
      toast({
        title: t("settings.storage.saveFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const testConnection = async () => {
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
    try {
      const res = await shares.mutateAsync({
        host: host.trim(),
        port: Number(port || 445),
        username: username.trim() || null,
        password: password || null,
        workgroup: workgroup.trim() || null,
      });
      if (res.shares[0] && !share) setShare(res.shares[0]);
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
      setRemotePath(browsePath);
    }
  };
  const statusText = watchStatusText(current, t);

  return (
    <section className="space-y-4 border-t border-border pt-6">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="font-medium">{t("settings.storage.title")}</h3>
          <p className="text-sm text-muted-foreground">
            {storageLabel(current)}
          </p>
          {statusText ? (
            <p className={cn("text-xs", watchStatusClass(current))}>
              {statusText}
            </p>
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
                onBlur={applySmbLocation}
                placeholder="\\\\host\\share or smb://host/share"
              />
              <Button type="button" variant="outline" onClick={applySmbLocation}>
                <Wifi className="h-4 w-4" />
              </Button>
            </div>
          </div>
          <div className="grid gap-3 sm:grid-cols-[1fr_6rem]">
            <div className="space-y-1">
              <Label htmlFor="storage-smb-host">{t("settings.storage.host")}</Label>
              <Input id="storage-smb-host" value={host} onChange={(e) => setHost(e.target.value)} />
            </div>
            <div className="space-y-1">
              <Label htmlFor="storage-smb-port">{t("settings.storage.port")}</Label>
              <Input id="storage-smb-port" value={port} onChange={(e) => setPort(e.target.value)} />
            </div>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <Label htmlFor="storage-smb-share">{t("settings.storage.share")}</Label>
              <Input id="storage-smb-share" value={share} onChange={(e) => setShare(e.target.value)} />
            </div>
            <div className="space-y-1">
              <Label htmlFor="storage-smb-path">{t("settings.storage.pathInShare")}</Label>
              <Input id="storage-smb-path" value={remotePath} onChange={(e) => setRemotePath(e.target.value)} />
            </div>
          </div>
          <div className="grid gap-3 sm:grid-cols-3">
            <Input placeholder={t("settings.storage.username")} value={username} onChange={(e) => setUsername(e.target.value)} />
            <Input placeholder={t("settings.storage.password")} type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
            <Input placeholder={t("settings.storage.workgroup")} value={workgroup} onChange={(e) => setWorkgroup(e.target.value)} />
          </div>
          {shares.data?.shares.length ? (
            <div className="flex flex-wrap gap-2">
              {shares.data.shares.map((name) => (
                <Button key={name} size="sm" variant="outline" onClick={() => setShare(name)}>
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
          <Button variant="outline" onClick={() => void loadShares()} disabled={shares.isPending || !host.trim()}>
            {t("settings.storage.listShares")}
          </Button>
        ) : null}
      </div>

      <div className="space-y-2 border-t border-border pt-4">
        <div className="flex items-center justify-between gap-2">
          <Label>{t("settings.storage.folderListing")}</Label>
          <div className="flex gap-1">
            <Button size="sm" variant="ghost" onClick={() => void browse.refetch()}>
              <RefreshCw className="h-4 w-4" />
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setBrowsePath(parentPath(browsePath))}>
              <ArrowUp className="h-4 w-4" />
            </Button>
            <Button size="sm" variant="outline" onClick={selectBrowsePath}>
              <Check className="h-4 w-4" />
              {t("settings.storage.selectFolder")}
            </Button>
          </div>
        </div>
        <div className="min-h-28 rounded-md border border-border">
          {(browse.data?.entries ?? []).map((entry: StorageBrowseEntry) => (
            <button
              key={entry.path}
              type="button"
              className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm hover:bg-accent"
              onClick={() => entry.is_dir && setBrowsePath(entry.path)}
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
          {!browse.data?.entries?.length ? (
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

  return <StorageSettingsForm key={JSON.stringify(data.library)} settings={data} />;
}
