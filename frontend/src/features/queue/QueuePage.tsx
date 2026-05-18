import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import {
  queryKeys,
  useCancelDownload,
  useDownloads,
  useFavorites,
} from "@/api/hooks";
import { subscribeJobProgress, type DownloadJob } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { formatQualityLabel } from "@/lib/quality";

export function QueuePage() {
  const { data, isLoading } = useDownloads();
  const { data: favorites } = useFavorites(0, 500);
  const cancel = useCancelDownload();
  const qc = useQueryClient();
  const [progress, setProgress] = useState<Record<number, number>>({});

  const titleByQobuzId = useMemo(() => {
    const map = new Map<number, string>();
    for (const item of favorites?.items ?? []) {
      map.set(item.qobuz_id, `${item.artist_name} — ${item.title}`);
    }
    return map;
  }, [favorites?.items]);

  useEffect(() => {
    const source = subscribeJobProgress((ev) => {
      setProgress((p) => ({ ...p, [ev.id]: ev.progress_pct }));
      void qc.invalidateQueries({ queryKey: queryKeys.downloads });
    });
    return () => source.close();
  }, [qc]);

  const jobs = data?.items ?? [];

  return (
    <div className="space-y-4">
      <h2 className="text-2xl font-semibold">Download queue</h2>
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
              cancelPending={cancel.isPending}
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
  cancelPending,
}: {
  job: DownloadJob;
  title: string;
  liveProgress?: number;
  onCancel: () => void;
  cancelPending: boolean;
}) {
  const pct = liveProgress ?? job.progress_pct;
  const canCancel = job.status === "queued" || job.status === "running";

  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div>
          <p className="font-medium">{title}</p>
          <p className="text-xs text-muted-foreground">
            #{job.id} · {job.job_type} · {formatQualityLabel(job.quality)} · {job.status}
          </p>
        </div>
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
      </div>
      <Progress value={pct} aria-label={`Progress ${pct}%`} />
      <p className="mt-1 text-xs text-muted-foreground">{pct.toFixed(0)}%</p>
      {job.error_message ? (
        <p className="mt-2 text-xs text-destructive">{job.error_message}</p>
      ) : null}
    </div>
  );
}
