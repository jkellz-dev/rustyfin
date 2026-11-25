-- Counting a library's items of one type (every list's TotalRecordCount):
-- 0.6-0.7 s for the music library's tracks, reading every row (owner,
-- 2026-11-24, a trace). With the link column included, the count reads this
-- index alone.
CREATE INDEX IF NOT EXISTS idx_media_items_library_type
    ON media_items (library_id, item_type) INCLUDE (link_id)
    WHERE owner_id IS NULL;
