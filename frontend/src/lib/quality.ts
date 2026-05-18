const STORAGE_KEY = "euterpe.defaultQuality";

export const QUALITY_OPTIONS = [
  { value: 5, label: "MP3 320" },
  { value: 6, label: "FLAC CD" },
  { value: 7, label: "FLAC Hi-Res" },
  { value: 27, label: "FLAC Hi-Res+" },
] as const;

export type QualityId = (typeof QUALITY_OPTIONS)[number]["value"];

export function getDefaultQuality(): QualityId {
  const raw = localStorage.getItem(STORAGE_KEY);
  const n = raw ? Number(raw) : 6;
  if (QUALITY_OPTIONS.some((o) => o.value === n)) {
    return n as QualityId;
  }
  return 6;
}

export function setDefaultQuality(value: QualityId) {
  localStorage.setItem(STORAGE_KEY, String(value));
}
