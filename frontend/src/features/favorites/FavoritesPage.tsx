import {
  flexRender,
  getCoreRowModel,
  useReactTable,
  type ColumnDef,
} from "@tanstack/react-table";
import {
  createContext,
  memo,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  useCreateDownloadByUrl,
  useActiveAlbumDownloadQobuzIds,
  useCreateDownload,
  useFavoritesList,
  useQobuzSync,
  useRemoveFavorites,
  type FavoritesListQuery,
} from "@/api/hooks";
import type { QobuzFavoriteItem, SortOrder } from "@/api/client";
import { FavoriteAlbumCover } from "@/features/favorites/FavoriteAlbumCover";
import { Loader2 } from "lucide-react";
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

const FavoritesOptimisticBusyContext = createContext<ReadonlySet<number>>(
  new Set(),
);

const FavoritesActiveDownloadIdsContext = createContext<ReadonlySet<number>>(
  new Set(),
);

type FavoritesBusyApi = {
  addOptimisticBusy: (qobuzId: number) => void;
  removeOptimisticBusy: (qobuzId: number) => void;
};

const FavoritesBusyApiContext = createContext<FavoritesBusyApi | null>(null);

/** Polls downloads every 3s; table content is memoized so covers do not re-render. */
function FavoritesDownloadLayer({ children }: { children: React.ReactNode }) {
  const activeDownloadIds = useActiveAlbumDownloadQobuzIds();
  const [optimisticRaw, setOptimisticRaw] = useState<Set<number>>(() => new Set());
  const [seenInActive, setSeenInActive] = useState<Set<number>>(() => new Set());

  const activeKey = useMemo(
    () => [...activeDownloadIds].sort((a, b) => a - b).join(","),
    [activeDownloadIds],
  );
  const [prevActiveKey, setPrevActiveKey] = useState(activeKey);
  if (prevActiveKey !== activeKey) {
    setPrevActiveKey(activeKey);
    setSeenInActive((prev) => {
      let changed = false;
      const next = new Set(prev);
      for (const id of activeDownloadIds) {
        if (!next.has(id)) {
          next.add(id);
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }

  const optimisticBusy = useMemo(() => {
    const visible = new Set<number>();
    for (const id of optimisticRaw) {
      if (activeDownloadIds.has(id) || !seenInActive.has(id)) {
        visible.add(id);
      }
    }
    return visible;
  }, [optimisticRaw, seenInActive, activeDownloadIds]);

  const addOptimisticBusy = useCallback((qobuzId: number) => {
    setOptimisticRaw((prev) => {
      if (prev.has(qobuzId)) return prev;
      const next = new Set(prev);
      next.add(qobuzId);
      return next;
    });
  }, []);

  const removeOptimisticBusy = useCallback((qobuzId: number) => {
    setOptimisticRaw((prev) => {
      if (!prev.has(qobuzId)) return prev;
      const next = new Set(prev);
      next.delete(qobuzId);
      return next;
    });
  }, []);

  const busyApi = useMemo(
    () => ({ addOptimisticBusy, removeOptimisticBusy }),
    [addOptimisticBusy, removeOptimisticBusy],
  );

  return (
    <FavoritesBusyApiContext.Provider value={busyApi}>
      <FavoritesActiveDownloadIdsContext.Provider value={activeDownloadIds}>
        <FavoritesOptimisticBusyContext.Provider value={optimisticBusy}>
          {children}
        </FavoritesOptimisticBusyContext.Provider>
      </FavoritesActiveDownloadIdsContext.Provider>
    </FavoritesBusyApiContext.Provider>
  );
}

/** Subscribes to download polling; keeps cover column cells from remounting. */
function FavoriteRowActions({
  item,
  onQueue,
  removePending,
  onRemove,
}: {
  item: QobuzFavoriteItem;
  onQueue: (item: QobuzFavoriteItem) => void;
  removePending: boolean;
  onRemove: (qobuzId: number) => void;
}) {
  const optimisticBusy = useContext(FavoritesOptimisticBusyContext);
  const activeDownloadIds = useContext(FavoritesActiveDownloadIdsContext);
  const busy =
    activeDownloadIds.has(item.qobuz_id) || optimisticBusy.has(item.qobuz_id);
  const label = item.in_library ? "Re-download" : "Download";
  return (
    <div className="flex gap-2">
      <Button
        size="sm"
        variant="secondary"
        disabled={busy}
        aria-label={busy ? "Downloading" : label}
        onClick={() => void onQueue(item)}
      >
        {busy ? (
          <>
            <Loader2 className="mr-1 size-3.5 animate-spin" aria-hidden />
            Downloading…
          </>
        ) : (
          label
        )}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        disabled={removePending}
        onClick={() => onRemove(item.qobuz_id)}
      >
        Remove
      </Button>
    </div>
  );
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

const coverColumn: ColumnDef<QobuzFavoriteItem> = {
  id: "cover",
  header: "",
  cell: ({ row }) => <FavoriteAlbumCover item={row.original} />,
};

export function FavoritesPage() {
  return (
    <FavoritesDownloadLayer>
      <FavoritesPageContent />
    </FavoritesDownloadLayer>
  );
}

const FavoritesPageContent = memo(function FavoritesPageContent() {
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
  const showSearching =
    favoritesQuery.isFetching &&
    !favoritesQuery.isPending &&
    q.length > 0;
  const sync = useQobuzSync();
  const downloadByUrl = useCreateDownloadByUrl();
  const remove = useRemoveFavorites();
  const download = useCreateDownload();
  const busyApi = useContext(FavoritesBusyApiContext);
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
      busyApi?.addOptimisticBusy(item.qobuz_id);
      try {
        await download.mutateAsync({
          job_type: "album",
          album_api_id: item.album_api_id,
          qobuz_id: item.qobuz_id,
          quality: getDefaultQuality(),
        });
        toast({ title: "Download queued", description: item.title });
      } catch (e) {
        busyApi?.removeOptimisticBusy(item.qobuz_id);
        toast({
          title: "Queue failed",
          description: e instanceof Error ? e.message : "Error",
          variant: "destructive",
        });
      }
    },
    [busyApi, download, toast],
  );

  const handleRemoveFavorite = useCallback(
    (qobuzId: number) => {
      void remove.mutateAsync([qobuzId]).catch((e) =>
        toast({
          title: "Remove failed",
          description: String(e),
          variant: "destructive",
        }),
      );
    },
    [remove, toast],
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
      coverColumn,
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
          <FavoriteRowActions
            item={row.original}
            onQueue={queueOne}
            removePending={remove.isPending}
            onRemove={handleRemoveFavorite}
          />
        ),
      },
    ],
    [handleRemoveFavorite, onSort, order, queueOne, remove.isPending, sort],
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
          {showSearching ? (
            <p className="text-sm text-muted-foreground">Searching…</p>
          ) : null}
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
});
