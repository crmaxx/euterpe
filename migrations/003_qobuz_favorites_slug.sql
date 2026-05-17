-- Phase 3 fix: Qobuz album/get needs short album ref or slug, not numeric favorites `id`.
-- Column stores `AlbumSummary::api_album_id()` (e.g. zg7pv28g4mldg or long slug).
ALTER TABLE qobuz_favorites ADD COLUMN slug TEXT;
