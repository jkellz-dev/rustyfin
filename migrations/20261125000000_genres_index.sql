-- Genre pages (GenreIds) and genre pictures match items by genre: 1.3 s a
-- page and 0.3 s a picture over the whole table without an index (owner,
-- 2026-11-24, pg_stat_statements). A GIN index answers `genres && ...`.
CREATE INDEX IF NOT EXISTS idx_media_items_genres ON media_items USING GIN (genres);
