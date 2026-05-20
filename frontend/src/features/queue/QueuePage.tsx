import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import {
  useCancelDownload,
  useDownloads,
  useFavoritesFlat,
  usePurgeDownload,
  usePurgeFinishedDownloads,
} from "@/api/hooks";
import { subscribeJobProgress, type DownloadJob } from "@/api/client";
import { ListMusic, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { formatQualityLabel } from "@/lib/quality";
import { usePreferences } from "@/hooks/use-preferences";

function isTerminalStatus(status: DownloadJob["status"]) {
  return status === "completed" || status === "failed" || status === "cancelled";
}

export function QueuePage() {
  const { t } = usePreferences();
  const { data, isLoading } = useDownloads();
  const { items: favoriteItems } = useFavoritesFlat({ limit: 100 });
  const cancel = useCancelDownload();
  const purgeFinished = usePurgeFinishedDownloads();
  const purgeOne = usePurgeDownload();
  const qc = useQueryClient();
  const [progress, setProgress] = useState<Record<number, number>>({});

  const titleByQobuzId = useMemo(() => {
    const map = new Map<number, string>();
    for (const item of favoriteItems) {
      map.set(item.qobuz_id, `${item.artist_name} — ${item.title}`);
    }
    return map;
  }, [favoriteItems]);

  useEffect(() => {
    const source = subscribeJobProgress((ev) => {
      setProgress((p) => ({ ...p, [ev.id]: ev.progress_pct }));
      void qc.invalidateQueries({ queryKey: ["downloads"] });
    });
    return () => source.close();
  }, [qc]);

  const jobs = data?.items ?? [];
  const hasTerminalJobs = jobs.some((j) => isTerminalStatus(j.status));

  const handleClearHistory = () => {
    if (
      !window.confirm(t("queue.clearConfirm"))
    ) {
      return;
    }
    void purgeFinished.mutateAsync();
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <ListMusic
            className="size-5 shrink-0 text-muted-foreground"
            aria-hidden
          />
          <h2 className="text-2xl font-semibold">{t("queue.title")}</h2>
        </div>
        {hasTerminalJobs ? (
          <Button
            size="sm"
            variant="outline"
            disabled={purgeFinished.isPending}
            onClick={handleClearHistory}
          >
            <Trash2 className="size-4" aria-hidden />
            {t("queue.clearHistory")}
          </Button>
        ) : null}
      </div>
      {isLoading ? (
        <p className="text-muted-foreground">{t("common.loading")}</p>
      ) : jobs.length === 0 ? (
        <p className="text-muted-foreground">{t("queue.noJobs")}</p>
      ) : (
        <div className="space-y-3">
          {jobs.map((job) => (
            <JobRow
              key={job.id}
              job={job}
              title={
                titleByQobuzId.get(job.qobuz_id) ??
                t("queue.album", { id: job.qobuz_id })
              }
              liveProgress={progress[job.id]}
              onCancel={() => void cancel.mutateAsync(job.id)}
              onDelete={() => void purgeOne.mutateAsync(job.id)}
              cancelPending={cancel.isPending}
              deletePending={purgeOne.isPending}
              t={t}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function JobRow({
  job,
  title,
  liveProgress,
  onCancel,
  onDelete,
  cancelPending,
  deletePending,
  t,
}: {
  job: DownloadJob;
  title: string;
  liveProgress?: number;
  onCancel: () => void;
  onDelete: () => void;
  cancelPending: boolean;
  deletePending: boolean;
  t: (key: string, params?: Record<string, string | number>) => string;
}) {
  const pct = liveProgress ?? job.progress_pct;
  const canCancel = job.status === "queued" || job.status === "running";
  const canDelete = isTerminalStatus(job.status);

  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div>
          <p className="font-medium">{title}</p>
          <p className="text-xs text-muted-foreground">
            #{job.id} · {job.job_type} · {formatQualityLabel(job.quality)} · {job.status}
          </p>
        </div>
        <div className="flex gap-2">
          {canCancel ? (
            <Button
              size="sm"
              variant="destructive"
              disabled={cancelPending}
              onClick={onCancel}
            >
              {t("queue.cancel")}
            </Button>
          ) : null}
          {canDelete ? (
            <Button
              size="sm"
              variant="outline"
              disabled={deletePending}
              onClick={onDelete}
            >
              {t("common.delete")}
            </Button>
          ) : null}
        </div>
      </div>
      <Progress value={pct} aria-label={t("queue.progress", { pct })} />
      <p className="mt-1 text-xs text-muted-foreground">{pct.toFixed(0)}%</p>
      {job.error_message ? (
        <p className="mt-2 text-xs text-destructive">{job.error_message}</p>
      ) : null}
    </div>
  );
}