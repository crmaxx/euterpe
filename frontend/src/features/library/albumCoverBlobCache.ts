import { getAdminToken } from "@/lib/auth";

/** In-flight fetches keyed by album + cover path (dedupes list + detail pane). */
const inflightByKey = new Map<string, Promise<string | null>>();

/** Blob URLs keyed by `albumCoverCacheKey(albumId)`. */
const blobUrlByKey = new Map<string, string>();

/** Keys that failed (404, bad bytes, or img onError) — no further fetch attempts. */
const failedKeys = new Set<string>();

let cacheEpoch = 0;
const cacheListeners = new Set<() => void>();

function bumpCacheEpoch(): void {
  cacheEpoch += 1;
  for (const listener of cacheListeners) {
    listener();
  }
}

/** Re-render cover cells when a blob URL is stored outside React Query. */
export function subscribeAlbumCoverCache(listener: () => void): () => void {
  cacheListeners.add(listener);
  return () => {
    cacheListeners.delete(listener);
  };
}

export function getAlbumCoverCacheEpoch(): number {
  return cacheEpoch;
}

/** Cover API is per album; path is only for UI, not for cache identity. */
export function albumCoverCacheKey(albumId: number): string {
  return String(albumId);
}

export function externalCoverCacheKey(url: string): string {
  return `ext:${url.trim()}`;
}

export function isAlbumCoverFailed(albumId: number): boolean {
  return failedKeys.has(albumCoverCacheKey(albumId));
}

export function markAlbumCoverFailed(albumId: number): void {
  failedKeys.add(albumCoverCacheKey(albumId));
}

export function clearAlbumCoverFailed(albumId: number): void {
  const idKey = String(albumId);
  failedKeys.delete(idKey);
  const legacyPrefix = `${albumId}|`;
  for (const key of [...failedKeys]) {
    if (key.startsWith(legacyPrefix)) {
      failedKeys.delete(key);
    }
  }
}

export function isExternalCoverFailed(url: string): boolean {
  const trimmed = url.trim();
  return trimmed.length === 0 || failedKeys.has(externalCoverCacheKey(trimmed));
}

export function markExternalCoverFailed(url: string): void {
  const trimmed = url.trim();
  if (trimmed.length > 0) {
    failedKeys.add(externalCoverCacheKey(trimmed));
  }
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
  bumpCacheEpoch();
}

/** Clear all cached cover blob URLs (tests). */
export function clearAlbumCoverBlobCache(): void {
  for (const url of blobUrlByKey.values()) {
    URL.revokeObjectURL(url);
  }
  blobUrlByKey.clear();
  inflightByKey.clear();
  failedKeys.clear();
  bumpCacheEpoch();
}

/** After cover replace: drop cached blobs and failure marks for this album. */
export function revokeAlbumCoverBlobs(albumId: number): void {
  const idKey = String(albumId);
  const legacyPrefix = `${albumId}|`;
  for (const key of [...failedKeys]) {
    if (key === idKey || key.startsWith(legacyPrefix)) {
      failedKeys.delete(key);
    }
  }
  for (const [key, url] of blobUrlByKey.entries()) {
    if (key === idKey || key.startsWith(legacyPrefix)) {
      URL.revokeObjectURL(url);
      blobUrlByKey.delete(key);
    }
  }
  for (const key of [...inflightByKey.keys()]) {
    if (key === idKey || key.startsWith(legacyPrefix)) {
      inflightByKey.delete(key);
    }
  }
  bumpCacheEpoch();
}

/** True if this blob URL is still the cached cover for the album. */
export function isActiveAlbumCoverBlobUrl(
  albumId: number,
  url: string | null | undefined,
): boolean {
  if (url == null) return true;
  return getAlbumCoverBlobUrl(albumCoverCacheKey(albumId)) === url;
}

export function hasImageMagic(head: Uint8Array): boolean {
  if (head.length < 3) return false;
  // JPEG
  if (head[0] === 0xff && head[1] === 0xd8 && head[2] === 0xff) return true;
  // PNG
  if (
    head.length >= 4 &&
    head[0] === 0x89 &&
    head[1] === 0x50 &&
    head[2] === 0x4e &&
    head[3] === 0x47
  ) {
    return true;
  }
  // GIF
  if (
    head.length >= 4 &&
    head[0] === 0x47 &&
    head[1] === 0x49 &&
    head[2] === 0x46 &&
    head[3] === 0x38
  ) {
    return true;
  }
  // BMP
  if (head[0] === 0x42 && head[1] === 0x4d) return true;
  // WEBP (RIFF....WEBP)
  if (
    head.length >= 12 &&
    head[0] === 0x52 &&
    head[1] === 0x49 &&
    head[2] === 0x46 &&
    head[3] === 0x46 &&
    head[8] === 0x57 &&
    head[9] === 0x45 &&
    head[10] === 0x42 &&
    head[11] === 0x50
  ) {
    return true;
  }
  return false;
}

async function isImageBlob(blob: Blob): Promise<boolean> {
  if (blob.size === 0) return false;
  if (blob.type.startsWith("image/")) return true;
  if (blob.type !== "" && blob.type !== "application/octet-stream") {
    return false;
  }
  const buffer = await blob.arrayBuffer();
  const head = new Uint8Array(buffer, 0, Math.min(12, buffer.byteLength));
  return hasImageMagic(head);
}

export async function fetchAlbumCoverBlobUrl(albumId: number): Promise<string | null> {
  const key = albumCoverCacheKey(albumId);
  if (failedKeys.has(key)) {
    return null;
  }

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
    });
    if (res.status === 404) {
      return null;
    }
    if (res.status === 401 || res.status === 403) {
      throw new Error(`cover fetch unauthorized: ${res.status}`);
    }
    if (!res.ok) {
      throw new Error(`cover fetch failed: ${res.status}`);
    }
    const blob = await res.blob();
    if (!(await isImageBlob(blob))) {
      markAlbumCoverFailed(albumId);
      return null;
    }
    const url = URL.createObjectURL(blob);
    setAlbumCoverBlobUrl(key, url);
    return url;
  })().finally(() => {
    inflightByKey.delete(key);
  });

  inflightByKey.set(key, promise);
  return promise;
}
