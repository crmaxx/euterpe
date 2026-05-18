import type { MetadataCandidate } from "@/api/client";
import { Modal } from "@/components/modal";
import { Button } from "@/components/ui/button";

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
  return (
    <Modal open={open} onClose={onClose}>
      <h3 className="font-medium">Choose release</h3>
      <p className="text-sm text-muted-foreground">
        Select a match from the catalog to fill tags and cover.
      </p>
      {candidates.length === 0 ? (
        <p className="text-sm text-muted-foreground">No candidates found.</p>
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
                  {c.track_count != null ? ` · ${c.track_count} tracks` : ""}
                </p>
              </div>
              <Button
                type="button"
                size="sm"
                disabled={applying}
                onClick={() => onApply(c)}
              >
                Apply
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
            Previous
          </Button>
          <span className="text-sm text-muted-foreground">Page {page}</span>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={!hasMore || loadingPage}
            onClick={() => onPageChange(page + 1)}
          >
            Next
          </Button>
        </div>
      ) : null}
      <div className="flex justify-end">
        <Button type="button" variant="secondary" onClick={onClose}>
          Cancel
        </Button>
      </div>
    </Modal>
  );
}
