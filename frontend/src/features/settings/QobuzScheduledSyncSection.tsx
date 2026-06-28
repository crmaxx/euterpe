import { useState } from "react";
import {
  usePatchQobuzScheduledSyncSettings,
  useQobuzScheduledSyncSettings,
  useRunQobuzScheduledSyncNow,
} from "@/api/hooks";
import type { QobuzScheduledSyncSettingsResponse } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";

function formatRunStatus(
  response: QobuzScheduledSyncSettingsResponse,
  t: (key: string) => string,
): string {
  const run = response.status.last_run;
  if (!run) {
    return t("settings.qobuzScheduled.none");
  }
  return `${run.trigger}: ${run.status}`;
}

function QobuzScheduledSyncForm({
  response,
}: {
  response: QobuzScheduledSyncSettingsResponse;
}) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const patch = usePatchQobuzScheduledSyncSettings();
  const runNow = useRunQobuzScheduledSyncNow();
  const [enabled, setEnabled] = useState(response.settings.enabled);
  const [cronExpression, setCronExpression] = useState(
    response.settings.cron_expression,
  );
  const [autoDownload, setAutoDownload] = useState(
    response.settings.auto_download_new_favorites,
  );

  const save = async () => {
    try {
      await patch.mutateAsync({
        enabled,
        cron_expression: cronExpression,
        auto_download_new_favorites: autoDownload,
      });
      toast({ title: t("settings.qobuzScheduled.saved") });
    } catch (e) {
      toast({
        title: t("settings.qobuzScheduled.saveFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const run = async () => {
    try {
      await runNow.mutateAsync();
      toast({ title: t("settings.qobuzScheduled.runComplete") });
    } catch (e) {
      toast({
        title: t("settings.qobuzScheduled.runFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  return (
    <section className="space-y-4 rounded-lg border border-border bg-card p-4">
      <div>
        <h3 className="font-medium">{t("settings.qobuzScheduled.title")}</h3>
      </div>
      <div className="grid max-w-md gap-3">
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => setEnabled(e.target.checked)}
          />
          {t("settings.qobuzScheduled.enable")}
        </label>
        <div className="space-y-1">
          <Label htmlFor="qobuz-scheduled-cron">
            {t("settings.qobuzScheduled.cron")}
          </Label>
          <Input
            id="qobuz-scheduled-cron"
            value={cronExpression}
            placeholder="0 3 * * *"
            onChange={(e) => setCronExpression(e.target.value)}
          />
        </div>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={autoDownload}
            onChange={(e) => setAutoDownload(e.target.checked)}
          />
          {t("settings.qobuzScheduled.autoDownload")}
        </label>
      </div>
      <dl className="grid gap-2 text-sm sm:grid-cols-[9rem_1fr]">
        <dt className="text-muted-foreground">
          {t("settings.qobuzScheduled.serverTimezone")}
        </dt>
        <dd>{response.status.server_timezone}</dd>
        <dt className="text-muted-foreground">
          {t("settings.qobuzScheduled.nextRun")}
        </dt>
        <dd>{response.status.next_run_at ?? t("settings.qobuzScheduled.never")}</dd>
        <dt className="text-muted-foreground">
          {t("settings.qobuzScheduled.lastRun")}
        </dt>
        <dd>{formatRunStatus(response, t)}</dd>
      </dl>
      <div className="flex flex-wrap gap-2">
        <Button disabled={patch.isPending} onClick={() => void save()}>
          {t("settings.qobuzScheduled.save")}
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={runNow.isPending}
          onClick={() => void run()}
        >
          {t("settings.qobuzScheduled.runNow")}
        </Button>
      </div>
    </section>
  );
}

export function QobuzScheduledSyncSection() {
  const { t } = usePreferences();
  const { data, isLoading } = useQobuzScheduledSyncSettings();

  if (isLoading || !data) {
    return <p className="text-sm text-muted-foreground">{t("common.loading")}</p>;
  }

  return (
    <QobuzScheduledSyncForm
      key={JSON.stringify(data.settings)}
      response={data}
    />
  );
}
