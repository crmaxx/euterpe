import { useState, type ReactNode } from "react";
import { Download, Loader2, Magnet } from "lucide-react";
import type {
  TorrentInspectResponse,
  TorrentPostDownloadOptions,
} from "@/api/client";
import {
  useConfirmTorrentDownload,
  useCreateDownloadByUrl,
  useInspectTorrentFile,
  useInspectTorrentMagnet,
  useServerInfo,
} from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { FavoritesPage } from "@/features/favorites/FavoritesPage";
import { TorrentInspectView } from "@/features/sources/TorrentInspectView";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";
import { cn } from "@/lib/utils";

type SourceTab = "torrent" | "qobuz-url" | "qobuz-favorites";

function TabButton({
  tab,
  activeTab,
  onSelect,
  children,
}: {
  tab: SourceTab;
  activeTab: SourceTab;
  onSelect: (tab: SourceTab) => void;
  children: ReactNode;
}) {
  const selected = activeTab === tab;
  return (
    <button
      type="button"
      role="tab"
      aria-selected={selected}
      aria-controls={`sources-panel-${tab}`}
      id={`sources-tab-${tab}`}
      className={cn(
        "rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
        selected
          ? "bg-background text-foreground shadow-sm"
          : "text-muted-foreground hover:text-foreground",
      )}
      onClick={() => onSelect(tab)}
    >
      {children}
    </button>
  );
}

function QobuzUrlPanel() {
  const { t, defaultQuality } = usePreferences();
  const { toast } = useToast();
  const downloadByUrl = useCreateDownloadByUrl();
  const [urlInput, setUrlInput] = useState("");

  const queueQobuzUrl = () => {
    const url = urlInput.trim();
    if (!url) return;
    void downloadByUrl
      .mutateAsync({ url, quality: defaultQuality })
      .then(() => {
        setUrlInput("");
        toast({ title: t("sources.qobuz.queued") });
      })
      .catch((err: unknown) => {
        toast({
          title: t("sources.qobuz.queueFailed"),
          description:
            err instanceof Error ? err.message : t("common.unknownError"),
          variant: "destructive",
        });
      });
  };

  return (
    <section className="space-y-3 rounded-lg border border-border bg-card p-4">
      <h3 className="font-medium">{t("sources.qobuz.title")}</h3>
      <p className="text-sm text-muted-foreground">{t("sources.qobuz.hint")}</p>
      <div className="flex flex-wrap items-end gap-2">
        <div className="min-w-[16rem] flex-1 space-y-1">
          <Label htmlFor="sources-qobuz-url">{t("sources.qobuz.url")}</Label>
          <Input
            id="sources-qobuz-url"
            value={urlInput}
            onChange={(e) => setUrlInput(e.target.value)}
            placeholder={t("sources.qobuz.urlPlaceholder")}
            disabled={downloadByUrl.isPending}
          />
        </div>
        <Button
          disabled={!urlInput.trim() || downloadByUrl.isPending}
          onClick={queueQobuzUrl}
        >
          {t("sources.qobuz.download")}
        </Button>
      </div>
    </section>
  );
}

function TorrentPanel() {
  const { t } = usePreferences();
  const { toast } = useToast();
  const { data: info } = useServerInfo();
  const inspectMagnet = useInspectTorrentMagnet();
  const inspectFile = useInspectTorrentFile();
  const confirm = useConfirmTorrentDownload();

  const [magnet, setMagnet] = useState("");
  const [inspect, setInspect] = useState<TorrentInspectResponse | null>(null);
  const [selection, setSelection] = useState<Record<number, boolean>>({});
  const [copyToLibrary, setCopyToLibrary] = useState(true);
  const [autoIndex, setAutoIndex] = useState(true);
  const [postDownload, setPostDownload] =
    useState<TorrentPostDownloadOptions | null>(null);

  const torrentEnabled = !!info?.torrent_incoming_dir;
  const busy =
    inspectMagnet.isPending || inspectFile.isPending || confirm.isPending;

  const reset = () => {
    setMagnet("");
    setInspect(null);
    setSelection({});
    setCopyToLibrary(true);
    setAutoIndex(true);
    setPostDownload(null);
  };

  const applyInspect = (result: TorrentInspectResponse) => {
    setInspect(result);
    const sel: Record<number, boolean> = {};
    for (const f of result.files) {
      sel[f.index] = f.selected;
    }
    setSelection(sel);
    setPostDownload(
      result.post_download_capability
        ? {
            convert_after_download: false,
            split_after_download: false,
            split_after_conversion: false,
            cue_path: result.post_download_capability.cue_candidates[0]?.cue_path ?? null,
            source_file_policy: "delete_after_success",
          }
        : null,
    );
  };

  const runInspectMagnet = async () => {
    const trimmed = magnet.trim();
    if (!trimmed) return;
    try {
      const result = await inspectMagnet.mutateAsync(trimmed);
      applyInspect(result);
    } catch (e) {
      toast({
        title: t("sources.torrent.inspectFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const onFileChange = async (file: File | undefined) => {
    if (!file) return;
    try {
      const result = await inspectFile.mutateAsync(file);
      applyInspect(result);
    } catch (e) {
      toast({
        title: t("sources.torrent.inspectFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const handleConfirm = async () => {
    if (!inspect) return;
    const files = inspect.files.map((f) => ({
      index: f.index,
      selected: selection[f.index] ?? false,
    }));
    try {
      await confirm.mutateAsync({
        inspect_id: inspect.inspect_id,
        files,
        copy_to_library: copyToLibrary,
        auto_index_after_import: autoIndex,
        post_download: postDownload ?? undefined,
      });
      toast({ title: t("sources.torrent.queued") });
      reset();
    } catch (e) {
      toast({
        title: t("sources.torrent.queueFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  if (inspect) {
    return (
      <section className="overflow-hidden rounded-lg border border-border bg-card">
        <TorrentInspectView
          inspect={inspect}
          selection={selection}
          copyToLibrary={copyToLibrary}
          autoIndex={autoIndex}
          postDownload={postDownload}
          busy={busy || confirm.isPending}
          onSelectionChange={setSelection}
          onCopyToLibraryChange={(v) => {
            setCopyToLibrary(v);
            if (!v) setAutoIndex(false);
          }}
          onAutoIndexChange={setAutoIndex}
          onPostDownloadChange={setPostDownload}
          onCancel={reset}
          onConfirm={() => void handleConfirm()}
        />
      </section>
    );
  }

  return (
    <div className="space-y-4">
      <section className="space-y-3 rounded-lg border border-border bg-card p-4">
        <div>
          <h3 className="font-medium">{t("sources.torrent.magnet")}</h3>
          <p className="text-sm text-muted-foreground">
            {torrentEnabled
              ? t("sources.torrent.hint")
              : t("sources.torrent.disabled")}
          </p>
        </div>
        <div className="flex flex-wrap items-end gap-2">
          <div className="min-w-[16rem] flex-1 space-y-1">
          <Input
            id="torrent-magnet"
            aria-label={t("sources.torrent.magnet")}
            value={magnet}
            onChange={(e) => setMagnet(e.target.value)}
            placeholder={t("sources.torrent.magnetPlaceholder")}
              disabled={!torrentEnabled || busy}
            />
          </div>
          <Button
            disabled={!torrentEnabled || !magnet.trim() || busy}
            onClick={() => void runInspectMagnet()}
          >
            {inspectMagnet.isPending ? (
              <Loader2 className="size-4 animate-spin" aria-hidden />
            ) : null}
            {t("sources.torrent.inspect")}
          </Button>
        </div>
        {torrentEnabled ? (
          <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Magnet className="size-3.5 shrink-0" aria-hidden />
            {info?.torrent_incoming_dir}
          </p>
        ) : null}
      </section>

      <section className="space-y-3 rounded-lg border border-border bg-card p-4">
        <div>
          <h3 className="font-medium">{t("sources.torrent.torrentFile")}</h3>
          <p className="text-sm text-muted-foreground">
            {torrentEnabled
              ? t("sources.torrent.hint")
              : t("sources.torrent.disabled")}
          </p>
        </div>
        <div>
          <Input
            id="torrent-file"
            aria-label={t("sources.torrent.torrentFile")}
            type="file"
            accept=".torrent,application/x-bittorrent"
            disabled={!torrentEnabled || busy}
            onChange={(e) => void onFileChange(e.target.files?.[0])}
          />
        </div>
      </section>
    </div>
  );
}

export function SourcesPage() {
  const { t } = usePreferences();
  const [activeTab, setActiveTab] = useState<SourceTab>("torrent");

  return (
    <div className="space-y-8">
      <div className="flex items-center gap-2">
        <Download className="size-5 shrink-0 text-muted-foreground" aria-hidden />
        <h2 className="text-2xl font-semibold">{t("sources.title")}</h2>
      </div>

      <div
        role="tablist"
        aria-label={t("sources.title")}
        className="inline-flex rounded-lg bg-muted p-1"
      >
        <TabButton tab="torrent" activeTab={activeTab} onSelect={setActiveTab}>
          {t("sources.tabs.torrent")}
        </TabButton>
        <TabButton tab="qobuz-url" activeTab={activeTab} onSelect={setActiveTab}>
          {t("sources.tabs.qobuzUrl")}
        </TabButton>
        <TabButton
          tab="qobuz-favorites"
          activeTab={activeTab}
          onSelect={setActiveTab}
        >
          {t("sources.tabs.qobuzFavorites")}
        </TabButton>
      </div>

      <div
        role="tabpanel"
        id={`sources-panel-${activeTab}`}
        aria-labelledby={`sources-tab-${activeTab}`}
      >
        {activeTab === "torrent" ? <TorrentPanel /> : null}
        {activeTab === "qobuz-url" ? <QobuzUrlPanel /> : null}
        {activeTab === "qobuz-favorites" ? <FavoritesPage /> : null}
      </div>
    </div>
  );
}
