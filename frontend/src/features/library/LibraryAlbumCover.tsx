import { memo } from "react";
import { useAlbumCoverBlobUrl } from "@/api/hooks";
import { cn } from "@/lib/utils";

type Props = {
  albumId: number;
  coverPath?: string | null;
  /** Applied to img on success; placeholder uses matching min size */
  className?: string;
};

const placeholderBase =
  "flex shrink-0 items-center justify-center overflow-hidden rounded-md border border-border bg-muted text-center text-[10px] leading-tight text-muted-foreground";

/**
 * Loads cover via authenticated fetch (img cannot send Bearer). Shows "No cover" when
 * `cover_path` is absent or GET returns 404.
 */
export const LibraryAlbumCover = memo(function LibraryAlbumCover({
  albumId,
  coverPath,
  className,
}: Props) {
  const trimmed = coverPath?.trim() ?? "";
  const placeholder = cn(placeholderBase, className ?? "size-24");
  const { data: src, isPending, isFetching, isError } = useAlbumCoverBlobUrl(
    albumId,
    trimmed,
  );

  if (!trimmed) {
    return (
      <div className={placeholder} data-testid="album-cover-placeholder">
        No cover
      </div>
    );
  }

  if ((isPending || isFetching) && !src) {
    return (
      <div className={placeholder} data-testid="album-cover-loading">
        …
      </div>
    );
  }

  if (isError || !src) {
    return (
      <div className={placeholder} data-testid="album-cover-placeholder">
        No cover
      </div>
    );
  }

  return (
    <img
      src={src}
      alt=""
      className={cn(
        "shrink-0 rounded-md border border-border object-cover",
        className ?? "size-24",
      )}
      data-testid="album-cover-image"
    />
  );
});
