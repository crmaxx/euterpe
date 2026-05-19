import { startTransition, useEffect, useRef, useState } from "react";
import { Progress } from "@/components/ui/progress";
import {
  estimateScanEta,
  formatDuration,
  scanProgressPercent,
  type ScanProgressSample,
} from "@/lib/scanProgress";

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

  if (!running && status !== "success" && status !== "failed") {
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
          {running
            ? discovering
              ? "Discovering files…"
              : "Indexing library…"
            : status === "success"
              ? "Scan complete"
              : `Scan ${status}`}
        </span>
        {running && elapsedLabel ? (
          <span className="text-muted-foreground tabular-nums">Elapsed {elapsedLabel}</span>
        ) : null}
      </div>

      <Progress
        value={
          discovering
            ? undefined
            : running
              ? (percent ?? 0)
              : (percent ?? 100)
        }
        className={discovering || (running && percent == null) ? "animate-pulse" : undefined}
      />

      <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-sm sm:grid-cols-4">
        <div>
          <dt className="text-muted-foreground">
            {discovering ? "Found so far" : "Total files"}
          </dt>
          <dd className="font-medium tabular-nums">
            {discovering ? filesSeen : filesTotal}
          </dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Indexed</dt>
          <dd className="font-medium tabular-nums">{filesIndexed}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Index queue</dt>
          <dd className="font-medium tabular-nums">{indexQueue}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">ETA</dt>
          <dd className="font-medium tabular-nums">
            {running
              ? discovering
                ? "—"
                : eta ?? (filesTotal > 0 ? "calculating…" : "—")
              : "—"}
          </dd>
        </div>
      </dl>

      {percent != null && running && !discovering ? (
        <p className="text-xs text-muted-foreground">
          Bar shows indexed vs total files ({percent}% written to the database).
        </p>
      ) : null}
    </div>
  );
}
