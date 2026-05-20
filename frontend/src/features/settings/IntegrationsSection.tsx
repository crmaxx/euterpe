import { Power, Settings2, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import type {
  IntegrationCatalogEntry,
  IntegrationListItem,
} from "@/api/client";
import {
  useCreateIntegration,
  useDeleteIntegration,
  useIntegrations,
  useIntegrationsCatalog,
  usePatchIntegration,
} from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Modal } from "@/components/modal";
import { useToast } from "@/hooks/use-toast";
import { cn } from "@/lib/utils";
import { usePreferences } from "@/hooks/use-preferences";

function IntegrationFormFields({
  entry,
  config,
  secrets,
  onConfigChange,
  onSecretChange,
}: {
  entry: IntegrationCatalogEntry;
  config: Record<string, string>;
  secrets: Record<string, string>;
  onConfigChange: (key: string, value: string) => void;
  onSecretChange: (key: string, value: string) => void;
}) {
  return (
    <div className="space-y-3">
      {entry.config_schema.map((field) => {
        const value = field.secret
          ? (secrets[field.key] ?? "")
          : (config[field.key] ?? "");
        return (
          <div key={field.key} className="space-y-1">
            <Label htmlFor={`int-${field.key}`}>{field.label}</Label>
            <Input
              id={`int-${field.key}`}
              type={field.secret ? "password" : "text"}
              placeholder={field.placeholder ?? undefined}
              value={value}
              onChange={(e) =>
                field.secret
                  ? onSecretChange(field.key, e.target.value)
                  : onConfigChange(field.key, e.target.value)
              }
            />
          </div>
        );
      })}
    </div>
  );
}

function buildPayload(
  entry: IntegrationCatalogEntry,
  config: Record<string, string>,
  secrets: Record<string, string>,
) {
  const configObj: Record<string, string> = {};
  const secretsObj: Record<string, string> = {};
  for (const field of entry.config_schema) {
    if (field.secret) {
      const v = secrets[field.key]?.trim();
      if (v) {
        secretsObj[field.key] = v;
      }
    } else {
      const v = config[field.key]?.trim();
      if (v) {
        configObj[field.key] = v;
      }
    }
  }
  return { config: configObj, secrets: secretsObj };
}

export function IntegrationsSection() {
  const { t } = usePreferences();
  const { toast } = useToast();
  const { data: list } = useIntegrations();
  const { data: catalog } = useIntegrationsCatalog();
  const createIntegration = useCreateIntegration();
  const patchIntegration = usePatchIntegration();
  const deleteIntegration = useDeleteIntegration();

  const [addOpen, setAddOpen] = useState(false);
  const [addProvider, setAddProvider] = useState<string | null>(null);
  const [config, setConfig] = useState<Record<string, string>>({});
  const [secrets, setSecrets] = useState<Record<string, string>>({});
  const [editItem, setEditItem] = useState<IntegrationListItem | null>(null);

  const catalogByProvider = useMemo(() => {
    const m = new Map<string, IntegrationCatalogEntry>();
    for (const e of catalog?.items ?? []) {
      m.set(e.provider, e);
    }
    return m;
  }, [catalog?.items]);

  const addEntry = addProvider ? catalogByProvider.get(addProvider) : undefined;
  const editEntry = editItem
    ? catalogByProvider.get(editItem.provider)
    : undefined;

  const configuredProviders = new Set(
    (list?.items ?? []).map((i) => i.provider),
  );
  const availableToAdd =
    catalog?.items.filter((e) => !configuredProviders.has(e.provider)) ?? [];

  async function handleCreate() {
    if (!addEntry) {
      return;
    }
    const { config: c, secrets: s } = buildPayload(addEntry, config, secrets);
    try {
      await createIntegration.mutateAsync({
        provider: addEntry.provider,
        type: "tag_source",
        config: c,
        secrets: Object.keys(s).length > 0 ? s : undefined,
      });
      toast({ title: t("integrations.toast.added") });
      setAddOpen(false);
      setAddProvider(null);
      setConfig({});
      setSecrets({});
    } catch (e) {
      toast({
        title: t("integrations.toast.addFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  }

  async function handlePatch() {
    if (!editItem || !editEntry) {
      return;
    }
    const { config: c, secrets: s } = buildPayload(editEntry, config, secrets);
    try {
      await patchIntegration.mutateAsync({
        id: editItem.id,
        body: {
          config: c,
          ...(Object.keys(s).length > 0 ? { secrets: s } : {}),
        },
      });
      toast({ title: t("integrations.toast.updated") });
      setEditItem(null);
    } catch (e) {
      toast({
        title: t("integrations.toast.updateFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  }

  async function toggleEnabled(item: IntegrationListItem) {
    try {
      await patchIntegration.mutateAsync({
        id: item.id,
        body: { enabled: !item.enabled },
      });
    } catch (e) {
      toast({
        title: t("integrations.toast.updateFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  }

  async function handleDelete(item: IntegrationListItem) {
    if (!window.confirm(t("integrations.removeConfirm", { name: item.display_name }))) {
      return;
    }
    try {
      await deleteIntegration.mutateAsync(item.id);
      toast({ title: t("integrations.toast.removed") });
    } catch (e) {
      toast({
        title: t("integrations.toast.deleteFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  }

  function openEdit(item: IntegrationListItem) {
    setEditItem(item);
    const entry = catalogByProvider.get(item.provider);
    const cfg: Record<string, string> = {};
    if (item.config && typeof item.config === "object") {
      for (const [k, v] of Object.entries(item.config as Record<string, unknown>)) {
        if (typeof v === "string") {
          cfg[k] = v;
        }
      }
    }
    setConfig(cfg);
    setSecrets({});
    void entry;
  }

  return (
    <section className="space-y-4 rounded-lg border border-border bg-card p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h3 className="font-medium">{t("integrations.title")}</h3>
          <p className="text-sm text-muted-foreground">{t("integrations.subtitle")}</p>
        </div>
        <Button
          type="button"
          size="sm"
          disabled={availableToAdd.length === 0}
          onClick={() => {
            setAddOpen(true);
            setAddProvider(availableToAdd[0]?.provider ?? null);
            setConfig({});
            setSecrets({});
          }}
        >
          {t("integrations.add")}
        </Button>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-border text-left text-muted-foreground">
              <th className="pb-2 pr-4 font-medium">{t("integrations.name")}</th>
              <th className="pb-2 pr-4 font-medium">{t("integrations.type")}</th>
              <th className="pb-2 pr-4 font-medium">{t("integrations.provider")}</th>
              <th className="pb-2 pr-4 font-medium">{t("integrations.status")}</th>
              <th className="pb-2 font-medium">{t("integrations.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {(list?.items ?? []).map((item) => (
              <tr key={item.id} className="border-b border-border/60">
                <td className="py-2 pr-4">{item.display_name}</td>
                <td className="py-2 pr-4">
                  {item.integration_type === "tag_source"
                    ? t("integrations.tagSource")
                    : item.integration_type}
                </td>
                <td className="py-2 pr-4">{item.provider}</td>
                <td className="py-2 pr-4">
                  {item.enabled ? t("integrations.enabled") : t("integrations.disabled")}
                </td>
                <td className="py-2">
                  <div className="flex items-center gap-1">
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="size-8 shrink-0 p-0"
                      aria-label={t("integrations.configure")}
                      onClick={() => openEdit(item)}
                    >
                      <Settings2 className="size-4" aria-hidden />
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="size-8 shrink-0 p-0"
                      aria-label={
                        item.enabled
                          ? t("integrations.disable")
                          : t("integrations.enable")
                      }
                      onClick={() => void toggleEnabled(item)}
                    >
                      <Power
                        className={cn(
                          "size-4",
                          item.enabled ? "text-foreground" : "text-muted-foreground",
                        )}
                        aria-hidden
                      />
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="size-8 shrink-0 p-0 text-muted-foreground hover:text-destructive"
                      aria-label={t("common.delete")}
                      onClick={() => void handleDelete(item)}
                    >
                      <Trash2 className="size-4" aria-hidden />
                    </Button>
                  </div>
                </td>
              </tr>
            ))}
            {(list?.items ?? []).length === 0 && (
              <tr>
                <td colSpan={5} className="py-4 text-muted-foreground">
                  {t("integrations.empty")}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <Modal open={addOpen} onClose={() => setAddOpen(false)}>
        <h3 className="font-medium">{t("integrations.addTitle")}</h3>
        <div className="space-y-2">
          <Label>{t("integrations.provider")}</Label>
          <select
            className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
            value={addProvider ?? ""}
            onChange={(e) => {
              setAddProvider(e.target.value);
              setConfig({});
              setSecrets({});
            }}
          >
            {availableToAdd.map((e) => (
              <option key={e.provider} value={e.provider}>
                {e.label}
              </option>
            ))}
          </select>
        </div>
        {addEntry && (
          <IntegrationFormFields
            entry={addEntry}
            config={config}
            secrets={secrets}
            onConfigChange={(k, v) => setConfig((c) => ({ ...c, [k]: v }))}
            onSecretChange={(k, v) => setSecrets((s) => ({ ...s, [k]: v }))}
          />
        )}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={() => setAddOpen(false)}>
            {t("common.cancel")}
          </Button>
          <Button
            type="button"
            disabled={!addEntry || createIntegration.isPending}
            onClick={() => void handleCreate()}
          >
            {t("common.add")}
          </Button>
        </div>
      </Modal>

      <Modal open={editItem != null} onClose={() => setEditItem(null)}>
        <h3 className="font-medium">
          {t("integrations.configureTitle", { name: editItem?.display_name ?? "" })}
        </h3>
        {editEntry && (
          <IntegrationFormFields
            entry={editEntry}
            config={config}
            secrets={secrets}
            onConfigChange={(k, v) => setConfig((c) => ({ ...c, [k]: v }))}
            onSecretChange={(k, v) => setSecrets((s) => ({ ...s, [k]: v }))}
          />
        )}
        <p className="text-xs text-muted-foreground">
          {t("integrations.secretsHint")}
        </p>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={() => setEditItem(null)}>
            {t("common.cancel")}
          </Button>
          <Button
            type="button"
            disabled={patchIntegration.isPending}
            onClick={() => void handlePatch()}
          >
            {t("common.save")}
          </Button>
        </div>
      </Modal>
    </section>
  );
}
