-- Where a Rustyfin link's change feed was read up to (roadmap 16b.2r): the
-- cursor its first page gave, kept after a sync that worked.
ALTER TABLE links ADD COLUMN IF NOT EXISTS catalog_cursor TEXT;
