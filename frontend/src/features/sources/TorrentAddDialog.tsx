import { useMemo, useState } from "react";
import { Loader2 } from "lucide-react";
import type { TorrentInspectResponse } from "@/api/client";
import {
  useConfirmTorrentDownload,
  useInspectTorrentFile,
  useInspectTorrentMagnet,
} from "@/api/hooks";
import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { TorrentInspectView } from "@/features/sources/TorrentInspectView";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";
import { cn } from "@/lib/utils";

type Props = {
  open: boolean;
  onClose: () => void;
};

export function TorrentAddDialog({ open, onClose }: Props) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const inspectMagnet = useInspectTorrentMagnet();
  const inspectFile = useInspectTorrentFile();
  const confirm = useConfirmTorrentDownload();

  const [magnet, setMagnet] = useState("");
  const [inspect, setInspect] = useState<TorrentInspectResponse | null>(null);
  const [selection, setSelection] = useState<Record<number, boolean>>({});
  const [copyToLibrary, setCopyToLibrary] = useState(true);
  const [autoIndex, setAutoIndex] = useState(true);

  const busy =
    inspectMagnet.isPending || inspectFile.isPending || confirm.isPending;

  const reset = () => {
    setMagnet("");
    setInspect(null);
    setSelection({});
    setCopyToLibrary(true);
    setAutoIndex(true);
  };

  const handleClose = () => {
    if (busy) return;
    reset();
    onClose();
  };

  const applyInspect = (result: TorrentInspectResponse) => {
    setInspect(result);
    const sel: Record<number, boolean> = {};
    for (const f of result.files) {
      sel[f.index] = f.selected;
    }
    setSelection(sel);
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
      });
      toast({ title: t("sources.torrent.queued") });
      handleClose();
    } catch (e) {
      toast({
        title: t("sources.torrent.queueFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const modalClass = useMemo(
    () => cn(inspect ? "max-w-5xl space-y-0 p-0" : "max-w-md"),
    [inspect],
  );

  return (
    <Modal open={open} onClose={handleClose} className={modalClass}>
      {!inspect ? (
        <div className="space-y-4 p-4">
          <h3 className="text-lg font-semibold">{t("sources.torrent.addTitle")}</h3>
          <div className="space-y-2">
            <Label htmlFor="torrent-magnet">{t("sources.torrent.magnet")}</Label>
            <Input
              id="torrent-magnet"
              value={magnet}
              onChange={(e) => setMagnet(e.target.value)}
              placeholder={t("sources.torrent.magnetPlaceholder")}
              disabled={busy}
            />
            <Button
              disabled={!magnet.trim() || busy}
              onClick={() => void runInspectMagnet()}
            >
              {inspectMagnet.isPending ? (
                <Loader2 className="size-4 animate-spin" aria-hidden />
              ) : null}
              {t("sources.torrent.inspect")}
            </Button>
          </div>
          <div className="space-y-2">
            <Label htmlFor="torrent-file">{t("sources.torrent.torrentFile")}</Label>
            <Input
              id="torrent-file"
              type="file"
              accept=".torrent,application/x-bittorrent"
              disabled={busy}
              onChange={(e) => void onFileChange(e.target.files?.[0])}
            />
          </div>
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={handleClose} disabled={busy}>
              {t("common.cancel")}
            </Button>
          </div>
        </div>
      ) : (
        <TorrentInspectView
          inspect={inspect}
          selection={selection}
          copyToLibrary={copyToLibrary}
          autoIndex={autoIndex}
          busy={busy || confirm.isPending}
          onSelectionChange={setSelection}
          onCopyToLibraryChange={(v) => {
            setCopyToLibrary(v);
            if (!v) setAutoIndex(false);
          }}
          onAutoIndexChange={setAutoIndex}
          onCancel={handleClose}
          onConfirm={() => void handleConfirm()}
        />
      )}
    </Modal>
  );
}
