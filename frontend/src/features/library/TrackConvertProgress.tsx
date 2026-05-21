import { Progress } from "@/components/ui/progress";
import { usePreferences } from "@/hooks/use-preferences";
import { cn } from "@/lib/utils";

import type { TrackConvertStatus } from "@/features/library/parseConvertFiles";

function progressValue(
  status: TrackConvertStatus,
  progressPct?: number,
): number {
  switch (status) {
    case "pending":
      return 0;
    case "running":
      return progressPct ?? 8;
    case "success":
    case "failed":
      return 100;
  }
}

type Props = {
  status: TrackConvertStatus;
  progressPct?: number;
  error?: string | null;
  className?: string;
};

export function TrackConvertProgress({
  status,
  progressPct,
  error,
  className,
}: Props) {
  const { t } = usePreferences();
  const label =
    status === "pending"
      ? t("library.convertTrackPending")
      : status === "running"
        ? progressPct != null
          ? `${t("library.convertTrackRunning")} ${Math.round(progressPct)}%`
          : t("library.convertTrackRunning")
        : status === "success"
          ? t("library.convertTrackSuccess")
          : t("library.convertTrackFailed");

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
          status === "failed"
            ? "text-destructive"
            : "text-muted-foreground",
        )}
        title={error ?? undefined}
      >
        {label}
        {status === "failed" && error ? ` — ${error}` : null}
      </p>
    </div>
  );
}
