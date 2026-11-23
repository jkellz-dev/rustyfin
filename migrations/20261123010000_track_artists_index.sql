-- An artist's tracks by name (a track's `artists`, guests included) are
-- found by index, not by reading every track (owner, 2026-11-23: Finamp's
-- artist page took seconds).
CREATE INDEX IF NOT EXISTS idx_media_items_artists ON media_items USING GIN (artists);
