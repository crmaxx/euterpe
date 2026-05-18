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
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { formatQualityLabel } from "@/lib/quality";

function isTerminalStatus(status: DownloadJob["status"]) {
  return status === "completed" || status === "failed" || status === "cancelled";
}

export function QueuePage() {
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
      !window.confirm(
        "Remove all completed, failed, and cancelled jobs from the list? Active downloads will be kept.",
      )
    ) {
      return;
    }
    void purgeFinished.mutateAsync();
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-2xl font-semibold">Download queue</h2>
        {hasTerminalJobs ? (
          <Button
            size="sm"
            variant="outline"
            disabled={purgeFinished.isPending}
            onClick={handleClearHistory}
          >
            Clear history
          </Button>
        ) : null}
      </div>
      {isLoading ? (
        <p className="text-muted-foreground">Loading…</p>
      ) : jobs.length === 0 ? (
        <p className="text-muted-foreground">No jobs.</p>
      ) : (
        <div className="space-y-3">
          {jobs.map((job) => (
            <JobRow
              key={job.id}
              job={job}
              title={titleByQobuzId.get(job.qobuz_id) ?? `Album #${job.qobuz_id}`}
              liveProgress={progress[job.id]}
              onCancel={() => void cancel.mutateAsync(job.id)}
              onDelete={() => void purgeOne.mutateAsync(job.id)}
              cancelPending={cancel.isPending}
              deletePending={purgeOne.isPending}
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
}: {
  job: DownloadJob;
  title: string;
  liveProgress?: number;
  onCancel: () => void;
  onDelete: () => void;
  cancelPending: boolean;
  deletePending: boolean;
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
              Cancel
            </Button>
          ) : null}
          {canDelete ? (
            <Button
              size="sm"
              variant="outline"
              disabled={deletePending}
              onClick={onDelete}
            >
              Delete
            </Button>
          ) : null}
        </div>
      </div>
      <Progress value={pct} aria-label={`Progress ${pct}%`} />
      <p className="mt-1 text-xs text-muted-foreground">{pct.toFixed(0)}%</p>
      {job.error_message ? (
        <p className="mt-2 text-xs text-destructive">{job.error_message}</p>
      ) : null}
    </div>
  );
}