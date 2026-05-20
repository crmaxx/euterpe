import { useState } from "react";
import { Download, Magnet, Plus } from "lucide-react";
import {
  useCreateDownloadByUrl,
  useServerInfo,
} from "@/api/hooks";
import { TorrentAddDialog } from "@/features/sources/TorrentAddDialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";

export function SourcesPage() {
  const { t, defaultQuality } = usePreferences();
  const { toast } = useToast();
  const { data: info } = useServerInfo();
  const downloadByUrl = useCreateDownloadByUrl();
  const [urlInput, setUrlInput] = useState("");
  const [torrentOpen, setTorrentOpen] = useState(false);

  const torrentEnabled = !!info?.torrent_incoming_dir;

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
    <div className="space-y-8">
      <div className="flex items-center gap-2">
        <Download className="size-5 shrink-0 text-muted-foreground" aria-hidden />
        <h2 className="text-2xl font-semibold">{t("sources.title")}</h2>
      </div>

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

      <section className="space-y-3 rounded-lg border border-border bg-card p-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h3 className="font-medium">{t("sources.torrent.title")}</h3>
            <p className="text-sm text-muted-foreground">
              {torrentEnabled
                ? t("sources.torrent.hint")
                : t("sources.torrent.disabled")}
            </p>
          </div>
          <Button
            disabled={!torrentEnabled}
            onClick={() => setTorrentOpen(true)}
          >
            <Plus className="size-4" aria-hidden />
            {t("sources.torrent.add")}
          </Button>
        </div>
        {torrentEnabled ? (
          <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Magnet className="size-3.5 shrink-0" aria-hidden />
            {info?.torrent_incoming_dir}
          </p>
        ) : null}
      </section>

      <TorrentAddDialog open={torrentOpen} onClose={() => setTorrentOpen(false)} />
    </div>
  );
}
