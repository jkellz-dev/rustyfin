-- Browsing by name (SortBy=SortName, the default) sorted every matching row
-- on each page: 135 ms for the first page of 75,000 tracks, 712 ms deep in
-- the list (11d stress run, 2026-11-16). These indexes hold the same sort key
-- as ItemQuery::JELLYFIN_SORT_KEY in crates/rustyfin-db (keep the two identical, or Postgres
-- will not use them): one per type for typed lists, one across all items.
CREATE INDEX IF NOT EXISTS idx_media_items_type_sort_key ON media_items (
    item_type,
    (CASE item_type WHEN 'Season' THEN COALESCE(lpad(index_number::text, greatest(4, length(index_number::text)), '0'), COALESCE(sort_name, name)) WHEN 'Episode' THEN COALESCE(lpad(parent_index_number::text, greatest(3, length(parent_index_number::text)), '0') || ' - ', '') || COALESCE(lpad(index_number::text, greatest(4, length(index_number::text)), '0') || ' - ', '') || COALESCE(sort_name, name) WHEN 'Audio' THEN COALESCE(lpad(parent_index_number::text, greatest(4, length(parent_index_number::text)), '0') || ' - ', '') || COALESCE(lpad(index_number::text, greatest(4, length(index_number::text)), '0') || ' - ', '') || COALESCE(sort_name, name) ELSE sort_name END),
    sort_name,
    name
) WHERE owner_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_media_items_sort_key ON media_items (
    (CASE item_type WHEN 'Season' THEN COALESCE(lpad(index_number::text, greatest(4, length(index_number::text)), '0'), COALESCE(sort_name, name)) WHEN 'Episode' THEN COALESCE(lpad(parent_index_number::text, greatest(3, length(parent_index_number::text)), '0') || ' - ', '') || COALESCE(lpad(index_number::text, greatest(4, length(index_number::text)), '0') || ' - ', '') || COALESCE(sort_name, name) WHEN 'Audio' THEN COALESCE(lpad(parent_index_number::text, greatest(4, length(parent_index_number::text)), '0') || ' - ', '') || COALESCE(lpad(index_number::text, greatest(4, length(index_number::text)), '0') || ' - ', '') || COALESCE(sort_name, name) ELSE sort_name END),
    sort_name,
    name
) WHERE owner_id IS NULL;
