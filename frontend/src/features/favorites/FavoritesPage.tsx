import {
  flexRender,
  getCoreRowModel,
  useReactTable,
  type ColumnDef,
} from "@tanstack/react-table";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  useCreateDownloadByUrl,
  useCreateDownload,
  useFavoritesList,
  useQobuzSync,
  useRemoveFavorites,
  type FavoritesListQuery,
} from "@/api/hooks";
import type { QobuzFavoriteItem, SortOrder } from "@/api/client";
import { FavoriteAlbumCover } from "@/features/favorites/FavoriteAlbumCover";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useToast } from "@/hooks/use-toast";
import { getDefaultQuality } from "@/lib/quality";

const SORT_STORAGE_KEY = "euterpe.favorites.sort";
const ORDER_STORAGE_KEY = "euterpe.favorites.order";

type FavoritesSort = NonNullable<FavoritesListQuery["sort"]>;

function loadStoredSort(): FavoritesSort {
  const s = sessionStorage.getItem(SORT_STORAGE_KEY);
  if (s === "title" || s === "artist" || s === "in_library") return s;
  return "title";
}

function loadStoredOrder(): SortOrder {
  const o = sessionStorage.getItem(ORDER_STORAGE_KEY);
  return o === "desc" ? "desc" : "asc";
}

function SortableHeader({
  label,
  column,
  sort,
  order,
  onSort,
}: {
  label: string;
  column: FavoritesSort;
  sort: FavoritesSort;
  order: SortOrder;
  onSort: (col: FavoritesSort) => void;
}) {
  const active = sort === column;
  return (
    <button
      type="button"
      className="font-medium hover:underline"
      onClick={() => onSort(column)}
    >
      {label}
      {active ? (order === "asc" ? " ↑" : " ↓") : ""}
    </button>
  );
}

export function FavoritesPage() {
  const [sort, setSort] = useState<FavoritesSort>(loadStoredSort);
  const [order, setOrder] = useState<SortOrder>(loadStoredOrder);
  const [qInput, setQInput] = useState("");
  const [q, setQ] = useState("");
  const [inLibrary, setInLibrary] = useState<boolean | undefined>(undefined);
  const [rowSelection, setRowSelection] = useState<Record<string, boolean>>({});
  const [showDownloadUrl, setShowDownloadUrl] = useState(false);
  const [urlInput, setUrlInput] = useState("");

  useEffect(() => {
    const t = window.setTimeout(() => setQ(qInput.trim()), 300);
    return () => window.clearTimeout(t);
  }, [qInput]);

  useEffect(() => {
    sessionStorage.setItem(SORT_STORAGE_KEY, sort);
    sessionStorage.setItem(ORDER_STORAGE_KEY, order);
  }, [sort, order]);

  const listParams = useMemo(
    () => ({
      limit: 50,
      sort,
      order,
      q: q || undefined,
      in_library: inLibrary,
    }),
    [sort, order, q, inLibrary],
  );

  const favoritesQuery = useFavoritesList(listParams);
  const { items } = favoritesQuery;
  const initialLoading = favoritesQuery.isPending;
  const isRefetching = favoritesQuery.isFetching;
  const sync = useQobuzSync();
  const downloadByUrl = useCreateDownloadByUrl();
  const remove = useRemoveFavorites();
  const download = useCreateDownload();
  const { toast } = useToast();

  const onSort = useCallback((col: FavoritesSort) => {
    setSort((prev) => {
      if (prev === col) {
        setOrder((o) => (o === "asc" ? "desc" : "asc"));
        return prev;
      }
      setOrder("asc");
      return col;
    });
  }, []);

  const queueOne = useCallback(
    async (item: QobuzFavoriteItem) => {
      try {
        await download.mutateAsync({
          job_type: "album",
          album_api_id: item.album_api_id,
          qobuz_id: item.qobuz_id,
          quality: getDefaultQuality(),
        });
        toast({ title: "Download queued", description: item.title });
      } catch (e) {
        toast({
          title: "Queue failed",
          description: e instanceof Error ? e.message : "Error",
          variant: "destructive",
        });
      }
    },
    [download, toast],
  );

  const columns = useMemo<ColumnDef<QobuzFavoriteItem>[]>(
    () => [
      {
        id: "select",
        header: ({ table }) => (
          <Checkbox
            checked={table.getIsAllPageRowsSelected()}
            onCheckedChange={(v) => table.toggleAllPageRowsSelected(!!v)}
            aria-label="Select all"
          />
        ),
        cell: ({ row }) => (
          <Checkbox
            checked={row.getIsSelected()}
            onCheckedChange={(v) => row.toggleSelected(!!v)}
            aria-label="Select row"
          />
        ),
      },
      {
        id: "cover",
        header: "",
        cell: ({ row }) => <FavoriteAlbumCover item={row.original} />,
      },
      {
        accessorKey: "title",
        header: () => (
          <SortableHeader
            label="Title"
            column="title"
            sort={sort}
            order={order}
            onSort={onSort}
          />
        ),
      },
      {
        accessorKey: "artist_name",
        header: () => (
          <SortableHeader
            label="Artist"
            column="artist"
            sort={sort}
            order={order}
            onSort={onSort}
          />
        ),
      },
      {
        id: "in_library",
        header: () => (
          <SortableHeader
            label="In library"
            column="in_library"
            sort={sort}
            order={order}
            onSort={onSort}
          />
        ),
        cell: ({ row }) => (row.original.in_library ? "Yes" : "No"),
      },
      {
        id: "actions",
        header: "Actions",
        cell: ({ row }) => (
          <div className="flex gap-2">
            <Button
              size="sm"
              variant="secondary"
              disabled={download.isPending}
              onClick={() => void queueOne(row.original)}
            >
              Download
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={remove.isPending}
              onClick={() =>
                void remove.mutateAsync([row.original.qobuz_id]).catch((e) =>
                  toast({
                    title: "Remove failed",
                    description: String(e),
                    variant: "destructive",
                  }),
                )
              }
            >
              Remove
            </Button>
          </div>
        ),
      },
    ],
    [download.isPending, onSort, order, queueOne, remove, sort, toast],
  );

  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Table API
  const table = useReactTable({
    data: items,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (row) => String(row.qobuz_id),
    onRowSelectionChange: setRowSelection,
    state: { rowSelection },
  });

  async function bulkDownload() {
    const selected = table.getSelectedRowModel().rows.map((r) => r.original);
    for (const item of selected) {
      await queueOne(item);
    }
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-2xl font-semibold">Favorites</h2>
        <div className="flex flex-wrap gap-2">
          <Button
            disabled={sync.isPending}
            onClick={() =>
              void sync.mutateAsync().then((r) =>
                toast({
                  title: "Sync complete",
                  description: `+${r.added} / -${r.removed} (${r.albums_total} total)`,
                }),
              )
            }
          >
            Sync now
          </Button>
          <Button
            variant="outline"
            onClick={() => setShowDownloadUrl((v) => !v)}
          >
            Download by URL
          </Button>
          <Button
            variant="secondary"
            disabled={!table.getSelectedRowModel().rows.length}
            onClick={() => void bulkDownload()}
          >
            Bulk download
          </Button>
        </div>
      </div>

      {showDownloadUrl ? (
        <div className="flex flex-wrap items-end gap-2 rounded-lg border border-border/60 bg-card/30 p-3">
          <div className="min-w-[16rem] flex-1 space-y-1">
            <Label htmlFor="fav-download-url">Qobuz album URL</Label>
            <Input
              id="fav-download-url"
              value={urlInput}
              onChange={(e) => setUrlInput(e.target.value)}
              placeholder="https://play.qobuz.com/album/…"
              disabled={downloadByUrl.isPending}
            />
          </div>
          <Button
            disabled={!urlInput.trim() || downloadByUrl.isPending}
            onClick={() =>
              void downloadByUrl
                .mutateAsync(urlInput.trim())
                .then(() => {
                  setUrlInput("");
                  setShowDownloadUrl(false);
                  toast({ title: "Download queued" });
                })
                .catch((err: unknown) => {
                  const message =
                    err instanceof Error ? err.message : "Could not queue download";
                  toast({
                    title: "Queue failed",
                    description: message,
                    variant: "destructive",
                  });
                })
            }
          >
            Download
          </Button>
          <Button
            variant="ghost"
            disabled={downloadByUrl.isPending}
            onClick={() => {
              setShowDownloadUrl(false);
              setUrlInput("");
            }}
          >
            Cancel
          </Button>
        </div>
      ) : null}

      <div className="flex flex-wrap items-end gap-4">
        <div className="min-w-[12rem] flex-1 space-y-1">
          <Label htmlFor="fav-search">Search</Label>
          <Input
            id="fav-search"
            value={qInput}
            onChange={(e) => setQInput(e.target.value)}
            placeholder="Title or artist"
          />
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            variant={inLibrary === undefined ? "secondary" : "outline"}
            onClick={() => setInLibrary(undefined)}
          >
            All
          </Button>
          <Button
            type="button"
            size="sm"
            variant={inLibrary === true ? "secondary" : "outline"}
            onClick={() => setInLibrary(true)}
          >
            In library
          </Button>
          <Button
            type="button"
            size="sm"
            variant={inLibrary === false ? "secondary" : "outline"}
            onClick={() => setInLibrary(false)}
          >
            Not in library
          </Button>
        </div>
      </div>

      {initialLoading ? (
        <p className="text-muted-foreground">Loading…</p>
      ) : (
        <>
          {isRefetching ? (
            <p className="text-sm text-muted-foreground">Searching…</p>
          ) : null}
          <div
            className={
              isRefetching ? "pointer-events-none rounded-lg opacity-60" : ""
            }
          >
          <div className="overflow-hidden rounded-lg border border-border">
            <table className="w-full text-sm">
              <thead className="bg-muted/50">
                {table.getHeaderGroups().map((hg) => (
                  <tr key={hg.id}>
                    {hg.headers.map((h) => (
                      <th key={h.id} className="px-3 py-2 text-left">
                        {flexRender(h.column.columnDef.header, h.getContext())}
                      </th>
                    ))}
                  </tr>
                ))}
              </thead>
              <tbody>
                {table.getRowModel().rows.map((row) => (
                  <tr key={row.id} className="border-t border-border">
                    {row.getVisibleCells().map((cell) => (
                      <td key={cell.id} className="px-3 py-2">
                        {flexRender(
                          cell.column.columnDef.cell,
                          cell.getContext(),
                        )}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          </div>
          {favoritesQuery.hasNextPage ? (
            <Button
              variant="outline"
              disabled={favoritesQuery.isFetchingNextPage}
              onClick={() => void favoritesQuery.fetchNextPage()}
            >
              {favoritesQuery.isFetchingNextPage ? "Loading…" : "Load more"}
            </Button>
          ) : null}
        </>
      )}
    </div>
  );
}
