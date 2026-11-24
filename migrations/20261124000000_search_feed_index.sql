-- The search index's change feed reads every title changed since its last
-- sync (linked copies too), every few seconds. The catalog's index leaves out
-- linked copies, so this read scanned the table: 80 ms a sync (owner,
-- 2026-11-23, pg_stat_statements). Titles and tracks only (no owner), as the
-- feed reads them.
CREATE INDEX IF NOT EXISTS idx_media_items_search_change ON media_items (change_xid, id)
    WHERE owner_id IS NULL;
