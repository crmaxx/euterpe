import { memo, useState } from "react";
import { useAlbumCoverBlobUrl } from "@/api/hooks";
import {
  albumCoverCacheKey,
  getAlbumCoverBlobUrl,
  isAlbumCoverFailed,
  isExternalCoverFailed,
  markAlbumCoverFailed,
  markExternalCoverFailed,
} from "@/features/library/albumCoverBlobCache";
import { cn } from "@/lib/utils";

type Props = {
  albumId: number;
  coverPath?: string | null;
  /** Qobuz URL when local cover is missing or fetch failed (e.g. favorites in library). */
  fallbackCoverUrl?: string | null;
  /** Applied to img on success; placeholder uses matching min size */
  className?: string;
};

const placeholderBase =
  "flex shrink-0 items-center justify-center overflow-hidden rounded-md border border-border bg-muted text-center text-[10px] leading-tight text-muted-foreground";

const coverImgClass =
  "shrink-0 rounded-md border border-border object-cover bg-muted";

function CoverPlaceholder({ className }: { className?: string }) {
  return (
    <div
      className={cn(placeholderBase, className ?? "size-24")}
      data-testid="album-cover-placeholder"
    >
      No cover
    </div>
  );
}

/**
 * Loads cover via authenticated fetch (img cannot send Bearer). Tries
 * `GET /library/albums/{id}/cover` once; optional Qobuz fallback URL once.
 * After any failure, shows "No cover" and does not retry.
 */
export const LibraryAlbumCover = memo(function LibraryAlbumCover({
  albumId,
  coverPath,
  fallbackCoverUrl,
  className,
}: Props) {
  const trimmed = coverPath?.trim() ?? "";
  const fallback = fallbackCoverUrl?.trim() ?? "";
  const cacheKey = albumCoverCacheKey(albumId);

  const libraryFailed = isAlbumCoverFailed(albumId);
  const fallbackFailed = fallback.length > 0 && isExternalCoverFailed(fallback);

  const [forcePlaceholder, setForcePlaceholder] = useState(false);
  const [localBlobBroken, setLocalBlobBroken] = useState(false);

  const { data: src, isPending, isFetched, isError, isFetching } =
    useAlbumCoverBlobUrl(albumId, trimmed);
  const displaySrc = src ?? getAlbumCoverBlobUrl(cacheKey);
  const loading = (isPending || isFetching) && !displaySrc;

  const giveUp = () => {
    markAlbumCoverFailed(albumId);
    if (fallback) markExternalCoverFailed(fallback);
    setForcePlaceholder(true);
  };

  const showPlaceholder =
    forcePlaceholder ||
    (!loading &&
      libraryFailed &&
      !displaySrc &&
      (!fallback || fallbackFailed)) ||
    (!loading && isFetched && !displaySrc && !fallback) ||
    (!loading && isError && !displaySrc && !fallback);

  if (showPlaceholder) {
    return <CoverPlaceholder className={className} />;
  }

  if (!libraryFailed && !isError && loading) {
    return (
      <div
        className={cn(placeholderBase, className ?? "size-24")}
        data-testid="album-cover-loading"
      >
        …
      </div>
    );
  }

  const imgClass = cn(coverImgClass, className ?? "size-24");

  if (displaySrc && !localBlobBroken) {
    return (
      <img
        src={displaySrc}
        alt=""
        className={imgClass}
        data-testid="album-cover-image"
        onError={() => {
          markAlbumCoverFailed(albumId);
          if (fallback && !isExternalCoverFailed(fallback)) {
            setLocalBlobBroken(true);
          } else {
            giveUp();
          }
        }}
      />
    );
  }

  const showFallback =
    fallback.length > 0 &&
    !fallbackFailed &&
    (libraryFailed ||
      localBlobBroken ||
      isError ||
      (isFetched && !displaySrc));

  if (showFallback) {
    return (
      <img
        src={fallback}
        alt=""
        width={40}
        height={40}
        className={imgClass}
        loading="lazy"
        decoding="async"
        referrerPolicy="no-referrer"
        data-testid="album-cover-fallback"
        onError={giveUp}
      />
    );
  }

  return <CoverPlaceholder className={className} />;
});
