import { useCallback, useEffect, useSyncExternalStore } from "react";
import type {
  ConvertFileProgress,
  ConvertJobSummary,
  ConvertProgressEvent,
} from "@/api/client";
import { parseConvertFilesPayload } from "@/features/library/parseConvertFiles";

export type AlbumConvertLive = {
  job_id: number;
  status: string;
  files: ConvertFileProgress[];
  files_total: number;
  files_done: number;
  progress_pct: number;
  error_message?: string | null;
};

const liveByAlbum = new Map<number, AlbumConvertLive>();
const listeners = new Set<() => void>();

function notify() {
  for (const l of listeners) {
    l();
  }
}

export function setAlbumConvertLive(
  albumId: number,
  live: AlbumConvertLive | null,
): void {
  if (live == null) {
    if (!liveByAlbum.delete(albumId)) return;
  } else {
    liveByAlbum.set(albumId, live);
  }
  notify();
}

export function getAlbumConvertLive(albumId: number): AlbumConvertLive | null {
  return liveByAlbum.get(albumId) ?? null;
}

export function subscribeAlbumConvertLive(cb: () => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

export function applyConvertProgressEvent(ev: ConvertProgressEvent): void {
  setAlbumConvertLive(ev.album_id, {
    job_id: ev.job_id,
    status: ev.status,
    files: ev.files ?? [],
    files_total: ev.files_total,
    files_done: ev.files_done,
    progress_pct: ev.progress_pct,
    error_message: ev.error_message,
  });
  if (ev.status === "success" || ev.status === "failed") {
    // Keep final per-track state until album/job refetch or navigation.
  }
}

export function hydrateAlbumConvertLiveFromJob(
  albumId: number,
  job: ConvertJobSummary | null | undefined,
): void {
  if (job == null) {
    setAlbumConvertLive(albumId, null);
    return;
  }
  if (job.status !== "queued" && job.status !== "running") {
    const existing = getAlbumConvertLive(albumId);
    if (existing?.job_id === job.id) {
      setAlbumConvertLive(albumId, null);
    }
    return;
  }
  const files = parseConvertFilesPayload(job.payload_json);
  if (files.length === 0) return;
  setAlbumConvertLive(albumId, {
    job_id: job.id,
    status: job.status,
    files,
    files_total: job.files_total,
    files_done: job.files_done,
    progress_pct: job.progress_pct,
    error_message: job.error_message,
  });
}

export function useAlbumConvertLive(albumId: number | null): AlbumConvertLive | null {
  const subscribe = useCallback(
    (cb: () => void) => subscribeAlbumConvertLive(cb),
    [],
  );
  const getSnapshot = useCallback(
    () => (albumId != null ? getAlbumConvertLive(albumId) : null),
    [albumId],
  );
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

export function useHydrateAlbumConvertLive(
  albumId: number | null,
  job: ConvertJobSummary | null | undefined,
): void {
  useEffect(() => {
    if (albumId == null) return;
    hydrateAlbumConvertLiveFromJob(albumId, job);
  }, [albumId, job]);
}
