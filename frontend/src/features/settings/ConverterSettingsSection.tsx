import { useState } from "react";
import {
  useConverterSettings,
  usePatchConverterSettings,
} from "@/api/hooks";
import type { ConverterSettings } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";

function ConverterSettingsForm({ settings }: { settings: ConverterSettings }) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const patch = usePatchConverterSettings();

  const [autoEnabled, setAutoEnabled] = useState(() => !!settings.auto_enabled);
  const [filePolicy, setFilePolicy] = useState(
    () => settings.file_policy ?? "sibling_then_delete",
  );
  const [parallelism, setParallelism] = useState(() =>
    String(settings.parallelism ?? 5),
  );
  const [preset, setPreset] = useState(
    () => settings.flac_encode?.preset ?? "balanced",
  );
  const [blockSize, setBlockSize] = useState(() =>
    settings.flac_encode?.block_size != null
      ? String(settings.flac_encode.block_size)
      : "",
  );
  const [multithread, setMultithread] = useState(
    () => !!settings.flac_encode?.multithread,
  );
  const [showAdvanced, setShowAdvanced] = useState(false);

  const save = async () => {
    const parallelismNum = Number(parallelism);
    const blockSizeNum = blockSize.trim() ? Number(blockSize) : null;
    try {
      await patch.mutateAsync({
        auto_enabled: autoEnabled,
        file_policy: filePolicy as ConverterSettings["file_policy"],
        parallelism: parallelismNum,
        flac_encode: {
          preset: preset as "fast" | "balanced" | "best",
          block_size: blockSizeNum,
          multithread,
        },
      });
      toast({ title: t("settings.converter.saved") });
    } catch (e) {
      toast({
        title: t("settings.converter.saveFailed"),
        description: e instanceof Error ? e.message : t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  return (
    <section className="space-y-4 rounded-lg border border-border bg-card p-4">
      <div>
        <h3 className="font-medium">{t("settings.converter.title")}</h3>
        <p className="text-sm text-muted-foreground">
          {t("settings.converter.hint")}
        </p>
      </div>
      <div className="grid max-w-md gap-3">
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={autoEnabled}
            onChange={(e) => setAutoEnabled(e.target.checked)}
          />
          {t("settings.converter.autoEnabled")}
        </label>
        <div className="space-y-1">
          <Label>{t("settings.converter.filePolicy")}</Label>
          <Select
            value={filePolicy}
            onValueChange={(v) =>
              setFilePolicy(v as "replace_in_place" | "sibling_then_delete")
            }
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="sibling_then_delete">
                {t("settings.converter.policySibling")}
              </SelectItem>
              <SelectItem value="replace_in_place">
                {t("settings.converter.policyReplace")}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label htmlFor="converter-parallelism">
            {t("settings.converter.parallelism")}
          </Label>
          <Input
            id="converter-parallelism"
            type="number"
            min={1}
            max={32}
            value={parallelism}
            onChange={(e) => setParallelism(e.target.value)}
          />
        </div>
        <div className="space-y-1">
          <Label>{t("settings.converter.flacPreset")}</Label>
          <p className="text-xs text-muted-foreground">
            {t("settings.converter.flacPresetHint")}
          </p>
          <Select
            value={preset}
            onValueChange={(v) => setPreset(v as "fast" | "balanced" | "best")}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="fast">{t("settings.converter.presetFast")}</SelectItem>
              <SelectItem value="balanced">
                {t("settings.converter.presetBalanced")}
              </SelectItem>
              <SelectItem value="best">{t("settings.converter.presetBest")}</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <button
          type="button"
          className="text-left text-sm text-muted-foreground underline-offset-2 hover:underline"
          onClick={() => setShowAdvanced((v) => !v)}
        >
          {showAdvanced
            ? t("settings.converter.hideAdvanced")
            : t("settings.converter.showAdvanced")}
        </button>
        {showAdvanced ? (
          <div className="space-y-1">
            <Label htmlFor="converter-block-size">
              {t("settings.converter.blockSize")}
            </Label>
            <Input
              id="converter-block-size"
              type="number"
              placeholder={t("settings.converter.blockSizePlaceholder")}
              value={blockSize}
              onChange={(e) => setBlockSize(e.target.value)}
            />
          </div>
        ) : null}
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={multithread}
            onChange={(e) => setMultithread(e.target.checked)}
          />
          {t("settings.converter.multithread")}
        </label>
        <p className="text-xs text-muted-foreground">
          {t("settings.converter.multithreadHint")}
        </p>
      </div>
      <Button disabled={patch.isPending} onClick={() => void save()}>
        {t("common.save")}
      </Button>
    </section>
  );
}

export function ConverterSettingsSection() {
  const { t } = usePreferences();
  const { data, isLoading } = useConverterSettings();

  if (isLoading || !data) {
    return <p className="text-sm text-muted-foreground">{t("common.loading")}</p>;
  }

  const formKey = JSON.stringify(data);
  return <ConverterSettingsForm key={formKey} settings={data} />;
}
