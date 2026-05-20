import type { MetadataCandidate } from "@/api/client";
import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";
import { usePreferences } from "@/hooks/use-preferences";

type MetadataCandidatePickerProps = {
  open: boolean;
  candidates: MetadataCandidate[];
  page?: number;
  hasMore?: boolean;
  loadingPage?: boolean;
  applying?: boolean;
  onClose: () => void;
  onPageChange?: (page: number) => void;
  onApply: (candidate: MetadataCandidate) => void;
};

export function MetadataCandidatePicker({
  open,
  candidates,
  page = 1,
  hasMore = false,
  loadingPage,
  applying,
  onClose,
  onPageChange,
  onApply,
}: MetadataCandidatePickerProps) {
  const { t } = usePreferences();
  return (
    <Modal open={open} onClose={onClose}>
      <h3 className="font-medium">{t("metadata.chooseRelease")}</h3>
      <p className="text-sm text-muted-foreground">
        {t("metadata.chooseReleaseDesc")}
      </p>
      {candidates.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("metadata.noCandidates")}</p>
      ) : (
        <ul className="max-h-64 space-y-2 overflow-y-auto">
          {candidates.map((c) => (
            <li
              key={c.id}
              className="flex items-start justify-between gap-2 rounded-md border border-border p-2"
            >
              <div className="min-w-0">
                <p className="truncate font-medium">{c.title}</p>
                <p className="truncate text-sm text-muted-foreground">
                  {c.artist_name}
                  {c.year != null ? ` · ${c.year}` : ""}
                  {" · "}
                  {c.source_label}
                  {c.track_count != null
                    ? ` · ${t("metadata.tracks", { count: c.track_count })}`
                    : ""}
                </p>
              </div>
              <Button
                type="button"
                size="sm"
                disabled={applying}
                onClick={() => onApply(c)}
              >
                {t("common.apply")}
              </Button>
            </li>
          ))}
        </ul>
      )}
      {onPageChange && (page > 1 || hasMore) ? (
        <div className="flex items-center justify-between gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={page <= 1 || loadingPage}
            onClick={() => onPageChange(page - 1)}
          >
            {t("metadata.prev")}
          </Button>
          <span className="text-sm text-muted-foreground">Page {page}</span>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={!hasMore || loadingPage}
            onClick={() => onPageChange(page + 1)}
          >
            {t("metadata.next")}
          </Button>
        </div>
      ) : null}
      <div className="flex justify-end">
        <Button type="button" variant="secondary" onClick={onClose}>
          {t("common.cancel")}
        </Button>
      </div>
    </Modal>
  );
}
