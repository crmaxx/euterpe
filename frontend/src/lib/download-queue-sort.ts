import type { DownloadJob } from "@/api/client";

function isTerminalStatus(status: DownloadJob["status"]) {
  return status === "completed" || status === "failed" || status === "cancelled";
}

/** UI order: running → queued (FIFO) → paused → finished history (newest first). */
function statusRank(status: DownloadJob["status"]): number {
  if (status === "running") return 0;
  if (status === "queued") return 1;
  if (status === "paused") return 2;
  return 3;
}

export function compareDownloadQueueJobs(a: DownloadJob, b: DownloadJob): number {
  const byStatus = statusRank(a.status) - statusRank(b.status);
  if (byStatus !== 0) return byStatus;

  if (!isTerminalStatus(a.status)) {
    if (a.queue_position !== b.queue_position) {
      return a.queue_position - b.queue_position;
    }
    return a.id - b.id;
  }

  return b.id - a.id;
}

export function sortDownloadQueueJobs(jobs: DownloadJob[]): DownloadJob[] {
  return [...jobs].sort(compareDownloadQueueJobs);
}
