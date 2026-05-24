import { useMemo, useState } from "react";
import { ChevronRight, File, Folder, Loader2 } from "lucide-react";
import type {
  TorrentInspectResponse,
  TorrentPostDownloadOptions,
} from "@/api/client";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { usePreferences } from "@/hooks/use-preferences";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  buildTorrentFileTree,
  collectFileIndices,
  filterTorrentFiles,
  flattenTorrentTree,
  folderSelectionState,
  type TorrentTreeNode,
} from "./torrentFileTree";

type Props = {
  inspect: TorrentInspectResponse;
  selection: Record<number, boolean>;
  copyToLibrary: boolean;
  autoIndex: boolean;
  postDownload: TorrentPostDownloadOptions | null;
  busy: boolean;
  onSelectionChange: (next: Record<number, boolean>) => void;
  onCopyToLibraryChange: (v: boolean) => void;
  onAutoIndexChange: (v: boolean) => void;
  onPostDownloadChange: (next: TorrentPostDownloadOptions | null) => void;
  onCancel: () => void;
  onConfirm: () => void;
};

function formatSizeLine(
  totalBytes: number,
  freeBytes: number | null | undefined,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  const total = formatBytes(totalBytes);
  if (freeBytes != null && freeBytes > 0) {
    return t("sources.torrent.sizeWithFree", {
      total,
      free: formatBytes(freeBytes),
    });
  }
  return total;
}

function TorrentTreeRow({
  node,
  selection,
  onToggleFile,
  onToggleFolder,
}: {
  node: TorrentTreeNode;
  selection: Record<number, boolean>;
  onToggleFile: (index: number, checked: boolean) => void;
  onToggleFolder: (node: TorrentTreeNode, checked: boolean) => void;
}) {
  const indent = node.depth * 16;

  if (node.kind === "file" && node.file) {
    const f = node.file;
    return (
      <div
        className="grid grid-cols-[minmax(0,1fr)_7rem] items-center gap-2 border-b border-border/60 px-2 py-1 hover:bg-accent/30"
        style={{ paddingLeft: 8 + indent }}
      >
        <label className="flex min-w-0 cursor-pointer items-center gap-2">
          <Checkbox
            checked={selection[f.index] ?? false}
            onCheckedChange={(v) => onToggleFile(f.index, !!v)}
            aria-label={f.path}
          />
          <File className="size-4 shrink-0 text-muted-foreground" aria-hidden />
          <span className="truncate text-sm">{node.name}</span>
        </label>
        <span className="text-right text-sm tabular-nums text-muted-foreground">
          {formatBytes(f.size_bytes)}
        </span>
      </div>
    );
  }

  const state = folderSelectionState(node, selection);
  return (
    <div
      className="grid grid-cols-[minmax(0,1fr)_7rem] items-center gap-2 border-b border-border/60 px-2 py-1 hover:bg-accent/30"
      style={{ paddingLeft: 8 + indent }}
    >
      <label className="flex min-w-0 cursor-pointer items-center gap-2">
        <Checkbox
          checked={
            state === "all" ? true : state === "some" ? "indeterminate" : false
          }
          onCheckedChange={(v) => onToggleFolder(node, v === true)}
          aria-label={node.name}
        />
        <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
        <Folder className="size-4 shrink-0 text-amber-600/80 dark:text-amber-500/80" aria-hidden />
        <span className="truncate text-sm font-medium">{node.name}</span>
      </label>
      <span className="text-right text-sm text-muted-foreground">—</span>
    </div>
  );
}

export function TorrentInspectView({
  inspect,
  selection,
  copyToLibrary,
  autoIndex,
  postDownload,
  busy,
  onSelectionChange,
  onCopyToLibraryChange,
  onAutoIndexChange,
  onPostDownloadChange,
  onCancel,
  onConfirm,
}: Props) {
  const { t } = usePreferences();
  const [filter, setFilter] = useState("");

  const visibleFiles = useMemo(
    () => filterTorrentFiles(inspect.files, filter),
    [inspect.files, filter],
  );

  const tree = useMemo(() => buildTorrentFileTree(visibleFiles), [visibleFiles]);
  const rows = useMemo(() => flattenTorrentTree(tree), [tree]);

  const selectedCount = useMemo(
    () => inspect.files.filter((f) => selection[f.index]).length,
    [inspect.files, selection],
  );

  const selectedCueCandidate = useMemo(() => {
    const candidates = inspect.post_download_capability?.cue_candidates ?? [];
    return candidates.find((candidate) => {
      const cue = inspect.files.find((f) => f.path === candidate.cue_path);
      const audio = inspect.files.find((f) => f.path === candidate.audio_path);
      return !!cue && !!audio && !!selection[cue.index] && !!selection[audio.index];
    });
  }, [inspect.files, inspect.post_download_capability?.cue_candidates, selection]);

  const postCueEnabled = !!selectedCueCandidate && !!postDownload;

  const setPostOption = (patch: Partial<TorrentPostDownloadOptions>) => {
    if (!postDownload) return;
    onPostDownloadChange({
      ...postDownload,
      cue_path: selectedCueCandidate?.cue_path ?? postDownload.cue_path ?? null,
      ...patch,
    });
  };

  const setAll = (checked: boolean) => {
    const next = { ...selection };
    for (const f of inspect.files) {
      next[f.index] = checked;
    }
    onSelectionChange(next);
  };

  const toggleFile = (index: number, checked: boolean) => {
    onSelectionChange({ ...selection, [index]: checked });
  };

  const toggleFolder = (node: TorrentTreeNode, checked: boolean) => {
    const next = { ...selection };
    for (const idx of collectFileIndices(node)) {
      next[idx] = checked;
    }
    onSelectionChange(next);
  };

  return (
    <div className="flex max-h-[min(88vh,800px)] flex-col">
      <div className="border-b border-border px-4 py-3">
        <h3 className="text-sm font-semibold leading-snug text-foreground break-words">
          {inspect.name}
        </h3>
      </div>

      <div className="grid min-h-0 flex-1 grid-cols-1 md:grid-cols-[minmax(220px,280px)_1fr]">
        <aside className="flex flex-col gap-4 border-b border-border p-4 md:border-b-0 md:border-r">
          <div className="space-y-2">
            <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              {t("sources.torrent.importOptions")}
            </p>
            <label className="flex cursor-pointer items-center gap-2">
              <Checkbox
                checked={copyToLibrary}
                onCheckedChange={(v) => onCopyToLibraryChange(!!v)}
              />
              <span className="text-sm">{t("sources.torrent.copyToLibrary")}</span>
            </label>
            <label className="flex cursor-pointer items-center gap-2">
              <Checkbox
                checked={autoIndex}
                disabled={!copyToLibrary}
                onCheckedChange={(v) => onAutoIndexChange(!!v)}
              />
              <span
                className={cn(
                  "text-sm",
                  copyToLibrary ? "text-foreground" : "text-muted-foreground",
                )}
              >
                {t("sources.torrent.autoIndex")}
              </span>
            </label>
          </div>

          {inspect.post_download_capability && postDownload ? (
            <div className="space-y-2">
              <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                {t("sources.torrent.postDownload")}
              </p>
              {inspect.post_download_capability.has_flac_image_cue ? (
                <label className="flex cursor-pointer items-center gap-2">
                  <Checkbox
                    checked={postDownload.split_after_download}
                    disabled={!postCueEnabled}
                    onCheckedChange={(v) =>
                      setPostOption({ split_after_download: !!v })
                    }
                  />
                  <span
                    className={cn(
                      "text-sm",
                      postCueEnabled ? "text-foreground" : "text-muted-foreground",
                    )}
                  >
                    {t("sources.torrent.splitAfterDownload")}
                  </span>
                </label>
              ) : null}
              {inspect.post_download_capability.has_convertible_image_cue ? (
                <>
                  <label className="flex cursor-pointer items-center gap-2">
                    <Checkbox
                      checked={postDownload.convert_after_download}
                      disabled={!postCueEnabled}
                      onCheckedChange={(v) =>
                        setPostOption({
                          convert_after_download: !!v,
                          split_after_conversion: v
                            ? postDownload.split_after_conversion
                            : false,
                        })
                      }
                    />
                    <span
                      className={cn(
                        "text-sm",
                        postCueEnabled ? "text-foreground" : "text-muted-foreground",
                      )}
                    >
                      {t("sources.torrent.convertAfterDownload")}
                    </span>
                  </label>
                  <label className="flex cursor-pointer items-center gap-2">
                    <Checkbox
                      checked={postDownload.split_after_conversion}
                      disabled={
                        !postCueEnabled || !postDownload.convert_after_download
                      }
                      onCheckedChange={(v) =>
                        setPostOption({ split_after_conversion: !!v })
                      }
                    />
                    <span
                      className={cn(
                        "text-sm",
                        postCueEnabled && postDownload.convert_after_download
                          ? "text-foreground"
                          : "text-muted-foreground",
                      )}
                    >
                      {t("sources.torrent.splitAfterConversion")}
                    </span>
                  </label>
                </>
              ) : null}
              <label className="flex cursor-pointer items-center gap-2">
                <Checkbox
                  checked={postDownload.source_file_policy === "delete_after_success"}
                  disabled={
                    !postCueEnabled ||
                    (!postDownload.split_after_download &&
                      !postDownload.split_after_conversion)
                  }
                  onCheckedChange={(v) =>
                    setPostOption({
                      source_file_policy: v ? "delete_after_success" : "keep",
                    })
                  }
                />
                <span className="text-sm text-muted-foreground">
                  {t("sources.torrent.deleteCueSource")}
                </span>
              </label>
            </div>
          ) : null}

          <div className="space-y-2 text-sm">
            <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              {t("sources.torrent.torrentInfo")}
            </p>
            <dl className="space-y-1.5 text-xs">
              <div>
                <dt className="text-muted-foreground">{t("sources.torrent.size")}</dt>
                <dd className="break-words text-foreground">
                  {formatSizeLine(
                    inspect.total_size_bytes,
                    inspect.free_space_bytes,
                    t,
                  )}
                </dd>
              </div>
              <div>
                <dt className="text-muted-foreground">
                  {t("sources.torrent.infoHashV1")}
                </dt>
                <dd className="break-all font-mono text-[11px] text-foreground">
                  {inspect.info_hash_v1}
                </dd>
              </div>
              {inspect.info_hash_v2 ? (
                <div>
                  <dt className="text-muted-foreground">
                    {t("sources.torrent.infoHashV2")}
                  </dt>
                  <dd className="break-all font-mono text-[11px] text-foreground">
                    {inspect.info_hash_v2}
                  </dd>
                </div>
              ) : null}
              {inspect.comment ? (
                <div>
                  <dt className="text-muted-foreground">{t("sources.torrent.comment")}</dt>
                  <dd className="break-all">
                    {inspect.comment.startsWith("http") ? (
                      <a
                        href={inspect.comment}
                        target="_blank"
                        rel="noreferrer"
                        className="text-primary underline-offset-2 hover:underline"
                      >
                        {inspect.comment}
                      </a>
                    ) : (
                      <span className="text-foreground">{inspect.comment}</span>
                    )}
                  </dd>
                </div>
              ) : null}
            </dl>
          </div>
        </aside>

        <section className="flex min-h-0 flex-col p-4">
          <div className="mb-2 flex flex-wrap items-center gap-2">
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={busy}
              onClick={() => setAll(true)}
            >
              {t("sources.torrent.selectAll")}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={busy}
              onClick={() => setAll(false)}
            >
              {t("sources.torrent.selectNone")}
            </Button>
            <Input
              className="h-8 min-w-[12rem] flex-1 text-sm"
              placeholder={t("sources.torrent.filterPlaceholder")}
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              disabled={busy}
            />
          </div>

          <div className="grid min-h-0 flex-1 grid-rows-[auto_1fr] overflow-hidden rounded-md border border-border">
            <div className="grid grid-cols-[minmax(0,1fr)_7rem] gap-2 border-b border-border bg-muted/40 px-2 py-1.5 text-xs font-medium text-muted-foreground">
              <span>{t("sources.torrent.colName")}</span>
              <span className="text-right">{t("sources.torrent.colSize")}</span>
            </div>
            <div className="min-h-0 overflow-y-auto">
              {rows.length === 0 ? (
                <p className="p-4 text-sm text-muted-foreground">
                  {t("sources.torrent.noFilesMatch")}
                </p>
              ) : (
                rows.map((node) => (
                  <TorrentTreeRow
                    key={node.id}
                    node={node}
                    selection={selection}
                    onToggleFile={toggleFile}
                    onToggleFolder={toggleFolder}
                  />
                ))
              )}
            </div>
          </div>
        </section>
      </div>

      <div className="flex shrink-0 justify-end gap-2 border-t border-border px-4 py-3">
        <Button type="button" variant="outline" onClick={onCancel} disabled={busy}>
          {t("common.cancel")}
        </Button>
        <Button
          type="button"
          disabled={selectedCount === 0 || busy}
          onClick={onConfirm}
        >
          {busy ? <Loader2 className="size-4 animate-spin" aria-hidden /> : null}
          {t("sources.torrent.queue", { count: selectedCount })}
        </Button>
      </div>
    </div>
  );
}
