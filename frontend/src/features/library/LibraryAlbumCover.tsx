import { useEffect, useRef, useState } from "react";
import { getAdminToken } from "@/lib/auth";
import { cn } from "@/lib/utils";

type Props = {
  albumId: number;
  coverPath?: string | null;
  /** Applied to img on success; placeholder uses matching min size */
  className?: string;
};

const placeholderBase =
  "flex shrink-0 items-center justify-center overflow-hidden rounded-md border border-border bg-muted text-center text-[10px] leading-tight text-muted-foreground";

/** Survives table remounts (e.g. favorites search refetch) without revoking active blob URLs. */
const blobUrlByAlbumId = new Map<number, string>();

/**
 * Loads cover via authenticated fetch (img cannot send Bearer). Shows "No cover" when
 * `cover_path` is absent or GET returns 404.
 */
export function LibraryAlbumCover({ albumId, coverPath, className }: Props) {
  const trimmed = coverPath?.trim() ?? "";
  const placeholder = cn(placeholderBase, className ?? "size-24");

  if (!trimmed) {
    return (
      <div className={placeholder} data-testid="album-cover-placeholder">
        No cover
      </div>
    );
  }

  return (
    <LibraryAlbumCoverFetched
      key={`${albumId}-${trimmed}`}
      albumId={albumId}
      className={className}
    />
  );
}

/** Only mounted when `cover_path` is non-empty; fetches blob without sync setState in effect. */
function LibraryAlbumCoverFetched({
  albumId,
  className,
}: {
  albumId: number;
  className?: string;
}) {
  const cachedUrl = blobUrlByAlbumId.get(albumId);
  const [src, setSrc] = useState<string | null>(() => cachedUrl ?? null);
  const [phase, setPhase] = useState<"loading" | "ready" | "missing">(() =>
    cachedUrl ? "ready" : "loading",
  );
  const objectUrlRef = useRef<string | null>(cachedUrl ?? null);

  const placeholder = cn(placeholderBase, className ?? "size-24");

  useEffect(() => {
    if (phase !== "loading") return;

    let cancelled = false;
    const ac = new AbortController();

    const headers = new Headers();
    const token = getAdminToken();
    if (token) headers.set("Authorization", `Bearer ${token}`);

    void (async () => {
      try {
        const res = await fetch(`/api/v1/library/albums/${albumId}/cover`, {
          headers,
          signal: ac.signal,
        });
        if (cancelled) return;
        if (res.status === 404 || !res.ok) {
          setPhase("missing");
          return;
        }
        const blob = await res.blob();
        if (cancelled) return;
        const url = URL.createObjectURL(blob);
        const prev = blobUrlByAlbumId.get(albumId);
        if (prev && prev !== url) {
          URL.revokeObjectURL(prev);
        }
        blobUrlByAlbumId.set(albumId, url);
        objectUrlRef.current = url;
        setSrc(url);
        setPhase("ready");
      } catch {
        if (!cancelled) setPhase("missing");
      }
    })();

    return () => {
      cancelled = true;
      ac.abort();
    };
  }, [albumId, phase]);

  if (phase === "missing") {
    return (
      <div className={placeholder} data-testid="album-cover-placeholder">
        No cover
      </div>
    );
  }

  if (phase === "loading") {
    return (
      <div className={placeholder} data-testid="album-cover-loading">
        …
      </div>
    );
  }

  return (
    <img
      src={src!}
      alt=""
      className={cn(
        "shrink-0 rounded-md border border-border object-cover",
        className ?? "size-24",
      )}
      data-testid="album-cover-image"
    />
  );
}
