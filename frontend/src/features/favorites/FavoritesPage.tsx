import {
  flexRender,
  getCoreRowModel,
  useReactTable,
  type ColumnDef,
} from "@tanstack/react-table";
import { useCallback, useMemo, useState } from "react";
import {
  useCreateDownload,
  useFavorites,
  useQobuzSync,
  useRemoveFavorites,
} from "@/api/hooks";
import type { QobuzFavoriteItem } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { useToast } from "@/hooks/use-toast";
import { getDefaultQuality } from "@/lib/quality";

export function FavoritesPage() {
  const { data, isLoading } = useFavorites();
  const sync = useQobuzSync();
  const remove = useRemoveFavorites();
  const download = useCreateDownload();
  const { toast } = useToast();
  const [rowSelection, setRowSelection] = useState<Record<string, boolean>>({});

  const items = data?.items ?? [];

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
      { accessorKey: "title", header: "Title" },
      { accessorKey: "artist_name", header: "Artist" },
      {
        id: "in_library",
        header: "In library",
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
    [download.isPending, queueOne, remove, toast],
  );

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
          <Button variant="outline" disabled title="Phase 2 API">
            Add by URL
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

      {isLoading ? (
        <p className="text-muted-foreground">Loading…</p>
      ) : (
        <div className="overflow-hidden rounded-lg border border-border">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              {table.getHeaderGroups().map((hg) => (
                <tr key={hg.id}>
                  {hg.headers.map((h) => (
                    <th key={h.id} className="px-3 py-2 text-left font-medium">
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
                      {flexRender(cell.column.columnDef.cell, cell.getContext())}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
