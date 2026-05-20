import { useCallback, useEffect, useRef, useState } from "react";
import { Howl, Howler } from "howler";
import { libraryTrackStreamUrl } from "@/api/client";
import { useToast } from "@/hooks/use-toast";
import { usePreferences } from "@/hooks/use-preferences";

export type AlbumPlayerTrack = {
  id: number;
  title: string;
  /** Howler `format` — stream URLs have no file extension. */
  format: string[];
  durationSec?: number;
};

export type PlaybackProgress = {
  trackId: number;
  positionSec: number;
  durationSec: number;
};

type PlaybackSession = {
  albumId: number;
  trackId: number;
  playing: boolean;
};

export function howlerFormatFromPath(path: string): string[] {
  const ext = path.split(/[/\\]/).pop()?.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "flac":
      return ["flac"];
    case "mp3":
      return ["mp3"];
    case "m4a":
      return ["m4a"];
    case "aac":
      return ["aac"];
    case "ogg":
      return ["ogg"];
    case "opus":
      return ["opus"];
    case "wav":
      return ["wav"];
    case "webm":
      return ["webm"];
    default:
      return ["mp3"];
  }
}

function unlockAudioContext() {
  const ctx = Howler.ctx;
  if (ctx?.state === "suspended") {
    void ctx.resume();
  }
}

type HowlHandlers = {
  onload?: () => void;
  onend?: () => void;
  onplay?: () => void;
  onpause?: () => void;
  onstop?: () => void;
  onloaderror?: () => void;
  onplayerror?: () => void;
};

function createHowl(
  src: string,
  format: string[],
  handlers: HowlHandlers,
): Howl {
  return new Howl({
    src: [src],
    format,
    html5: true,
    volume: 1,
    ...handlers,
  });
}

function howlDurationSec(howl: Howl, fallback?: number): number {
  const d = howl.duration();
  if (typeof d === "number" && Number.isFinite(d) && d > 0) {
    return d;
  }
  return fallback ?? 0;
}

export function useAlbumPlayer(
  albumId: number | null,
  queue: AlbumPlayerTrack[],
) {
  const { t } = usePreferences();
  const { toast } = useToast();
  const howlRef = useRef<Howl | null>(null);
  const queueRef = useRef<AlbumPlayerTrack[]>([]);
  const indexRef = useRef(0);
  const playTrackAtRef = useRef<(idx: number) => void>(() => {});
  const [session, setSession] = useState<PlaybackSession | null>(null);
  const [playback, setPlayback] = useState<PlaybackProgress | null>(null);

  const playingTrackId =
    albumId != null && session?.albumId === albumId ? session.trackId : null;
  const isPlaying =
    albumId != null && session?.albumId === albumId && session.playing;

  const unloadHowl = useCallback(() => {
    const howl = howlRef.current;
    if (!howl) {
      return;
    }
    // Drop ref before unload so a late onstop from the old instance cannot endSession().
    howlRef.current = null;
    howl.unload();
  }, []);

  const endSession = useCallback(() => {
    unloadHowl();
    setSession(null);
    setPlayback(null);
  }, [unloadHowl]);

  const skipToNext = useCallback(() => {
    indexRef.current += 1;
    if (indexRef.current < queueRef.current.length) {
      playTrackAtRef.current(indexRef.current);
    } else {
      endSession();
    }
  }, [endSession]);

  const playTrackAt = useCallback(
    (idx: number) => {
      if (albumId == null) {
        return;
      }
      const q = queueRef.current;
      const track = q[idx];
      if (!track) {
        endSession();
        return;
      }

      unloadHowl();
      setSession({ albumId, trackId: track.id, playing: false });
      setPlayback({
        trackId: track.id,
        positionSec: 0,
        durationSec: track.durationSec ?? 0,
      });

      const src = libraryTrackStreamUrl(track.id);
      const howl = createHowl(src, track.format, {
        onend: skipToNext,
        onplay: () =>
          setSession((s) =>
            s?.albumId === albumId && s.trackId === track.id
              ? { ...s, playing: true }
              : s,
          ),
        onpause: () =>
          setSession((s) =>
            s?.albumId === albumId && s.trackId === track.id
              ? { ...s, playing: false }
              : s,
          ),
        onload: () => {
          howl.play();
        },
        onloaderror: () => {
          toast({
            title: t("library.playbackFailed"),
            description: track.title,
            variant: "destructive",
          });
          skipToNext();
        },
        onplayerror: () => {
          toast({
            title: t("library.playbackFailed"),
            description: track.title,
            variant: "destructive",
          });
          skipToNext();
        },
      });
      howlRef.current = howl;
    },
    [albumId, endSession, skipToNext, toast, t, unloadHowl],
  );

  useEffect(() => {
    playTrackAtRef.current = playTrackAt;
  }, [playTrackAt]);

  useEffect(() => {
    if (albumId == null || session?.albumId !== albumId) {
      return;
    }

    let raf = 0;
    const tick = () => {
      const howl = howlRef.current;
      if (!howl || session.albumId !== albumId) {
        return;
      }
      const pos = howl.seek() as number;
      const track = queueRef.current.find((t) => t.id === session.trackId);
      const durationSec = howlDurationSec(howl, track?.durationSec);
      setPlayback({
        trackId: session.trackId,
        positionSec: typeof pos === "number" && Number.isFinite(pos) ? pos : 0,
        durationSec,
      });
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [session, albumId]);

  const seekTo = useCallback((ratio: number) => {
    const howl = howlRef.current;
    if (!howl) {
      return;
    }
    const track = queueRef.current.find((t) => t.id === session?.trackId);
    const durationSec = howlDurationSec(howl, track?.durationSec);
    if (durationSec <= 0) {
      return;
    }
    const clamped = Math.max(0, Math.min(1, ratio));
    howl.seek(clamped * durationSec);
  }, [session?.trackId]);

  const playFromIndex = useCallback(
    (startIndex: number) => {
      if (queue.length === 0 || albumId == null) {
        return;
      }
      unlockAudioContext();
      queueRef.current = queue;
      indexRef.current = Math.max(0, Math.min(startIndex, queue.length - 1));
      playTrackAt(indexRef.current);
    },
    [albumId, queue, playTrackAt],
  );

  const playAlbum = useCallback(() => {
    playFromIndex(0);
  }, [playFromIndex]);

  const togglePlayPause = useCallback(() => {
    unlockAudioContext();
    const howl = howlRef.current;
    if (!howl) {
      return;
    }
    if (howl.playing()) {
      howl.pause();
    } else {
      howl.play();
    }
  }, []);

  const playTrack = useCallback(
    (trackId: number) => {
      const idx = queue.findIndex((t) => t.id === trackId);
      if (idx < 0) {
        return;
      }
      if (playingTrackId === trackId && howlRef.current) {
        togglePlayPause();
        return;
      }
      playFromIndex(idx);
    },
    [queue, playingTrackId, playFromIndex, togglePlayPause],
  );

  const stop = endSession;

  useEffect(() => {
    unloadHowl();
  }, [albumId, unloadHowl]);

  useEffect(() => () => unloadHowl(), [unloadHowl]);

  const isAlbumActive =
    playingTrackId != null && queue.some((t) => t.id === playingTrackId);

  const activePlayback =
    albumId != null &&
    playback != null &&
    session?.albumId === albumId &&
    session.trackId === playback.trackId
      ? playback
      : null;

  return {
    playingTrackId,
    isPlaying,
    isAlbumActive,
    playback: activePlayback,
    playAlbum,
    playTrack,
    stop,
    togglePlayPause,
    seekTo,
  };
}
