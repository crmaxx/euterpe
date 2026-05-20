import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import {
  useCancelDownload,
  useDownloads,
  useFavoritesFlat,
  usePatchDownloadPriority,
  usePauseDownload,
  usePurgeDownload,
  useResumeDownload,
  useRetryDownload,
  usePurgeFinishedDownloads,
} from "@/api/hooks";
import {
  subscribeJobProgress,
  type DownloadJob,
  type JobProgressEvent,
  type TorrentJobDetail,
} from "@/api/client";
import { ArrowDown, ArrowUp, ListMusic, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { sortDownloadQueueJobs } from "@/lib/download-queue-sort";
import { formatBytes, formatBytesPerSec, formatEtaSecs } from "@/lib/format";
import { formatQualityLabel } from "@/lib/quality";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";

type LiveJobState = {
  progress_pct: number;
  download_speed_bps: number;
  torrent_detail?: TorrentJobDetail | null;
};

function isTerminalStatus(status: DownloadJob["status"]) {
  return status === "completed" || status === "failed" || status === "cancelled";
}

function jobTitle(
  job: DownloadJob,
  titleByQobuzId: Map<number, string>,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  if (job.source === "qobuz" && job.qobuz_id > 0) {
    const fromFavorites = titleByQobuzId.get(job.qobuz_id);
    if (fromFavorites) return fromFavorites;
    const label = job.display_title?.trim();
    if (label && label !== "Album download") return label;
    return t("queue.album", { id: job.qobuz_id });
  }
  if (job.display_title?.trim()) {
    return job.display_title.trim();
  }
  return t("queue.job", { id: job.id });
}

function sourceBadge(
  job: DownloadJob,
  t: (key: string) => string,
): string {
  if (job.source === "torrent") return t("queue.badge.torrent");
  if (job.source === "qobuz") return t("queue.badge.qobuz");
  return job.job_type;
}

function torrentStatusLine(
  detail: TorrentJobDetail,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  if (detail.error) {
    return t("queue.torrent.error", { message: detail.error });
  }
  if (detail.euterpe_phase === "importing") {
    return t("queue.torrent.importing");
  }
  const stateKey =
    detail.librqbit_state === "initializing"
      ? "queue.torrent.initializing"
      : detail.librqbit_state === "paused"
        ? "queue.torrent.paused"
        : detail.librqbit_state === "error"
          ? "queue.torrent.errorState"
          : "queue.torrent.downloading";
  const parts: string[] = [t(stateKey)];
  if (detail.librqbit_state === "live" || detail.librqbit_state === "initializing") {
    const peers = detail.peers_live + detail.peers_connecting;
    parts.push(t("queue.torrent.peers", { count: peers }));
    if (peers === 0) {
      parts.push(t("queue.torrent.noPeers"));
    }
    if (detail.download_speed_bps > 0) {
      parts.push(formatBytesPerSec(detail.download_speed_bps));
    } else if (
      detail.librqbit_state === "live" &&
      detail.progress_bytes === 0 &&
      peers > 0
    ) {
      parts.push(t("queue.torrent.noDataYet"));
    }
    if (detail.eta_secs != null && detail.eta_secs > 0) {
      const eta = formatEtaSecs(detail.eta_secs);
      if (eta) parts.push(t("queue.torrent.eta", { eta }));
    }
    if (detail.total_bytes > 0) {
      parts.push(
        t("queue.torrent.bytes", {
          done: formatBytes(detail.progress_bytes),
          total: formatBytes(detail.total_bytes),
        }),
      );
    } else if (detail.librqbit_state === "initializing") {
      parts.push(t("queue.torrent.fetchingMetadata"));
    }
  }
  return parts.join(" · ");
}

function torrentStatusForJob(
  job: DownloadJob,
  detail: TorrentJobDetail | null | undefined,
  t: (key: string, params?: Record<string, string | number>) => string,
): string | null {
  if (detail) {
    return torrentStatusLine(detail, t);
  }
  if (job.status === "running" || job.status === "queued") {
    return t("queue.torrent.waitingStatus");
  }
  return null;
}

export function QueuePage() {
  const { t } = usePreferences();
  const { toast } = useToast();
  const { data, isLoading } = useDownloads();
  const { items: favoriteItems } = useFavoritesFlat({ limit: 100 });
  const cancel = useCancelDownload();
  const purgeFinished = usePurgeFinishedDownloads();
  const purgeOne = usePurgeDownload();
  const retry = useRetryDownload();
  const pause = usePauseDownload();
  const resume = useResumeDownload();
  const patchPriority = usePatchDownloadPriority();
  const qc = useQueryClient();
  const [live, setLive] = useState<Record<number, LiveJobState>>({});

  const titleByQobuzId = useMemo(() => {
    const map = new Map<number, string>();
    for (const item of favoriteItems) {
      map.set(item.qobuz_id, `${item.artist_name} — ${item.title}`);
    }
    return map;
  }, [favoriteItems]);

  useEffect(() => {
    const source = subscribeJobProgress((ev: JobProgressEvent) => {
      setLive((p) => ({
        ...p,
        [ev.id]: {
          progress_pct: ev.progress_pct,
          download_speed_bps: ev.download_speed_bps,
          torrent_detail: ev.torrent_detail ?? p[ev.id]?.torrent_detail,
        },
      }));
      void qc.invalidateQueries({ queryKey: ["downloads"] });
    });
    return () => source.close();
  }, [qc]);

  const jobs = useMemo(
    () => sortDownloadQueueJobs(data?.items ?? []),
    [data?.items],
  );
  const qobuzJobs = jobs.filter((j) => j.source === "qobuz");
  const torrentJobs = jobs.filter((j) => j.source === "torrent");
  const hasTerminalJobs = jobs.some((j) => isTerminalStatus(j.status));

  const clearLiveProgress = (id: number) => {
    setLive((prev) => {
      if (!(id in prev)) return prev;
      const next = { ...prev };
      delete next[id];
      return next;
    });
  };

  const handlePause = (id: number) => {
    void pause
      .mutateAsync(id)
      .then(() => clearLiveProgress(id))
      .catch((e) =>
        toast({
          title: t("queue.pauseFailed"),
          description: e instanceof Error ? e.message : t("common.unknownError"),
          variant: "destructive",
        }),
      );
  };

  const handleResume = (id: number) => {
    void resume.mutateAsync(id).catch((e) =>
      toast({
        title: t("queue.resumeFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      }),
    );
  };

  const handleRetry = (id: number) => {
    void retry
      .mutateAsync(id)
      .then(() => clearLiveProgress(id))
      .catch((e) =>
        toast({
          title: t("queue.retryFailed"),
          description: e instanceof Error ? e.message : t("common.unknownError"),
          variant: "destructive",
        }),
      );
  };

  const handleClearHistory = () => {
    if (!window.confirm(t("queue.clearConfirm"))) {
      return;
    }
    void purgeFinished.mutateAsync();
  };

  return (
    <div className="space-y-6">
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
        <div className="space-y-8">
          <QueueSection
            title={t("queue.section.qobuz")}
            jobs={qobuzJobs}
            titleByQobuzId={titleByQobuzId}
            live={live}
            onCancel={(id) => void cancel.mutateAsync(id)}
            onDelete={(id) => void purgeOne.mutateAsync(id)}
            onPriority={(id, direction) =>
              void patchPriority.mutateAsync({ id, direction })
            }
            onPause={handlePause}
            onResume={handleResume}
            onRetry={handleRetry}
            cancelPending={cancel.isPending}
            deletePending={purgeOne.isPending}
            pausePending={pause.isPending}
            resumePending={resume.isPending}
            retryPending={retry.isPending}
            priorityPending={patchPriority.isPending}
            t={t}
          />
          <QueueSection
            title={t("queue.section.torrent")}
            jobs={torrentJobs}
            titleByQobuzId={titleByQobuzId}
            live={live}
            onCancel={(id) => void cancel.mutateAsync(id)}
            onDelete={(id) => void purgeOne.mutateAsync(id)}
            onPriority={(id, direction) =>
              void patchPriority.mutateAsync({ id, direction })
            }
            onPause={handlePause}
            onResume={handleResume}
            onRetry={handleRetry}
            cancelPending={cancel.isPending}
            deletePending={purgeOne.isPending}
            pausePending={pause.isPending}
            resumePending={resume.isPending}
            retryPending={retry.isPending}
            priorityPending={patchPriority.isPending}
            t={t}
          />
        </div>
      )}
    </div>
  );
}

function QueueSection({
  title,
  jobs,
  titleByQobuzId,
  live,
  onCancel,
  onDelete,
  onPause,
  onResume,
  onRetry,
  onPriority,
  cancelPending,
  deletePending,
  pausePending,
  resumePending,
  retryPending,
  priorityPending,
  t,
}: {
  title: string;
  jobs: DownloadJob[];
  titleByQobuzId: Map<number, string>;
  live: Record<number, LiveJobState>;
  onCancel: (id: number) => void;
  onDelete: (id: number) => void;
  onPause: (id: number) => void;
  onResume: (id: number) => void;
  onRetry: (id: number) => void;
  onPriority: (id: number, direction: "up" | "down") => void;
  cancelPending: boolean;
  deletePending: boolean;
  pausePending: boolean;
  resumePending: boolean;
  retryPending: boolean;
  priorityPending: boolean;
  t: (key: string, params?: Record<string, string | number>) => string;
}) {
  if (jobs.length === 0) {
    return null;
  }
  return (
    <section className="space-y-3">
      <h3 className="text-sm font-medium uppercase tracking-wide text-muted-foreground">
        {title}
      </h3>
      <div className="space-y-3">
        {jobs.map((job) => (
          <JobRow
            key={job.id}
            job={job}
            title={jobTitle(job, titleByQobuzId, t)}
            badge={sourceBadge(job, t)}
            live={live[job.id]}
            onCancel={() => onCancel(job.id)}
            onDelete={() => onDelete(job.id)}
            onPause={() => onPause(job.id)}
            onResume={() => onResume(job.id)}
            onRetry={() => onRetry(job.id)}
            onPriorityUp={() => onPriority(job.id, "up")}
            onPriorityDown={() => onPriority(job.id, "down")}
            cancelPending={cancelPending}
            deletePending={deletePending}
            pausePending={pausePending}
            resumePending={resumePending}
            retryPending={retryPending}
            priorityPending={priorityPending}
            t={t}
          />
        ))}
      </div>
    </section>
  );
}

function JobRow({
  job,
  title,
  badge,
  live,
  onCancel,
  onDelete,
  onPause,
  onResume,
  onRetry,
  onPriorityUp,
  onPriorityDown,
  cancelPending,
  deletePending,
  pausePending,
  resumePending,
  retryPending,
  priorityPending,
  t,
}: {
  job: DownloadJob;
  title: string;
  badge: string;
  live?: LiveJobState;
  onCancel: () => void;
  onDelete: () => void;
  onPause: () => void;
  onResume: () => void;
  onRetry: () => void;
  onPriorityUp: () => void;
  onPriorityDown: () => void;
  cancelPending: boolean;
  deletePending: boolean;
  pausePending: boolean;
  resumePending: boolean;
  retryPending: boolean;
  priorityPending: boolean;
  t: (key: string, params?: Record<string, string | number>) => string;
}) {
  const pct = live?.progress_pct ?? job.progress_pct;
  const speedBps = live?.download_speed_bps ?? job.download_speed_bps ?? 0;
  const torrentDetail = live?.torrent_detail ?? job.torrent_detail;
  const torrentStatus =
    job.source === "torrent"
      ? torrentStatusForJob(job, torrentDetail, t)
      : null;
  const canCancel =
    job.status === "queued" ||
    job.status === "running" ||
    job.status === "paused";
  const canPause = job.status === "queued" || job.status === "running";
  const canResume = job.status === "paused";
  const canRetry = job.status === "failed";
  const canDelete = isTerminalStatus(job.status);
  const canReorder = job.status === "queued";
  const torrentSpeedBps =
    torrentDetail?.download_speed_bps ?? speedBps;
  const showSpeed =
    (job.status === "running" || job.status === "queued") &&
    torrentSpeedBps > 0;

  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0 flex-1">
          <p className="font-medium">{title}</p>
          <p className="text-xs text-muted-foreground">
            #{job.id} ·{" "}
            <span className="rounded bg-muted px-1 py-0.5 font-medium uppercase tracking-wide">
              {badge}
            </span>
            {job.source === "qobuz" && job.quality > 0
              ? ` · ${formatQualityLabel(job.quality)}`
              : null}{" "}
            · {job.status}
          </p>
          {torrentStatus ? (
            <p className="mt-1 text-xs text-muted-foreground">{torrentStatus}</p>
          ) : null}
        </div>
        <div className="flex shrink-0 gap-2">
          {canReorder ? (
            <>
              <Button
                size="icon"
                variant="outline"
                className="size-8"
                disabled={priorityPending}
                aria-label={t("queue.priorityUp")}
                onClick={onPriorityUp}
              >
                <ArrowUp className="size-4" aria-hidden />
              </Button>
              <Button
                size="icon"
                variant="outline"
                className="size-8"
                disabled={priorityPending}
                aria-label={t("queue.priorityDown")}
                onClick={onPriorityDown}
              >
                <ArrowDown className="size-4" aria-hidden />
              </Button>
            </>
          ) : null}
          {canPause ? (
            <Button
              size="sm"
              variant="outline"
              disabled={pausePending}
              onClick={onPause}
            >
              {t("queue.pause")}
            </Button>
          ) : null}
          {canResume ? (
            <Button
              size="sm"
              variant="default"
              disabled={resumePending}
              onClick={onResume}
            >
              {t("queue.resume")}
            </Button>
          ) : null}
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
          {canRetry ? (
            <Button
              size="sm"
              variant="default"
              disabled={retryPending}
              onClick={onRetry}
            >
              {t("queue.retry")}
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
      <div className="mt-1 flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
        <span>{pct.toFixed(0)}%</span>
        {showSpeed ? (
          <span>{formatBytesPerSec(torrentSpeedBps)}</span>
        ) : null}
      </div>
      {job.error_message ? (
        <p className="mt-2 text-xs text-destructive">{job.error_message}</p>
      ) : null}
    </div>
  );
}
