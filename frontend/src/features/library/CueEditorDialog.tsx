import { useMemo, useState } from "react";
import type { CueAlbumResponse, CueDocument } from "@/api/client";
import {
  useAlbumCue,
  useSplitAlbumCue,
  useValidateAlbumCue,
} from "@/api/hooks";
import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";

type Props = {
  albumId: number | null;
  open: boolean;
  onClose: () => void;
};

type CueEditorFormProps = {
  albumId: number;
  data: CueAlbumResponse;
  onClose: () => void;
};

export function CueEditorDialog({ albumId, open, onClose }: Props) {
  const { t } = usePreferences();
  const cueQuery = useAlbumCue(albumId, open);

  return (
    <Modal open={open} onClose={onClose} className="max-w-6xl space-y-0 p-0">
      <div className="flex max-h-[88vh] min-h-[620px] flex-col overflow-hidden">
        {!cueQuery.data?.document ? (
          <div className="flex flex-1 items-center justify-center p-6 text-sm text-muted-foreground">
            {cueQuery.isLoading ? t("common.loading") : t("library.cueLoadFailed")}
          </div>
        ) : (
          <CueEditorForm
            key={`${albumId}-${cueQuery.data.document.cue_path}`}
            albumId={albumId!}
            data={cueQuery.data}
            onClose={onClose}
          />
        )}
      </div>
    </Modal>
  );
}

function CueEditorForm({ albumId, data, onClose }: CueEditorFormProps) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const validateCue = useValidateAlbumCue();
  const splitCue = useSplitAlbumCue();
  const [document, setDocument] = useState(data.document);
  const [validation, setValidation] = useState(data.validation);
  const [deleteSource, setDeleteSource] = useState(true);

  const issues = validation?.issues ?? [];
  const splitDisabled =
    splitCue.isPending || validateCue.isPending || validation?.valid !== true;

  function update<K extends keyof CueDocument>(key: K, value: CueDocument[K]) {
    setDocument((old) => ({ ...old, [key]: value }));
    setValidation((old) => (old ? { ...old, valid: false } : old));
  }

  function updateAudioPath(value: string) {
    setDocument((old) => ({
      ...old,
      audio_path: value,
      audio_format: inferCueAudioFormat(value),
    }));
    setValidation((old) => (old ? { ...old, valid: false } : old));
  }

  function updateTrack(index: number, patch: Partial<CueDocument["tracks"][number]>) {
    setDocument((old) => ({
      ...old,
      tracks: old.tracks.map((track, i) =>
        i === index ? { ...track, ...patch } : track,
      ),
    }));
    setValidation((old) => (old ? { ...old, valid: false } : old));
  }

  async function runValidate() {
    try {
      const result = await validateCue.mutateAsync({ albumId, document });
      setValidation(result);
    } catch (e) {
      toast({
        title: t("library.toast.cueValidateFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  }

  async function runSplit() {
    if (validation?.valid !== true) return;
    try {
      await splitCue.mutateAsync({
        albumId,
        body: {
          document,
          source_file_policy: deleteSource ? "delete_after_success" : "keep",
          file_mask: "{$n} {$a} $t",
        },
      });
      toast({ title: t("library.toast.cueSplitStarted") });
      onClose();
    } catch (e) {
      toast({
        title: t("library.toast.cueSplitFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  }

  const firstIssueByField = useMemo(() => {
    const map = new Map<string, string>();
    for (const issue of validation?.issues ?? []) {
      if (issue.field && !map.has(issue.field)) {
        map.set(issue.field, issue.message);
      }
    }
    return map;
  }, [validation?.issues]);

  return (
    <>
      <div className="border-b border-border bg-muted/40 px-4 py-3">
        <h3 className="text-sm font-semibold">CUE</h3>
        {data.cue_files.length > 1 ? (
          <select
            className="mt-2 h-9 w-full rounded-md border border-border bg-card px-2 text-sm"
            value={document.cue_path}
            disabled
            aria-label="CUE file"
          >
            {data.cue_files.map((file) => (
              <option key={file.path} value={file.path}>
                {file.path}
              </option>
            ))}
          </select>
        ) : null}
      </div>

      <div className="grid grid-cols-[8rem_minmax(0,1fr)] border-b border-border">
        <Label className="px-3 py-2 text-right" htmlFor="cue-album-artist">
          Album artist
        </Label>
        <Input
          id="cue-album-artist"
          className="rounded-none border-0 border-l"
          value={document.album_artist}
          onChange={(ev) => update("album_artist", ev.target.value)}
        />
        <Label className="px-3 py-2 text-right" htmlFor="cue-album-title">
          Album title
        </Label>
        <Input
          id="cue-album-title"
          className="rounded-none border-0 border-l border-t"
          value={document.album_title}
          aria-invalid={firstIssueByField.has("album_title")}
          onChange={(ev) => update("album_title", ev.target.value)}
        />
        <Label className="px-3 py-2 text-right" htmlFor="cue-file">
          FILE
        </Label>
        <Input
          id="cue-file"
          className="rounded-none border-0 border-l border-t font-mono text-xs"
          value={document.audio_path}
          aria-invalid={firstIssueByField.has("audio_path")}
          onChange={(ev) => updateAudioPath(ev.target.value)}
        />
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        <table className="w-full min-w-[760px] border-collapse text-sm">
          <thead className="sticky top-0 bg-muted">
            <tr>
              <th className="border border-border p-2">A</th>
              <th className="border border-border p-2">Track</th>
              <th className="border border-border p-2">Artist</th>
              <th className="border border-border p-2">Title</th>
              <th className="border border-border p-2">Length</th>
              <th className="border border-border p-2">Pregap</th>
            </tr>
          </thead>
          <tbody>
            {document.tracks.map((track, index) => (
              <tr key={track.number}>
                <td className="border border-border p-1 text-center">
                  <Checkbox
                    checked={track.selected}
                    onCheckedChange={(v) => updateTrack(index, { selected: !!v })}
                    aria-label={`Track ${track.number}`}
                  />
                </td>
                <td className="border border-border p-1 text-center tabular-nums">
                  {String(track.number).padStart(2, "0")}
                </td>
                <td className="border border-border p-1">
                  <Input
                    value={track.artist ?? ""}
                    onChange={(ev) => updateTrack(index, { artist: ev.target.value })}
                  />
                </td>
                <td className="border border-border p-1">
                  <Input
                    value={track.title}
                    aria-invalid={issues.some(
                      (i) =>
                        i.field === "tracks.title" && i.track_number === track.number,
                    )}
                    onChange={(ev) => updateTrack(index, { title: ev.target.value })}
                  />
                </td>
                <td className="border border-border p-1 text-center tabular-nums">
                  {track.duration ?? ""}
                </td>
                <td className="border border-border p-1 text-center tabular-nums">
                  {track.pregap ?? ""}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="border-t border-border bg-muted/30 p-3">
        <div className="grid grid-cols-[5rem_8rem_5rem_minmax(0,1fr)] items-center gap-2">
          <Label htmlFor="cue-year" className="text-right">
            Year
          </Label>
          <Input
            id="cue-year"
            type="number"
            value={document.year ?? ""}
            aria-invalid={firstIssueByField.has("year")}
            onChange={(ev) =>
              update(
                "year",
                ev.target.value ? Number.parseInt(ev.target.value, 10) : null,
              )
            }
          />
          <Label htmlFor="cue-genre" className="text-right">
            Genre
          </Label>
          <Input
            id="cue-genre"
            value={document.genre ?? ""}
            aria-invalid={firstIssueByField.has("genre")}
            onChange={(ev) => update("genre", ev.target.value)}
          />
        </div>
        {document.comment != null ? (
          <div className="mt-2 grid grid-cols-[5rem_minmax(0,1fr)] items-center gap-2">
            <Label htmlFor="cue-comment" className="text-right">
              Comment
            </Label>
            <Input
              id="cue-comment"
              value={document.comment}
              onChange={(ev) => update("comment", ev.target.value)}
            />
          </div>
        ) : null}
        <div className="mt-2 flex items-center gap-2">
          <Checkbox
            checked={deleteSource}
            onCheckedChange={(v) => setDeleteSource(!!v)}
            aria-label="Delete source after successful split"
          />
          <span className="text-sm">Delete source after successful split</span>
        </div>
        {issues.length > 0 ? (
          <div className="mt-2 space-y-1 text-sm text-destructive">
            {issues.map((issue, index) => (
              <p key={`${issue.code}-${index}`}>{issue.message}</p>
            ))}
          </div>
        ) : null}
        <div className="mt-3 flex justify-end gap-2">
          <Button variant="outline" onClick={onClose} disabled={splitCue.isPending}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="secondary"
            onClick={() => void runValidate()}
            disabled={validateCue.isPending}
          >
            Check
          </Button>
          <Button onClick={() => void runSplit()} disabled={splitDisabled}>
            Split
          </Button>
        </div>
      </div>
    </>
  );
}

function inferCueAudioFormat(path: string): CueDocument["audio_format"] {
  const ext = path.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "flac":
      return "flac";
    case "wav":
    case "wave":
      return "wav";
    case "ape":
      return "ape";
    case "m4a":
    case "mp4":
      return "m4a";
    case "wv":
    case "wavpack":
      return "wv";
    default:
      return "unknown";
  }
}
