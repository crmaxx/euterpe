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
