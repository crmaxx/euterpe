import { memo } from "react";
import type { QobuzFavoriteItem } from "@/api/client";
import { LibraryAlbumCover } from "@/features/library/LibraryAlbumCover";
import { cn } from "@/lib/utils";

const placeholderClass =
  "inline-block h-10 w-10 shrink-0 rounded-md border border-border bg-muted";

type Props = {
  item: QobuzFavoriteItem;
  className?: string;
};

/** Stable cover cell — avoids remounting fetches when the table re-renders. */
export const FavoriteAlbumCover = memo(function FavoriteAlbumCover({
  item,
  className = "h-10 w-10",
}: Props) {
  if (item.in_library && item.local_album_id != null) {
    return (
      <LibraryAlbumCover albumId={item.local_album_id} className={className} />
    );
  }

  if (item.cover_url) {
    return (
      <img
        key={item.cover_url}
        src={item.cover_url}
        alt=""
        width={40}
        height={40}
        className={cn(
          "shrink-0 rounded-md border border-border object-cover bg-muted",
          className,
        )}
        loading="eager"
        decoding="async"
        referrerPolicy="no-referrer"
      />
    );
  }

  return <span className={cn(placeholderClass, className)} aria-hidden />;
});
