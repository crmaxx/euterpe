import { getAdminToken } from "@/lib/auth";

/** In-flight fetches keyed by album + cover path (dedupes list + detail pane). */
const inflightByKey = new Map<string, Promise<string | null>>();

/** Blob URLs keyed by `albumCoverCacheKey(albumId, coverPath)`. */
const blobUrlByKey = new Map<string, string>();

export function albumCoverCacheKey(albumId: number, coverPath: string): string {
  return `${albumId}|${coverPath.trim()}`;
}

export function getAlbumCoverBlobUrl(key: string): string | undefined {
  return blobUrlByKey.get(key);
}

export function setAlbumCoverBlobUrl(key: string, url: string): void {
  const prev = blobUrlByKey.get(key);
  if (prev && prev !== url) {
    URL.revokeObjectURL(prev);
  }
  blobUrlByKey.set(key, url);
}

/** After cover replace: drop cached blobs for this album (any path). */
export function revokeAlbumCoverBlobs(albumId: number): void {
  const prefix = `${albumId}|`;
  for (const [key, url] of blobUrlByKey.entries()) {
    if (key.startsWith(prefix)) {
      URL.revokeObjectURL(url);
      blobUrlByKey.delete(key);
    }
  }
}

export async function fetchAlbumCoverBlobUrl(
  albumId: number,
  coverPath: string,
  signal?: AbortSignal,
): Promise<string | null> {
  const key = albumCoverCacheKey(albumId, coverPath);
  const cached = getAlbumCoverBlobUrl(key);
  if (cached) return cached;

  const inflight = inflightByKey.get(key);
  if (inflight) return inflight;

  const promise = (async () => {
    const headers = new Headers();
    const token = getAdminToken();
    if (token) headers.set("Authorization", `Bearer ${token}`);

    const res = await fetch(`/api/v1/library/albums/${albumId}/cover`, {
      headers,
      signal,
    });
    if (res.status === 404) return null;
    if (!res.ok) {
      throw new Error(`cover fetch failed: ${res.status}`);
    }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    setAlbumCoverBlobUrl(key, url);
    return url;
  })().finally(() => {
    inflightByKey.delete(key);
  });

  inflightByKey.set(key, promise);
  return promise;
}
