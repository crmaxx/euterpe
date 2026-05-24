import type { ConvertFileProgress } from "@/api/client";
import {
  isConvertiblePath,
  normalizeTrackPath,
} from "@/lib/convertible";
export type TrackConvertStatus = "pending" | "running" | "failed";

const TRACK_STATUSES = new Set<string>([
  "pending",
  "running",
  "failed",
]);

export function findTrackConvertProgress(
  trackPath: string,
  files: ConvertFileProgress[],
): {
  status: TrackConvertStatus;
  progressPct?: number;
  error?: string | null;
} | null {
  if (!isConvertiblePath(trackPath) || files.length === 0) return null;
  const norm = normalizeTrackPath(trackPath);
  const row = files.find((f) => normalizeTrackPath(f.path) === norm);
  if (!row || !TRACK_STATUSES.has(row.status)) return null;
  if (row.status === "success") return null;
  const progressPct =
    row.progress_pct != null && Number.isFinite(row.progress_pct)
      ? Math.min(100, Math.max(0, row.progress_pct))
      : undefined;
  return {
    status: row.status as TrackConvertStatus,
    progressPct,
    error: row.error,
  };
}

export function parseConvertFilesPayload(
  payloadJson: string | null | undefined,
): ConvertFileProgress[] {
  if (!payloadJson) return [];
  try {
    const parsed: unknown = JSON.parse(payloadJson);
    if (!Array.isArray(parsed)) return [];
    const out: ConvertFileProgress[] = [];
    for (const item of parsed) {
      if (
        item != null &&
        typeof item === "object" &&
        "path" in item &&
        "status" in item &&
        typeof (item as { path: unknown }).path === "string" &&
        typeof (item as { status: unknown }).status === "string"
      ) {
        const row = item as {
          path: string;
          status: string;
          progress_pct?: number | null;
          error?: string | null;
        };
        const progress_pct =
          row.progress_pct != null && typeof row.progress_pct === "number"
            ? row.progress_pct
            : undefined;
        out.push({
          path: row.path,
          status: row.status,
          progress_pct,
          error: row.error ?? undefined,
        });
      }
    }
    return out;
  } catch {
    return [];
  }
}
