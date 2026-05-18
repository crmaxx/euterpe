import { useState } from "react";
import {
  useLibraryAlbum,
  useLibraryAlbums,
  useLibraryTrack,
  usePatchTrackTags,
  useScanLatest,
  useStartLibraryScan,
} from "@/api/hooks";
import type { LibraryTrackDetailResponse, LibraryTrackTagsPatchRequest } from "@/api/client";
import { LibraryAlbumCover } from "@/features/library/LibraryAlbumCover";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useToast } from "@/hooks/use-toast";

function trackToTagForm(d: LibraryTrackDetailResponse): LibraryTrackTagsPatchRequest {
  return {
    title: d.title,
    artist_name: d.artist_name,
    album_title: d.album_title,
    track_number: d.track_number ?? undefined,
    year: d.year ?? undefined,
    disc_number: d.disc_number ?? undefined,
    genre: d.genre ?? undefined,
  };
}

function TrackTagsEditorForm({
  trackId,
  track,
  onClose,
  patchTags,
  toast,
}: {
  trackId: number;
  track: LibraryTrackDetailResponse;
  onClose: () => void;
  patchTags: ReturnType<typeof usePatchTrackTags>;
  toast: ReturnType<typeof useToast>["toast"];
}) {
  const [tagForm, setTagForm] = useState<LibraryTrackTagsPatchRequest>(() =>
    trackToTagForm(track),
  );

  async function handleSave() {
    try {
      await patchTags.mutateAsync({ id: trackId, body: tagForm });
      toast({ title: "Tags saved" });
      onClose();
    } catch (e) {
      toast({
        title: "Save failed",
        description: e instanceof Error ? e.message : "Unknown error",
        variant: "destructive",
      });
    }
  }

  return (
    <>
      <h3 className="font-medium">Edit track tags</h3>
      <div className="space-y-2">
        <Label htmlFor="tag-title">Title</Label>
        <Input
          id="tag-title"
          value={tagForm.title ?? ""}
          onChange={(e) => setTagForm((f) => ({ ...f, title: e.target.value }))}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="tag-artist">Artist</Label>
        <Input
          id="tag-artist"
          value={tagForm.artist_name ?? ""}
          onChange={(e) =>
            setTagForm((f) => ({ ...f, artist_name: e.target.value }))
          }
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="tag-album">Album</Label>
        <Input
          id="tag-album"
          value={tagForm.album_title ?? ""}
          onChange={(e) =>
            setTagForm((f) => ({ ...f, album_title: e.target.value }))
          }
        />
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-2">
          <Label htmlFor="tag-track">Track #</Label>
          <Input
            id="tag-track"
            type="number"
            min={1}
            value={tagForm.track_number ?? ""}
            onChange={(e) => {
              const v = e.target.value;
              setTagForm((f) => ({
                ...f,
                track_number: v === "" ? undefined : Number.parseInt(v, 10),
              }));
            }}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="tag-disc">Disc #</Label>
          <Input
            id="tag-disc"
            type="number"
            min={1}
            value={tagForm.disc_number ?? ""}
            onChange={(e) => {
              const v = e.target.value;
              setTagForm((f) => ({
                ...f,
                disc_number: v === "" ? undefined : Number.parseInt(v, 10),
              }));
            }}
          />
        </div>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-2">
          <Label htmlFor="tag-year">Year</Label>
          <Input
            id="tag-year"
            type="number"
            value={tagForm.year ?? ""}
            onChange={(e) => {
              const v = e.target.value;
              setTagForm((f) => ({
                ...f,
                year: v === "" ? undefined : Number.parseInt(v, 10),
              }));
            }}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="tag-genre">Genre</Label>
          <Input
            id="tag-genre"
            value={tagForm.genre ?? ""}
            onChange={(e) =>
              setTagForm((f) => ({ ...f, genre: e.target.value }))
            }
          />
        </div>
      </div>
      <p className="text-xs text-muted-foreground">
        Clear genre field and save to remove genre from the file.
      </p>
      <div className="flex justify-end gap-2">
        <Button type="button" variant="secondary" onClick={onClose}>
          Cancel
        </Button>
        <Button
          type="button"
          disabled={patchTags.isPending}
          onClick={() => void handleSave()}
        >
          Save
        </Button>
      </div>
    </>
  );
}

export function LibraryPage() {
  const { toast } = useToast();
  const [search, setSearch] = useState("");
  const [selectedAlbumId, setSelectedAlbumId] = useState<number | null>(null);
  const [editingTrackId, setEditingTrackId] = useState<number | null>(null);

  const { data: scan } = useScanLatest();
  const startScan = useStartLibraryScan();
  const { data: albums, isLoading } = useLibraryAlbums(0, 100, search);
  const { data: albumDetail } = useLibraryAlbum(selectedAlbumId);
  const trackQuery = useLibraryTrack(editingTrackId);
  const patchTags = usePatchTrackTags();

  const scanRunning = scan?.run?.status === "running";

  async function handleScan() {
    try {
      await startScan.mutateAsync();
      toast({ title: "Library scan started" });
    } catch (e) {
      toast({
        title: "Scan failed",
        description: e instanceof Error ? e.message : "Unknown error",
        variant: "destructive",
      });
    }
  }

  function openTagEditor(trackId: number) {
    setEditingTrackId(trackId);
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h2 className="text-2xl font-semibold">Library</h2>
          <p className="text-sm text-muted-foreground">
            Local files indexed from the server library path.
            {scan?.run && (
              <>
                {" "}
                Scan: {scan.run.status} ({scan.run.files_indexed}/
                {scan.run.files_seen} files)
              </>
            )}
          </p>
        </div>
        <Button
          type="button"
          disabled={scanRunning || startScan.isPending}
          onClick={() => void handleScan()}
        >
          {scanRunning ? "Scanning…" : "Rescan library"}
        </Button>
      </div>

      <div className="max-w-sm">
        <Label htmlFor="library-search">Search</Label>
        <Input
          id="library-search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Title or artist"
        />
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <section className="rounded-lg border border-border">
          <div className="border-b border-border px-4 py-2 text-sm font-medium">
            Albums ({albums?.total ?? 0})
          </div>
          {isLoading ? (
            <p className="p-4 text-sm text-muted-foreground">Loading…</p>
          ) : (
            <ul className="divide-y divide-border">
              {(albums?.items ?? []).map((a) => (
                <li key={a.id}>
                  <button
                    type="button"
                    className="flex w-full gap-3 px-4 py-3 text-left hover:bg-accent/50"
                    onClick={() => setSelectedAlbumId(a.id)}
                  >
                    <LibraryAlbumCover
                      albumId={a.id}
                      coverPath={a.cover_path}
                      className="h-12 w-12"
                    />
                    <div className="min-w-0 flex-1">
                      <span className="font-medium">{a.title}</span>
                      <span className="mt-0.5 block text-sm text-muted-foreground">
                        {a.artist_name}
                        {a.year != null ? ` · ${a.year}` : ""} · {a.track_count}{" "}
                        tracks
                      </span>
                    </div>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="rounded-lg border border-border">
          <div className="border-b border-border px-4 py-3">
            {!albumDetail ? (
              <div className="text-sm font-medium">
                {selectedAlbumId ? "Loading…" : "Select an album"}
              </div>
            ) : (
              <div className="flex gap-4">
                <LibraryAlbumCover
                  albumId={albumDetail.id}
                  coverPath={albumDetail.cover_path}
                  className="size-28 sm:size-32"
                />
                <div className="min-w-0 flex-1">
                  <h3 className="text-sm font-medium">{albumDetail.title}</h3>
                  <p className="mt-1 text-sm text-muted-foreground">
                    {albumDetail.artist_name}
                    {albumDetail.year != null ? ` · ${albumDetail.year}` : ""}
                  </p>
                </div>
              </div>
            )}
          </div>
          {!selectedAlbumId ? (
            <p className="p-4 text-sm text-muted-foreground">
              Choose an album to view tracks and edit tags.
            </p>
          ) : !albumDetail ? (
            <p className="p-4 text-sm text-muted-foreground">Loading…</p>
          ) : (
            <ul className="divide-y divide-border">
              {albumDetail.tracks.map((t) => (
                <li
                  key={t.id}
                  className="flex items-center justify-between gap-2 px-4 py-2"
                >
                  <div className="min-w-0">
                    <p className="truncate font-medium">{t.title}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {t.disc_number != null ? `D${t.disc_number} · ` : ""}
                      {t.track_number != null ? `#${t.track_number} · ` : ""}
                      {t.year != null ? `${t.year} · ` : ""}
                      {t.genre ? `${t.genre} · ` : ""}
                      {t.path}
                    </p>
                  </div>
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={() => openTagEditor(t.id)}
                  >
                    Edit tags
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>

      {editingTrackId != null && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
          role="dialog"
          aria-modal="true"
        >
          <div className="w-full max-w-md space-y-4 rounded-lg border border-border bg-card p-4">
            {trackQuery.isLoading ? (
              <>
                <h3 className="font-medium">Edit track tags</h3>
                <p className="text-sm text-muted-foreground">Loading track…</p>
                <div className="flex justify-end gap-2">
                  <Button
                    type="button"
                    variant="secondary"
                    onClick={() => setEditingTrackId(null)}
                  >
                    Cancel
                  </Button>
                </div>
              </>
            ) : trackQuery.data ? (
              <TrackTagsEditorForm
                key={trackQuery.data.id}
                trackId={editingTrackId}
                track={trackQuery.data}
                onClose={() => setEditingTrackId(null)}
                patchTags={patchTags}
                toast={toast}
              />
            ) : (
              <>
                <h3 className="font-medium">Edit track tags</h3>
                <p className="text-sm text-destructive">Could not load track.</p>
                <div className="flex justify-end gap-2">
                  <Button
                    type="button"
                    variant="secondary"
                    onClick={() => setEditingTrackId(null)}
                  >
                    Cancel
                  </Button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
