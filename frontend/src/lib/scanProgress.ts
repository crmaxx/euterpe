export type ScanProgressSample = {
  t: number;
  filesIndexed: number;
  filesTotal: number;
};

/** Progress bar: indexed / total when total known; else undefined (indeterminate). */
export function scanProgressPercent(
  filesIndexed: number,
  filesTotal: number,
): number | undefined {
  if (filesTotal <= 0) return undefined;
  return Math.min(100, Math.round((filesIndexed / filesTotal) * 100));
}

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "—";
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))}s`;
  const m = Math.floor(seconds / 60);
  const s = Math.round(seconds % 60);
  if (m < 60) return s > 0 ? `${m}m ${s}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const rm = m % 60;
  return rm > 0 ? `${h}h ${rm}m` : `${h}h`;
}

/**
 * ETA from recent indexed rate and remaining work (total − indexed).
 * Requires files_total > 0 (enumerate finished).
 */
export function estimateScanEta(
  filesIndexed: number,
  filesTotal: number,
  samples: ScanProgressSample[],
): string | null {
  if (filesTotal <= 0 || filesIndexed < 1 || samples.length < 2) return null;

  const now = samples[samples.length - 1]!.t;
  const window = samples.filter((s) => now - s.t <= 20_000);
  if (window.length < 2) return null;

  const first = window[0]!;
  const last = window[window.length - 1]!;
  const dt = (last.t - first.t) / 1000;
  if (dt < 0.5) return null;

  const indexRate = (last.filesIndexed - first.filesIndexed) / dt;
  if (indexRate < 0.05) return null;

  const remaining = Math.max(0, filesTotal - filesIndexed);
  if (remaining <= 0) return null;

  return `~${formatDuration(remaining / indexRate)}`;
}
