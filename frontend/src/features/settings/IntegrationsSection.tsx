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

const TYPE_LABEL: Record<string, string> = {
  tag_source: "Tag source",
};

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
      toast({ title: "Integration added" });
      setAddOpen(false);
      setAddProvider(null);
      setConfig({});
      setSecrets({});
    } catch (e) {
      toast({
        title: "Could not add integration",
        description: e instanceof Error ? e.message : "Unknown error",
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
      toast({ title: "Integration updated" });
      setEditItem(null);
    } catch (e) {
      toast({
        title: "Update failed",
        description: e instanceof Error ? e.message : "Unknown error",
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
        title: "Update failed",
        description: e instanceof Error ? e.message : "Unknown error",
        variant: "destructive",
      });
    }
  }

  async function handleDelete(item: IntegrationListItem) {
    if (!window.confirm(`Remove ${item.display_name}?`)) {
      return;
    }
    try {
      await deleteIntegration.mutateAsync(item.id);
      toast({ title: "Integration removed" });
    } catch (e) {
      toast({
        title: "Delete failed",
        description: e instanceof Error ? e.message : "Unknown error",
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
          <h3 className="font-medium">Integrations</h3>
          <p className="text-sm text-muted-foreground">
            Tag sources for autofill in the library.
          </p>
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
          Add integration
        </Button>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-border text-left text-muted-foreground">
              <th className="pb-2 pr-4 font-medium">Name</th>
              <th className="pb-2 pr-4 font-medium">Type</th>
              <th className="pb-2 pr-4 font-medium">Provider</th>
              <th className="pb-2 pr-4 font-medium">Status</th>
              <th className="pb-2 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {(list?.items ?? []).map((item) => (
              <tr key={item.id} className="border-b border-border/60">
                <td className="py-2 pr-4">{item.display_name}</td>
                <td className="py-2 pr-4">
                  {TYPE_LABEL[item.integration_type] ?? item.integration_type}
                </td>
                <td className="py-2 pr-4">{item.provider}</td>
                <td className="py-2 pr-4">
                  {item.enabled ? "Enabled" : "Disabled"}
                </td>
                <td className="py-2">
                  <div className="flex flex-wrap gap-1">
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => openEdit(item)}
                    >
                      Configure
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => void toggleEnabled(item)}
                    >
                      {item.enabled ? "Disable" : "Enable"}
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      onClick={() => void handleDelete(item)}
                    >
                      Delete
                    </Button>
                  </div>
                </td>
              </tr>
            ))}
            {(list?.items ?? []).length === 0 && (
              <tr>
                <td colSpan={5} className="py-4 text-muted-foreground">
                  No integrations configured.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <Modal open={addOpen} onClose={() => setAddOpen(false)}>
        <h3 className="font-medium">Add integration</h3>
        <div className="space-y-2">
          <Label>Provider</Label>
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
            Cancel
          </Button>
          <Button
            type="button"
            disabled={!addEntry || createIntegration.isPending}
            onClick={() => void handleCreate()}
          >
            Add
          </Button>
        </div>
      </Modal>

      <Modal open={editItem != null} onClose={() => setEditItem(null)}>
        <h3 className="font-medium">Configure {editItem?.display_name}</h3>
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
          Leave secret fields empty to keep existing values.
        </p>
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={() => setEditItem(null)}>
            Cancel
          </Button>
          <Button
            type="button"
            disabled={patchIntegration.isPending}
            onClick={() => void handlePatch()}
          >
            Save
          </Button>
        </div>
      </Modal>
    </section>
  );
}
