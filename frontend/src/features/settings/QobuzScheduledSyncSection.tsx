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
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  const run = response.status.last_run;
  if (!run) {
    return t("settings.qobuzScheduled.none");
  }
  return t("settings.qobuzScheduled.lastRunSummary", {
    trigger: run.trigger,
    status: run.status,
    total: run.albums_total ?? 0,
    added: run.albums_added ?? 0,
    removed: run.albums_removed ?? 0,
  });
}

function formatRunDescription(
  response: QobuzScheduledSyncSettingsResponse,
  t: (key: string, params?: Record<string, string | number>) => string,
): string | undefined {
  const run = response.status.last_run;
  if (!run || run.status !== "success") {
    return undefined;
  }
  const added = run.albums_added ?? 0;
  if (!response.settings.auto_download_new_favorites) {
    return t("settings.qobuzScheduled.runListOnly", { added });
  }
  if (added === 0) {
    return t("settings.qobuzScheduled.runNoNewFavorites");
  }
  return t("settings.qobuzScheduled.runAutoDownload", { added });
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
    const trimmedCronExpression = cronExpression.trim();
    if (!trimmedCronExpression) {
      toast({
        title: t("settings.qobuzScheduled.cronRequired"),
        variant: "destructive",
      });
      return;
    }
    try {
      await patch.mutateAsync({
        enabled,
        cron_expression: trimmedCronExpression,
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
      const result = await runNow.mutateAsync();
      toast({
        title: t("settings.qobuzScheduled.runComplete"),
        description: formatRunDescription(result, t),
      });
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
