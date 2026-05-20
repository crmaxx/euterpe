/** Format byte size (e.g. `633.5 MB`). */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return "0 B";
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/** Format bytes per second as human-readable throughput (e.g. `1.2 MB/s`). */
export function formatBytesPerSec(bps: number): string {
  if (!Number.isFinite(bps) || bps <= 0) {
    return "0 B/s";
  }
  const units = ["B/s", "KB/s", "MB/s", "GB/s"] as const;
  let value = bps;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

/** Short ETA label for torrent queue (e.g. `8m`, `1h 5m`). */
export function formatEtaSecs(sec: number): string {
  if (!Number.isFinite(sec) || sec <= 0) {
    return "";
  }
  const total = Math.ceil(sec);
  if (total < 60) {
    return `${total}s`;
  }
  if (total < 3600) {
    return `${Math.ceil(total / 60)}m`;
  }
  const h = Math.floor(total / 3600);
  const m = Math.ceil((total % 3600) / 60);
  return m > 0 ? `${h}h ${m}m` : `${h}h`;
}

/** Format seconds as `m:ss` (e.g. `3:05`). */
export function formatDuration(sec: number): string {
  if (!Number.isFinite(sec) || sec < 0) {
    return "0:00";
  }
  const total = Math.floor(sec);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}
