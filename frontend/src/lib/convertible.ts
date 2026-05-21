/** Extensions that the server converts to FLAC (see `euterpe_converter::is_convertible_extension`). */
const CONVERTIBLE_EXTENSIONS = new Set([
  "wav",
  "wave",
  "m4a",
  "mp4",
  "caf",
  "ape",
]);

export function isConvertiblePath(path: string): boolean {
  const base = path.replace(/\\/g, "/").split("/").pop() ?? path;
  const dot = base.lastIndexOf(".");
  if (dot < 0) return false;
  return CONVERTIBLE_EXTENSIONS.has(base.slice(dot + 1).toLowerCase());
}

export function normalizeTrackPath(path: string): string {
  return path.replace(/\\/g, "/");
}
