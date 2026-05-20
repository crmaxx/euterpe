import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  useLibraryAlbum,
  useLibraryAlbumsKeyset,
  useLibraryTrack,
  usePatchTrackTags,
  useCancelLibraryScan,
  useScanLatest,
  useStartLibraryScan,
  usePrefetchLibraryAlbumCovers,
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
import { Folder, Pencil, ScanSearch } from "lucide-react";
import { cn } from "@/lib/utils";
import { LibraryScanProgress } from "@/features/library/LibraryScanProgress";
import { usePreferences } from "@/hooks/use-preferences";

function albumFolderFromTracks(
  tracks: { path: string }[],
): string | undefined {
  const p = tracks[0]?.path;
  if (!p) return undefined;
  const i = p.lastIndexOf("/");
  if (i <= 0) return undefined;
  return p.slice(0, i);
}

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
  const { t } = usePreferences();
  const [tagForm, setTagForm] = useState<LibraryTrackTagsPatchRequest>(() =>
    trackToTagForm(track),
  );

  const handleSave = useCallback(async () => {
    try {
      await patchTags.mutateAsync({ id: trackId, body: tagForm });
      toast({ title: t("library.toast.tagsSaved") });
      onClose();
    } catch (e) {
      toast({
        title: t("library.toast.saveFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  }, [trackId, tagForm, patchTags, onClose, toast, t]);

  useEffect(() => {
    onSaveReady(() => void handleSave());
    return () => onSaveReady(null);
  }, [handleSave, onSaveReady]);

  return (
    <>
      <h3 className="font-medium">{t("library.editTrackTags")}</h3>
      <div className="space-y-2">
        <Label htmlFor="tag-title">{t("library.tagsForm.title")}</Label>
        <Input
          id="tag-title"
          value={tagForm.title ?? ""}
          onChange={(e) => setTagForm((f) => ({ ...f, title: e.target.value }))}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="tag-artist">{t("library.tagsForm.artist")}</Label>
        <Input
          id="tag-artist"
          value={tagForm.artist_name ?? ""}
          onChange={(e) =>
            setTagForm((f) => ({ ...f, artist_name: e.target.value }))
          }
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="tag-album">{t("library.tagsForm.album")}</Label>
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
          <Label htmlFor="tag-track">{t("library.tagsForm.track")}</Label>
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
          <Label htmlFor="tag-disc">{t("library.tagsForm.disc")}</Label>
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
          <Label htmlFor="tag-year">{t("library.tagsForm.year")}</Label>
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
          <Label htmlFor="tag-genre">{t("library.tagsForm.genre")}</Label>
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
        {t("library.tagsForm.genreHint")}
      </p>
      <div className="flex justify-end gap-2">
        <Button type="button" variant="secondary" onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <Button type="button" disabled={patchTags.isPending} onClick={() => void handleSave()}>
          {t("common.save")}
        </Button>
      </div>
    </>
  );
}

const COVER_ACCEPT =
  "image/jpeg,image/png,image/webp,image/bmp";

export function LibraryPage() {
  const { t } = usePreferences();
  const { toast } = useToast();
  const qc = useQueryClient();
  const [searchInput, setSearchInput] = useState("");
  const [q, setQ] = useState("");
  const [selectedAlbumId, setSelectedAlbumId] = useState<number | null>(null);
  const [editingTrackId, setEditingTrackId] = useState<number | null>(null);
  const tagEditorSaveRef = useRef<(() => void) | null>(null);
  const coverInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const timerId = window.setTimeout(() => setQ(searchInput.trim()), 300);
    return () => window.clearTimeout(timerId);
  }, [searchInput]);

  const listParams = useMemo(
    () => ({ limit: 50, sort: "title" as const, order: "asc" as const, q: q || undefined }),
    [q],
  );

  const { data: scan } = useScanLatest();
  const startScan = useStartLibraryScan();
  const cancelScan = useCancelLibraryScan();
  const albumsQuery = useLibraryAlbumsKeyset(listParams);
  const albumItems = flattenKeysetPages<LibraryAlbumItem>(albumsQuery.data);
  usePrefetchLibraryAlbumCovers(albumItems);
  const isLoading = albumsQuery.isLoading;
  const { data: albumDetail } = useLibraryAlbum(selectedAlbumId);
  const trackQuery = useLibraryTrack(editingTrackId);
  const patchTags = usePatchTrackTags();
  const uploadCover = useUploadLibraryAlbumCover();

  const scanRunning = scan?.run?.status === "running";
  const showScanProgress =
    scan?.run != null &&
    (scan.run.status === "running" || scan.run.status === "cancelled");
  const repairFolder = albumDetail
    ? albumFolderFromTracks(albumDetail.tracks)
    : undefined;

  async function handleCoverFileSelected(
    e: React.ChangeEvent<HTMLInputElement>,
  ) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file || selectedAlbumId == null) return;
    if (file.size > MAX_ALBUM_COVER_BYTES) {
      toast({
        title: t("library.toast.fileTooLarge"),
        description: t("library.toast.fileTooLargeDesc"),
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
        title: t("library.toast.coverUpdated"),
        description:
          result.tracks_embedded > 0
            ? t("library.toast.coverEmbedded", { count: result.tracks_embedded })
            : undefined,
      });
    } catch (err) {
      const message =
        err instanceof Error ? err.message : t("common.unknownError");
      toast({
        title: t("library.toast.coverFailed"),
        description: message,
        variant: "destructive",
      });
    }
  }

  async function handleScan(root?: string) {
    try {
      await startScan.mutateAsync(root);
      toast({
        title: root ? t("library.toast.repairStarted") : t("library.toast.rebuildStarted"),
      });
    } catch (e) {
      toast({
        title: t("library.toast.scanFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  }

  async function handleCancelScan() {
    const id = scan?.run?.id;
    if (id == null) return;
    try {
      await cancelScan.mutateAsync(id);
      toast({ title: t("library.toast.scanCancelled") });
    } catch (e) {
      toast({
        title: t("library.toast.cancelFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
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
          <div className="flex items-center gap-2">
            <Folder
              className="size-5 shrink-0 text-muted-foreground"
              aria-hidden
            />
            <h2 className="text-2xl font-semibold">{t("library.title")}</h2>
          </div>
          <p className="text-sm text-muted-foreground">{t("library.subtitle")}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          {scanRunning ? (
            <Button
              type="button"
              variant="destructive"
              disabled={cancelScan.isPending}
              onClick={() => void handleCancelScan()}
            >
              {cancelScan.isPending ? t("library.cancelling") : t("library.cancelScan")}
            </Button>
          ) : null}
          <Button
            type="button"
            variant="outline"
            disabled={scanRunning || startScan.isPending}
            onClick={() => void handleScan()}
          >
            <ScanSearch className="size-4" aria-hidden />
            {scanRunning ? t("library.scanning") : t("library.rebuild")}
          </Button>
        </div>
      </div>

      {showScanProgress && scan.run ? (
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
        <Label htmlFor="library-search">{t("library.search")}</Label>
        <Input
          id="library-search"
          value={searchInput}
          onChange={(e) => setSearchInput(e.target.value)}
          placeholder={t("library.searchPlaceholder")}
        />
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <section className="rounded-lg border border-border">
          <div className="border-b border-border px-4 py-2 text-sm font-medium">
            {t("library.albums")} ({albumItems.length}
            {albumsQuery.hasNextPage ? "+" : ""})
          </div>
          {isLoading ? (
            <p className="p-4 text-sm text-muted-foreground">{t("common.loading")}</p>
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
                        {t("library.tracksCount")}
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
                {albumsQuery.isFetchingNextPage
                  ? t("common.loading")
                  : t("common.loadMore")}
              </Button>
            </div>
          ) : null}
        </section>

        <section className="rounded-lg border border-border">
          <div className="border-b border-border px-4 py-3">
            {!albumDetail ? (
              <div className="text-sm font-medium">
                {selectedAlbumId ? t("common.loading") : t("library.selectAlbum")}
              </div>
            ) : (
              <div className="flex items-start gap-4">
                <label
                  title={t("library.replaceCover")}
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
                  <div className="flex shrink-0 flex-col gap-2 sm:flex-row sm:items-start">
                    {repairFolder ? (
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={scanRunning || startScan.isPending}
                        onClick={() => void handleScan(repairFolder)}
                      >
                        {t("library.repairFolder")}
                      </Button>
                    ) : null}
                    <TagAutofillBar
                      albumId={albumDetail.id}
                      onApplied={handleAutofillApplied}
                    />
                  </div>
                </div>
              </div>
            )}
          </div>
          {!selectedAlbumId ? (
            <p className="p-4 text-sm text-muted-foreground">
              {t("library.chooseAlbum")}
            </p>
          ) : !albumDetail ? (
            <p className="p-4 text-sm text-muted-foreground">{t("common.loading")}</p>
          ) : (
            <ul className="divide-y divide-border">
              {albumDetail.tracks.map((track) => (
                <li
                  key={track.id}
                  className="flex items-center justify-between gap-2 px-4 py-2"
                >
                  <div className="min-w-0">
                    <p className="truncate font-medium">{track.title}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {track.disc_number != null ? `D${track.disc_number} · ` : ""}
                      {track.track_number != null ? `#${track.track_number} · ` : ""}
                      {track.year != null ? `${track.year} · ` : ""}
                      {track.genre ? `${track.genre} · ` : ""}
                      {track.path}
                    </p>
                  </div>
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    className="size-8 shrink-0 p-0"
                    aria-label={t("library.editTags")}
                    onClick={() => openTagEditor(track.id)}
                  >
                    <Pencil className="size-4" aria-hidden />
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
            <h3 className="font-medium">{t("library.editTrackTags")}</h3>
            <p className="text-sm text-muted-foreground">{t("library.loadingTrack")}</p>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="secondary" onClick={closeTagEditor}>
                {t("common.cancel")}
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
            <h3 className="font-medium">{t("library.editTrackTags")}</h3>
            <p className="text-sm text-destructive">{t("library.trackLoadFailed")}</p>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="secondary" onClick={closeTagEditor}>
                {t("common.cancel")}
              </Button>
            </div>
          </>
        )}
      </Modal>
    </div>
  );
}
