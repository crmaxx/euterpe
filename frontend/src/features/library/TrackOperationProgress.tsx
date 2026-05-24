import { Progress } from "@/components/ui/progress";
import { usePreferences } from "@/hooks/use-preferences";
import { cn } from "@/lib/utils";

export type TrackOperationKind = "convert" | "cue";
export type TrackOperationStatus = "pending" | "running" | "failed";

function progressValue(
  status: TrackOperationStatus,
  progressPct?: number,
): number {
  switch (status) {
    case "pending":
      return 0;
    case "running":
      return progressPct ?? 8;
    case "failed":
      return 100;
  }
}

type Props = {
  kind: TrackOperationKind;
  status: TrackOperationStatus;
  progressPct?: number;
  error?: string | null;
  className?: string;
};

export function TrackOperationProgress({
  kind,
  status,
  progressPct,
  error,
  className,
}: Props) {
  const { t } = usePreferences();
  const prefix = kind === "cue" ? "cueTrack" : "convertTrack";
  const label =
    status === "pending"
      ? t(`library.${prefix}Pending`)
      : status === "running"
        ? progressPct != null
          ? `${t(`library.${prefix}Running`)} ${Math.round(progressPct)}%`
          : t(`library.${prefix}Running`)
        : t(`library.${prefix}Failed`);

  return (
    <div className={cn("px-4 pb-2 pl-14", className)}>
      <Progress
        value={progressValue(status, progressPct)}
        aria-label={label}
        className={cn(
          "h-1",
          status === "running" &&
            progressPct == null &&
            "[&>div]:animate-pulse",
        )}
      />
      <p
        className={cn(
          "mt-1 text-xs",
          status === "failed" ? "text-destructive" : "text-muted-foreground",
        )}
        title={error ?? undefined}
      >
        {label}
        {status === "failed" && error ? ` — ${error}` : null}
      </p>
    </div>
  );
}
