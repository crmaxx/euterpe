import { startTransition, useEffect, useRef, useState } from "react";
import { Progress } from "@/components/ui/progress";
import {
  estimateScanEta,
  formatDuration,
  scanProgressPercent,
  type ScanProgressSample,
} from "@/lib/scanProgress";
import { usePreferences } from "@/hooks/use-preferences";

type Props = {
  filesSeen: number;
  filesProcessed: number;
  filesIndexed: number;
  filesTotal: number;
  status: string;
  startedAt?: string;
  compact?: boolean;
};

export function LibraryScanProgress({
  filesSeen,
  filesProcessed,
  filesIndexed,
  filesTotal,
  status,
  startedAt,
  compact = false,
}: Props) {
  const { t } = usePreferences();
  const running = status === "running";
  const discovering = running && filesTotal <= 0;
  const [samples, setSamples] = useState<ScanProgressSample[]>([]);
  const lastKey = useRef("");
  const [elapsedLabel, setElapsedLabel] = useState<string | null>(null);

  useEffect(() => {
    if (!running) {
      startTransition(() => {
        lastKey.current = "";
        setSamples([]);
      });
      return;
    }
    const key = `${filesIndexed}:${filesTotal}`;
    if (key === lastKey.current) return;
    lastKey.current = key;
    const t = Date.now();
    startTransition(() => {
      setSamples((prev) =>
        [...prev, { t, filesIndexed, filesTotal }].slice(-40),
      );
    });
  }, [running, filesIndexed, filesTotal]);

  useEffect(() => {
    if (!running || !startedAt) return;
    const t0 = Date.parse(startedAt);
    if (Number.isNaN(t0)) return;
    const tick = () => {
      setElapsedLabel(formatDuration((Date.now() - t0) / 1000));
    };
    const timeoutId = window.setTimeout(tick, 0);
    const intervalId = window.setInterval(tick, 1000);
    return () => {
      window.clearTimeout(timeoutId);
      window.clearInterval(intervalId);
    };
  }, [running, startedAt]);

  const percent = scanProgressPercent(filesIndexed, filesTotal);
  const eta =
    running && !discovering
      ? estimateScanEta(filesIndexed, filesTotal, samples)
      : null;

  const indexQueue = Math.max(0, filesProcessed - filesIndexed);

  const cancelled = status === "cancelled";

  if (!running && status !== "success" && status !== "failed" && !cancelled) {
    return null;
  }

  if (compact) {
    const label =
      filesTotal > 0
        ? `${filesIndexed}/${filesTotal}`
        : `${filesIndexed}/${filesSeen}`;
    return (
      <span className="tabular-nums">
        {label}
        {running && eta ? ` · ${eta}` : null}
      </span>
    );
  }

  return (
    <div className="space-y-3 rounded-lg border border-border bg-card/50 p-4">
      <div className="flex flex-wrap items-baseline justify-between gap-2 text-sm">
        <span className="font-medium">
          {cancelled
            ? t("scan.cancelled")
            : running
              ? discovering
                ? t("scan.discovering")
                : t("scan.indexing")
              : status === "success"
                ? t("scan.complete")
                : t("scan.status", { status })}
        </span>
        {running && elapsedLabel ? (
          <span className="text-muted-foreground tabular-nums">
            {t("scan.elapsed", { time: elapsedLabel })}
          </span>
        ) : null}
      </div>

      {!cancelled ? (
        <Progress
          value={
            discovering
              ? undefined
              : running
                ? (percent ?? 0)
                : (percent ?? 100)
          }
          className={
            discovering || (running && percent == null) ? "animate-pulse" : undefined
          }
        />
      ) : null}

      <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-sm sm:grid-cols-4">
        <div>
          <dt className="text-muted-foreground">
            {discovering ? t("scan.foundSoFar") : t("scan.totalFiles")}
          </dt>
          <dd className="font-medium tabular-nums">
            {discovering ? filesSeen : filesTotal}
          </dd>
        </div>
        <div>
          <dt className="text-muted-foreground">{t("scan.indexed")}</dt>
          <dd className="font-medium tabular-nums">{filesIndexed}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">{t("scan.indexQueue")}</dt>
          <dd className="font-medium tabular-nums">{indexQueue}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">{t("scan.eta")}</dt>
          <dd className="font-medium tabular-nums">
            {running
              ? discovering
                ? "—"
                : eta ?? (filesTotal > 0 ? t("scan.calculating") : "—")
              : "—"}
          </dd>
        </div>
      </dl>

      {percent != null && running && !discovering ? (
        <p className="text-xs text-muted-foreground">
          {t("scan.barHint", { pct: percent })}
        </p>
      ) : null}
    </div>
  );
}
