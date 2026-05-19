import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  useLibraryAlbum,
  useLibraryAlbumsKeyset,
  useLibraryTrack,
  usePatchTrackTags,
  useScanLatest,
  useStartLibraryScan,
  useUploadLibraryAlbumCover,
} from "@/api/hooks";
import type {
  LibraryAlbumItem,
  LibraryTrackDetailResponse,
  LibraryTrackTagsPatchRequest,
} from "@/api/client";
import { MAX_ALBUM_COVER_BYTES } from "@/api/client";
import { LibraryAlbumCover } from "@/features/library/LibraryAlbumCover";
import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { flattenKeysetPages } from "@/api/hooks/keyset";
import { TagAutofillBar } from "@/features/library/TagAutofillBar";
import { useToast } from "@/hooks/use-toast";
import { useQueryClient } from "@tanstack/react-query";
import { queryKeys } from "@/api/hooks";
import { cn } from "@/lib/utils";
import { LibraryScanProgress } from "@/features/library/LibraryScanProgress";

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
  onSaveReady,
  patchTags,
  toast,
}: {
  trackId: number;
  track: LibraryTrackDetailResponse;
  onClose: () => void;
  onSaveReady: (save: (() => void) | null) => void;
  patchTags: ReturnType<typeof usePatchTrackTags>;
  toast: ReturnType<typeof useToast>["toast"];
}) {
  const [tagForm, setTagForm] = useState<LibraryTrackTagsPatchRequest>(() =>
    trackToTagForm(track),
  );

  const handleSave = useCallback(async () => {
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
  }, [trackId, tagForm, patchTags, onClose, toast]);

  useEffect(() => {
    onSaveReady(() => void handleSave());
    return () => onSaveReady(null);
  }, [handleSave, onSaveReady]);

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
        <Button type="button" disabled={patchTags.isPending} onClick={() => void handleSave()}>
          Save
        </Button>
      </div>
    </>
  );
}

const COVER_ACCEPT =
  "image/jpeg,image/png,image/webp,image/bmp";

export function LibraryPage() {
  const { toast } = useToast();
  const qc = useQueryClient();
  const [searchInput, setSearchInput] = useState("");
  const [q, setQ] = useState("");
  const [selectedAlbumId, setSelectedAlbumId] = useState<number | null>(null);
  const [editingTrackId, setEditingTrackId] = useState<number | null>(null);
  const tagEditorSaveRef = useRef<(() => void) | null>(null);
  const coverInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const t = window.setTimeout(() => setQ(searchInput.trim()), 300);
    return () => window.clearTimeout(t);
  }, [searchInput]);

  const listParams = useMemo(
    () => ({ limit: 50, sort: "title" as const, order: "asc" as const, q: q || undefined }),
    [q],
  );

  const { data: scan } = useScanLatest();
  const startScan = useStartLibraryScan();
  const albumsQuery = useLibraryAlbumsKeyset(listParams);
  const albumItems = flattenKeysetPages<LibraryAlbumItem>(albumsQuery.data);
  const isLoading = albumsQuery.isLoading;
  const { data: albumDetail } = useLibraryAlbum(selectedAlbumId);
  const trackQuery = useLibraryTrack(editingTrackId);
  const patchTags = usePatchTrackTags();
  const uploadCover = useUploadLibraryAlbumCover();

  const scanRunning = scan?.run?.status === "running";

  async function handleCoverFileSelected(
    e: React.ChangeEvent<HTMLInputElement>,
  ) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file || selectedAlbumId == null) return;
    if (file.size > MAX_ALBUM_COVER_BYTES) {
      toast({
        title: "File too large",
        description: "Cover image must be 20 MiB or smaller.",
        variant: "destructive",
      });
      return;
    }
    try {
      const result = await uploadCover.mutateAsync({
        albumId: selectedAlbumId,
        file,
      });
      toast({
        title: "Cover updated",
        description:
          result.tracks_embedded > 0
            ? `Embedded in ${result.tracks_embedded} track(s).`
            : undefined,
      });
    } catch (err) {
      const message =
        err instanceof Error ? err.message : "Unknown error";
      toast({
        title: "Cover upload failed",
        description: message,
        variant: "destructive",
      });
    }
  }

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

  const bindTagEditorSave = useCallback((save: (() => void) | null) => {
    tagEditorSaveRef.current = save;
  }, []);

  function closeTagEditor() {
    tagEditorSaveRef.current = null;
    setEditingTrackId(null);
  }

  function openTagEditor(trackId: number) {
    tagEditorSaveRef.current = null;
    setEditingTrackId(trackId);
  }

  const tagEditorCanConfirm =
    editingTrackId != null && !!trackQuery.data && !trackQuery.isLoading;

  const handleAutofillApplied = useCallback(() => {
    if (editingTrackId != null) {
      void qc.invalidateQueries({ queryKey: queryKeys.libraryTrack(editingTrackId) });
    }
    if (selectedAlbumId != null) {
      void qc.invalidateQueries({ queryKey: queryKeys.libraryAlbum(selectedAlbumId) });
    }
  }, [qc, editingTrackId, selectedAlbumId]);

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h2 className="text-2xl font-semibold">Library</h2>
          <p className="text-sm text-muted-foreground">
            Local files indexed from the server library path.
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

      {scanRunning && scan?.run ? (
        <LibraryScanProgress
          status={scan.run.status}
          filesSeen={scan.run.files_seen}
          filesProcessed={scan.run.files_processed}
          filesIndexed={scan.run.files_indexed}
          filesTotal={scan.run.files_total}
          startedAt={scan.run.started_at}
        />
      ) : null}

      <div className="max-w-sm">
        <Label htmlFor="library-search">Search</Label>
        <Input
          id="library-search"
          value={searchInput}
          onChange={(e) => setSearchInput(e.target.value)}
          placeholder="Title or artist"
        />
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <section className="rounded-lg border border-border">
          <div className="border-b border-border px-4 py-2 text-sm font-medium">
            Albums ({albumItems.length}
            {albumsQuery.hasNextPage ? "+" : ""})
          </div>
          {isLoading ? (
            <p className="p-4 text-sm text-muted-foreground">Loading…</p>
          ) : (
            <ul className="divide-y divide-border">
              {albumItems.map((a) => (
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
          {albumsQuery.hasNextPage ? (
            <div className="p-4">
              <Button
                variant="outline"
                size="sm"
                disabled={albumsQuery.isFetchingNextPage}
                onClick={() => void albumsQuery.fetchNextPage()}
              >
                {albumsQuery.isFetchingNextPage ? "Loading…" : "Load more"}
              </Button>
            </div>
          ) : null}
        </section>

        <section className="rounded-lg border border-border">
          <div className="border-b border-border px-4 py-3">
            {!albumDetail ? (
              <div className="text-sm font-medium">
                {selectedAlbumId ? "Loading…" : "Select an album"}
              </div>
            ) : (
              <div className="flex items-start gap-4">
                <label
                  title="Replace cover"
                  className={cn(
                    "relative block shrink-0 cursor-pointer rounded-md transition hover:opacity-90",
                    "focus-within:ring-2 focus-within:ring-ring focus-within:ring-offset-2 focus-within:ring-offset-background",
                    uploadCover.isPending && "pointer-events-none opacity-60",
                  )}
                >
                  <LibraryAlbumCover
                    albumId={albumDetail.id}
                    coverPath={albumDetail.cover_path}
                    className="size-28 sm:size-32"
                  />
                  <input
                    ref={coverInputRef}
                    type="file"
                    accept={COVER_ACCEPT}
                    className="sr-only"
                    data-testid="album-cover-file-input"
                    disabled={uploadCover.isPending}
                    onChange={(ev) => void handleCoverFileSelected(ev)}
                  />
                  {uploadCover.isPending ? (
                    <span className="absolute inset-0 flex items-center justify-center rounded-md bg-background/70 text-sm text-muted-foreground">
                      …
                    </span>
                  ) : null}
                </label>
                <div className="flex min-w-0 flex-1 items-start justify-between gap-3">
                  <div className="min-w-0">
                    <h3 className="text-sm font-medium">{albumDetail.title}</h3>
                    <p className="mt-1 text-sm text-muted-foreground">
                      {albumDetail.artist_name}
                      {albumDetail.year != null ? ` · ${albumDetail.year}` : ""}
                    </p>
                  </div>
                  <TagAutofillBar
                    albumId={albumDetail.id}
                    onApplied={handleAutofillApplied}
                  />
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

      <Modal
        open={editingTrackId != null}
        onClose={closeTagEditor}
        onConfirm={
          tagEditorCanConfirm ? () => tagEditorSaveRef.current?.() : undefined
        }
        confirmDisabled={!tagEditorCanConfirm || patchTags.isPending}
      >
        {editingTrackId == null ? null : trackQuery.isLoading ? (
          <>
            <h3 className="font-medium">Edit track tags</h3>
            <p className="text-sm text-muted-foreground">Loading track…</p>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="secondary" onClick={closeTagEditor}>
                Cancel
              </Button>
            </div>
          </>
        ) : trackQuery.data ? (
          <TrackTagsEditorForm
            key={`${trackQuery.data.id}-${trackQuery.dataUpdatedAt}`}
            trackId={editingTrackId}
            track={trackQuery.data}
            onClose={closeTagEditor}
            onSaveReady={bindTagEditorSave}
            patchTags={patchTags}
            toast={toast}
          />
        ) : (
          <>
            <h3 className="font-medium">Edit track tags</h3>
            <p className="text-sm text-destructive">Could not load track.</p>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="secondary" onClick={closeTagEditor}>
                Cancel
              </Button>
            </div>
          </>
        )}
      </Modal>
    </div>
  );
}
