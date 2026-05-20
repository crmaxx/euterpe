import { useState } from "react";
import {
  useDownloadsSettings,
  usePatchDownloadsSettings,
} from "@/api/hooks";
import type { DownloadsSettings } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";

function DownloadsWorkerForm({ settings }: { settings: DownloadsSettings }) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const patch = usePatchDownloadsSettings();
  const [concurrency, setConcurrency] = useState(() =>
    String(settings.concurrency ?? 3),
  );

  const save = async () => {
    try {
      await patch.mutateAsync({ concurrency: Number(concurrency) });
      toast({ title: t("settings.downloadsWorker.saved") });
    } catch (e) {
      toast({
        title: t("settings.downloadsWorker.saveFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  return (
    <div className="space-y-3 border-t border-border pt-4">
      <div>
        <h4 className="text-sm font-medium">
          {t("settings.downloadsWorker.title")}
        </h4>
        <p className="text-sm text-muted-foreground">
          {t("settings.downloadsWorker.hint")}
        </p>
      </div>
      <div className="max-w-xs space-y-1">
        <Label htmlFor="downloads-concurrency">
          {t("settings.downloadsWorker.concurrency")}
        </Label>
        <Input
          id="downloads-concurrency"
          type="number"
          min={1}
          max={32}
          value={concurrency}
          onChange={(e) => setConcurrency(e.target.value)}
        />
      </div>
      <Button disabled={patch.isPending} onClick={() => void save()}>
        {t("common.save")}
      </Button>
    </div>
  );
}

export function DownloadsSettingsSection() {
  const { data, isLoading } = useDownloadsSettings();

  if (isLoading || !data) {
    return null;
  }

  return <DownloadsWorkerForm key={String(data.concurrency)} settings={data} />;
}
