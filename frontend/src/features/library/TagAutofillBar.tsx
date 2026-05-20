import { useMemo, useState } from "react";
import type { MetadataCandidate } from "@/api/client";
import {
  useAlbumMetadataApply,
  useAlbumMetadataLookup,
  useIntegrations,
} from "@/api/hooks";
import { MetadataCandidatePicker } from "@/components/metadata-candidate-picker";
import { SplitButton, type SplitButtonOption } from "@/components/split-button";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";

const DEFAULT_INTEGRATION_KEY = "euterpe.defaultTagIntegrationId";

type TagAutofillBarProps = {
  albumId: number;
  onApplied: () => void;
};

export function TagAutofillBar({ albumId, onApplied }: TagAutofillBarProps) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const { data: integrations } = useIntegrations();
  const lookup = useAlbumMetadataLookup();
  const apply = useAlbumMetadataApply();

  const [pickerOpen, setPickerOpen] = useState(false);
  const [candidates, setCandidates] = useState<MetadataCandidate[]>([]);
  const [lookupPage, setLookupPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [activeIntegrationId, setActiveIntegrationId] = useState<number | null>(
    null,
  );

  const enabled = useMemo(
    () => (integrations?.items ?? []).filter((i) => i.enabled),
    [integrations?.items],
  );

  const options: SplitButtonOption[] = enabled.map((i) => ({
    id: i.id,
    label: i.display_name,
  }));

  const defaultId = useMemo(() => {
    const stored = localStorage.getItem(DEFAULT_INTEGRATION_KEY);
    const parsed = stored ? Number.parseInt(stored, 10) : Number.NaN;
    if (enabled.some((i) => i.id === parsed)) {
      return parsed;
    }
    return enabled[0]?.id ?? null;
  }, [enabled]);

  const defaultItem = enabled.find((i) => i.id === defaultId);

  if (enabled.length === 0) {
    return null;
  }

  async function runLookup(integrationId: number, page = 1) {
    setActiveIntegrationId(integrationId);
    localStorage.setItem(DEFAULT_INTEGRATION_KEY, String(integrationId));
    try {
      const result = await lookup.mutateAsync({ albumId, integrationId, page });
      setCandidates(result.candidates);
      setLookupPage(result.page);
      setHasMore(result.has_more);
      setPickerOpen(true);
    } catch (e) {
      toast({
        title: t("library.lookupFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  }

  async function handleApply(candidate: MetadataCandidate) {
    if (activeIntegrationId == null) {
      return;
    }
    try {
      const result = await apply.mutateAsync({
        albumId,
        integrationId: activeIntegrationId,
        candidateId: candidate.id,
      });
      toast({
        title: t("library.metadataApplied"),
        description: result.cover_applied
          ? t("library.metadataAppliedDescCover", {
              count: result.tracks_updated,
            })
          : t("library.metadataAppliedDesc", { count: result.tracks_updated }),
      });
      if (result.warnings.length > 0) {
        toast({
          title: t("library.warnings"),
          description: result.warnings.slice(0, 3).join("; "),
          variant: "destructive",
        });
      }
      setPickerOpen(false);
      onApplied();
    } catch (e) {
      toast({
        title: t("library.applyFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  }

  return (
    <>
      <SplitButton
        label={
          defaultItem
            ? t("library.autofillWith", { name: defaultItem.display_name })
            : t("library.autofill")
        }
        options={options}
        loading={lookup.isPending}
        onPrimaryClick={() => {
          if (defaultId != null) {
            void runLookup(defaultId);
          }
        }}
        onSelect={(id) => void runLookup(id)}
      />
      <MetadataCandidatePicker
        open={pickerOpen}
        candidates={candidates}
        page={lookupPage}
        hasMore={hasMore}
        loadingPage={lookup.isPending}
        applying={apply.isPending}
        onClose={() => setPickerOpen(false)}
        onPageChange={(page) => {
          if (activeIntegrationId != null) {
            void runLookup(activeIntegrationId, page);
          }
        }}
        onApply={(c) => void handleApply(c)}
      />
    </>
  );
}
