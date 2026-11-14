-- A track's own artists, from its tags (owner decision 2026-11-14: add artist and
-- album). NULL: tags never read; empty: read, none found. Either way the track's
-- artist is its album artist. Album and album artist come from the existing
-- parent links, so renames cannot leave a stale copy here.
ALTER TABLE media_items ADD COLUMN artists TEXT[];
