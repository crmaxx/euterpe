import { useState } from "react";
import { usePatchTorrentSettings, useTorrentSettings } from "@/api/hooks";
import type { TorrentSettings } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";

function TorrentSettingsForm({ settings }: { settings: TorrentSettings }) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const patch = usePatchTorrentSettings();

  const [disableUpload, setDisableUpload] = useState(
    () => settings.disable_upload,
  );
  const [maxUpload, setMaxUpload] = useState(() =>
    String(settings.max_upload_kib_per_sec),
  );

  const save = async () => {
    try {
      await patch.mutateAsync({
        disable_upload: disableUpload,
        max_upload_kib_per_sec: Number(maxUpload),
      });
      toast({ title: t("settings.torrent.saved") });
    } catch (e) {
      toast({
        title: t("settings.torrent.saveFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  return (
    <section className="space-y-4 rounded-lg border border-border bg-card p-4">
      <div>
        <h3 className="font-medium">{t("settings.torrent.title")}</h3>
        <p className="text-sm text-muted-foreground">{t("settings.torrent.hint")}</p>
      </div>
      <div className="grid max-w-md gap-3">
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={disableUpload}
            onChange={(e) => setDisableUpload(e.target.checked)}
          />
          {t("settings.torrent.disableUpload")}
        </label>
        <div className="space-y-1">
          <Label htmlFor="torrent-upload">{t("settings.torrent.maxUpload")}</Label>
          <Input
            id="torrent-upload"
            type="number"
            min={0}
            step={1}
            value={maxUpload}
            disabled={disableUpload}
            onChange={(e) => setMaxUpload(e.target.value)}
          />
        </div>
      </div>
      <Button disabled={patch.isPending} onClick={() => void save()}>
        {t("common.save")}
      </Button>
    </section>
  );
}

export function TorrentSettingsSection() {
  const { t } = usePreferences();
  const { data, isLoading } = useTorrentSettings();

  if (isLoading || !data) {
    return <p className="text-sm text-muted-foreground">{t("common.loading")}</p>;
  }

  const formKey = `${data.disable_upload}-${data.max_upload_kib_per_sec}`;

  return <TorrentSettingsForm key={formKey} settings={data} />;
}
