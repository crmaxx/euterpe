import { memo, useState } from "react";
import type { QobuzFavoriteItem } from "@/api/client";
import { LibraryAlbumCover } from "@/features/library/LibraryAlbumCover";
import {
  isExternalCoverFailed,
  markExternalCoverFailed,
} from "@/features/library/albumCoverBlobCache";
import { cn } from "@/lib/utils";

const placeholderClass =
  "inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-md border border-border bg-muted text-center text-[10px] leading-tight text-muted-foreground";

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
      <LibraryAlbumCover
        albumId={item.local_album_id}
        coverPath={item.local_cover_path}
        fallbackCoverUrl={item.cover_url}
        className={className}
      />
    );
  }

  const url = item.cover_url?.trim() ?? "";
  if (!url) {
    return <span className={cn(placeholderClass, className)} aria-hidden />;
  }

  return <QobuzCoverImage url={url} className={className} />;
}, favoriteCoverPropsEqual);

function favoriteCoverPropsEqual(prev: Props, next: Props): boolean {
  if (prev.className !== next.className) return false;
  const a = prev.item;
  const b = next.item;
  return (
    a.qobuz_id === b.qobuz_id &&
    a.in_library === b.in_library &&
    a.local_album_id === b.local_album_id &&
    a.local_cover_path === b.local_cover_path &&
    a.cover_url === b.cover_url
  );
}

const QobuzCoverImage = memo(function QobuzCoverImage({
  url,
  className,
}: {
  url: string;
  className: string;
}) {
  const [failed, setFailed] = useState(() => isExternalCoverFailed(url));

  if (failed) {
    return (
      <span
        className={cn(placeholderClass, className)}
        data-testid="album-cover-placeholder"
      >
        No cover
      </span>
    );
  }

  return (
    <img
      src={url}
      alt=""
      width={40}
      height={40}
      className={cn(
        "shrink-0 rounded-md border border-border object-cover bg-muted",
        className,
      )}
      loading="lazy"
      decoding="async"
      referrerPolicy="no-referrer"
      onError={() => {
        markExternalCoverFailed(url);
        setFailed(true);
      }}
    />
  );
});
