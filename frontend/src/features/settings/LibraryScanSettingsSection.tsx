import { useState } from "react";
import {
  useLibraryScanSettings,
  usePatchLibraryScanSettings,
} from "@/api/hooks";
import type { LibraryScanSettings } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";

function ScanSettingsForm({ settings }: { settings: LibraryScanSettings }) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const patch = usePatchLibraryScanSettings();

  const [workerTotal, setWorkerTotal] = useState(() =>
    String(settings.worker_total ?? 10),
  );
  const [enumWorkers, setEnumWorkers] = useState(() =>
    String(settings.enum_workers ?? 5),
  );
  const [processWorkers, setProcessWorkers] = useState(() =>
    String(settings.process_workers ?? 5),
  );
  const [seedDepth, setSeedDepth] = useState(() =>
    String(settings.seed_depth ?? 0),
  );

  const save = async () => {
    try {
      await patch.mutateAsync({
        worker_total: Number(workerTotal),
        enum_workers: Number(enumWorkers),
        process_workers: Number(processWorkers),
        seed_depth: Number(seedDepth),
      });
      toast({ title: t("settings.libraryScan.saved") });
    } catch (e) {
      toast({
        title: t("settings.libraryScan.saveFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  return (
    <section className="space-y-4 rounded-lg border border-border bg-card p-4">
      <div>
        <h3 className="font-medium">{t("settings.libraryScan.title")}</h3>
        <p className="text-sm text-muted-foreground">
          {t("settings.libraryScan.hint")}
        </p>
      </div>
      <div className="grid max-w-md gap-3">
        <div className="space-y-1">
          <Label htmlFor="scan-worker-total">
            {t("settings.libraryScan.workerTotal")}
          </Label>
          <Input
            id="scan-worker-total"
            type="number"
            min={2}
            max={32}
            value={workerTotal}
            onChange={(e) => setWorkerTotal(e.target.value)}
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor="scan-enum">{t("settings.libraryScan.enumWorkers")}</Label>
          <Input
            id="scan-enum"
            type="number"
            min={1}
            value={enumWorkers}
            onChange={(e) => setEnumWorkers(e.target.value)}
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor="scan-process">
            {t("settings.libraryScan.processWorkers")}
          </Label>
          <Input
            id="scan-process"
            type="number"
            min={1}
            value={processWorkers}
            onChange={(e) => setProcessWorkers(e.target.value)}
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor="scan-depth">{t("settings.libraryScan.seedDepth")}</Label>
          <Input
            id="scan-depth"
            type="number"
            min={0}
            value={seedDepth}
            onChange={(e) => setSeedDepth(e.target.value)}
          />
        </div>
      </div>
      <Button disabled={patch.isPending} onClick={() => void save()}>
        {t("common.save")}
      </Button>
    </section>
  );
}

export function LibraryScanSettingsSection() {
  const { t } = usePreferences();
  const { data, isLoading } = useLibraryScanSettings();

  if (isLoading || !data) {
    return <p className="text-sm text-muted-foreground">{t("common.loading")}</p>;
  }

  return <ScanSettingsForm key={JSON.stringify(data)} settings={data} />;
}
