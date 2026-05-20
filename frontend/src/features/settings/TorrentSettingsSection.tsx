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

  const [ratio, setRatio] = useState(() => String(settings.seed_ratio_limit));
  const [seedTime, setSeedTime] = useState(() =>
    String(settings.seed_time_limit_sec),
  );
  const [maxUpload, setMaxUpload] = useState(() =>
    String(settings.max_upload_kib_per_sec),
  );

  const save = async () => {
    try {
      await patch.mutateAsync({
        seed_ratio_limit: Number(ratio),
        seed_time_limit_sec: Number(seedTime),
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
        <div className="space-y-1">
          <Label htmlFor="torrent-ratio">{t("settings.torrent.ratio")}</Label>
          <Input
            id="torrent-ratio"
            type="number"
            min={0}
            step={0.1}
            value={ratio}
            onChange={(e) => setRatio(e.target.value)}
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor="torrent-seed-time">{t("settings.torrent.seedTime")}</Label>
          <Input
            id="torrent-seed-time"
            type="number"
            min={0}
            step={1}
            value={seedTime}
            onChange={(e) => setSeedTime(e.target.value)}
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor="torrent-upload">{t("settings.torrent.maxUpload")}</Label>
          <Input
            id="torrent-upload"
            type="number"
            min={0}
            step={1}
            value={maxUpload}
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

  const formKey = `${data.seed_ratio_limit}-${data.seed_time_limit_sec}-${data.max_upload_kib_per_sec}`;

  return <TorrentSettingsForm key={formKey} settings={data} />;
}
