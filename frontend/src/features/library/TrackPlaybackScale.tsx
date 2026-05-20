import { useCallback, useRef, useState } from "react";
import { formatDuration } from "@/lib/format";
import { usePreferences } from "@/hooks/use-preferences";

type TrackPlaybackScaleProps = {
  positionSec: number;
  durationSec: number;
  onSeek: (ratio: number) => void;
};

export function TrackPlaybackScale({
  positionSec,
  durationSec,
  onSeek,
}: TrackPlaybackScaleProps) {
  const { t } = usePreferences();
  const railRef = useRef<HTMLDivElement>(null);
  const [hoverRatio, setHoverRatio] = useState<number | null>(null);
  const [dragging, setDragging] = useState(false);

  const progress =
    durationSec > 0
      ? Math.max(0, Math.min(1, positionSec / durationSec))
      : 0;
  const displayRatio = hoverRatio ?? progress;

  const ratioFromClientX = useCallback((clientX: number) => {
    const rail = railRef.current;
    if (!rail) {
      return 0;
    }
    const rect = rail.getBoundingClientRect();
    if (rect.width <= 0) {
      return 0;
    }
    return Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
  }, []);

  const handlePointer = useCallback(
    (clientX: number) => {
      const ratio = ratioFromClientX(clientX);
      setHoverRatio(ratio);
      if (dragging) {
        onSeek(ratio);
      }
    },
    [dragging, onSeek, ratioFromClientX],
  );

  const previewSec =
    durationSec > 0 ? displayRatio * durationSec : positionSec;

  return (
    <div
      ref={railRef}
      className="relative h-6 px-4 pb-2"
      role="group"
      aria-label={t("library.seek")}
      onPointerMove={(e) => handlePointer(e.clientX)}
      onPointerLeave={() => {
        if (!dragging) {
          setHoverRatio(null);
        }
      }}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        setDragging(true);
        const ratio = ratioFromClientX(e.clientX);
        setHoverRatio(ratio);
        onSeek(ratio);
      }}
      onPointerUp={(e) => {
        setDragging(false);
        if (e.currentTarget.hasPointerCapture(e.pointerId)) {
          e.currentTarget.releasePointerCapture(e.pointerId);
        }
        setHoverRatio(null);
      }}
    >
      <div className="relative h-1 w-full rounded-full bg-muted">
        <div
          className="absolute inset-y-0 left-0 rounded-full bg-amber-400"
          style={{ width: `${progress * 100}%` }}
        />
        <div
          className="absolute top-1/2 size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-amber-400 shadow-sm"
          style={{ left: `${progress * 100}%` }}
        />
      </div>
      {(hoverRatio != null || dragging) && durationSec > 0 ? (
        <span
          className="pointer-events-none absolute -top-5 z-10 -translate-x-1/2 rounded bg-popover px-1.5 py-0.5 text-[10px] font-medium tabular-nums text-popover-foreground shadow-sm"
          style={{ left: `calc(${displayRatio * 100}% + 1rem)` }}
        >
          {formatDuration(previewSec)}
        </span>
      ) : null}
      <input
        type="range"
        min={0}
        max={1000}
        value={Math.round(progress * 1000)}
        onChange={(e) => onSeek(Number(e.target.value) / 1000)}
        className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
        aria-label={t("library.seek")}
      />
    </div>
  );
}
