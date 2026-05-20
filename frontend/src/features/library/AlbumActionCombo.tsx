import { useEffect, useMemo, useState } from "react";
import type { MetadataCandidate } from "@/api/client";
import {
  useAlbumMetadataApply,
  useAlbumMetadataLookup,
  useIntegrations,
} from "@/api/hooks";
import {
  ComboActionButton,
  type ComboActionOption,
} from "@/components/combo-action-button";
import { MetadataCandidatePicker } from "@/components/metadata-candidate-picker";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";

const ACTION_STORAGE_KEY = "euterpe.albumAction";
const DEFAULT_INTEGRATION_KEY = "euterpe.defaultTagIntegrationId";

const ACTION_EDIT_TAGS = "edit-tags";
const ACTION_REPAIR_FOLDER = "repair-folder";
const AUTOFILL_PREFIX = "autofill:";

function autofillActionId(integrationId: number) {
  return `${AUTOFILL_PREFIX}${integrationId}`;
}

function parseAutofillId(actionId: string): number | null {
  if (!actionId.startsWith(AUTOFILL_PREFIX)) {
    return null;
  }
  const n = Number.parseInt(actionId.slice(AUTOFILL_PREFIX.length), 10);
  return Number.isFinite(n) ? n : null;
}

type AlbumActionComboProps = {
  albumId: number;
  repairFolder?: string;
  scanRunning: boolean;
  scanPending: boolean;
  onEditTags: () => void;
  onRepairFolder: (folder: string) => void;
  onApplied: () => void;
};

export function AlbumActionCombo({
  albumId,
  repairFolder,
  scanRunning,
  scanPending,
  onEditTags,
  onRepairFolder,
  onApplied,
}: AlbumActionComboProps) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const { data: integrations } = useIntegrations();
  const lookup = useAlbumMetadataLookup();
  const apply = useAlbumMetadataApply();

  const [selectedId, setSelectedId] = useState(ACTION_EDIT_TAGS);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [candidates, setCandidates] = useState<MetadataCandidate[]>([]);
  const [lookupPage, setLookupPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [activeIntegrationId, setActiveIntegrationId] = useState<number | null>(
    null,
  );

  const enabledIntegrations = useMemo(
    () => (integrations?.items ?? []).filter((i) => i.enabled),
    [integrations?.items],
  );

  const options: ComboActionOption[] = useMemo(() => {
    const list: ComboActionOption[] = [
      {
        id: ACTION_EDIT_TAGS,
        label: t("library.editAlbumTags"),
      },
    ];
    if (repairFolder) {
      list.push({
        id: ACTION_REPAIR_FOLDER,
        label: t("library.repairFolder"),
        disabled: scanRunning || scanPending,
      });
    }
    for (const integration of enabledIntegrations) {
      list.push({
        id: autofillActionId(integration.id),
        label: t("library.autofillWith", { name: integration.display_name }),
      });
    }
    return list;
  }, [t, repairFolder, scanRunning, scanPending, enabledIntegrations]);

  useEffect(() => {
    setSelectedId((current) => {
      if (options.some((o) => o.id === current && !o.disabled)) {
        return current;
      }
      const stored = localStorage.getItem(ACTION_STORAGE_KEY);
      if (stored && options.some((o) => o.id === stored && !o.disabled)) {
        return stored;
      }
      return options.find((o) => !o.disabled)?.id ?? current;
    });
  }, [options]);

  function selectAction(id: string) {
    setSelectedId(id);
    localStorage.setItem(ACTION_STORAGE_KEY, id);
    const integrationId = parseAutofillId(id);
    if (integrationId != null) {
      localStorage.setItem(DEFAULT_INTEGRATION_KEY, String(integrationId));
    }
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

  function runSelectedAction() {
    if (selectedId === ACTION_EDIT_TAGS) {
      onEditTags();
      return;
    }
    if (selectedId === ACTION_REPAIR_FOLDER) {
      if (repairFolder) {
        onRepairFolder(repairFolder);
      }
      return;
    }
    const integrationId = parseAutofillId(selectedId);
    if (integrationId != null) {
      void runLookup(integrationId);
    }
  }

  const selectedLabel =
    options.find((o) => o.id === selectedId)?.label ?? options[0]?.label ?? "";

  if (options.length === 0) {
    return null;
  }

  return (
    <>
      <ComboActionButton
        options={options}
        value={selectedId}
        onValueChange={selectAction}
        onRun={runSelectedAction}
        loading={lookup.isPending || (selectedId === ACTION_REPAIR_FOLDER && scanPending)}
        loadingLabel={t("common.loading")}
        menuAriaLabel={t("library.chooseAlbumAction")}
        runAriaLabel={t("library.runAlbumAction", { action: selectedLabel })}
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
