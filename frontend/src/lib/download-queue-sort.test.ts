import { describe, expect, it } from "vitest";
import type { DownloadJob } from "@/api/client";
import { sortDownloadQueueJobs } from "./download-queue-sort";

function job(
  partial: Pick<DownloadJob, "id" | "status" | "queue_position"> &
    Partial<DownloadJob>,
): DownloadJob {
  return {
    job_type: "album",
    source: "qobuz",
    qobuz_id: partial.id,
    quality: 6,
    progress_pct: 0,
    download_speed_bps: 0,
    error_message: null,
    display_title: "",
    torrent_detail: null,
    created_at: "",
    updated_at: "",
    ...partial,
  };
}

describe("sortDownloadQueueJobs", () => {
  it("puts active jobs above terminal and keeps FIFO among queued", () => {
    const sorted = sortDownloadQueueJobs([
      job({ id: 1, status: "completed", queue_position: 1 }),
      job({ id: 3, status: "queued", queue_position: 2 }),
      job({ id: 2, status: "queued", queue_position: 1 }),
      job({ id: 4, status: "running", queue_position: 0 }),
    ]);
    expect(sorted.map((j) => j.id)).toEqual([4, 2, 3, 1]);
  });

  it("places paused above terminal jobs", () => {
    const sorted = sortDownloadQueueJobs([
      job({ id: 1, status: "completed", queue_position: 1 }),
      job({ id: 2, status: "paused", queue_position: 1 }),
      job({ id: 3, status: "running", queue_position: 0 }),
    ]);
    expect(sorted.map((j) => j.id)).toEqual([3, 2, 1]);
  });

  it("orders terminal jobs by id desc", () => {
    const sorted = sortDownloadQueueJobs([
      job({ id: 10, status: "failed", queue_position: 0 }),
      job({ id: 12, status: "completed", queue_position: 0 }),
    ]);
    expect(sorted.map((j) => j.id)).toEqual([12, 10]);
  });
});
