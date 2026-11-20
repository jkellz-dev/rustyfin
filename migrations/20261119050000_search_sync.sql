-- The search index keeps itself in step from the database's change stamps
-- (owner, 2026-11-19: Meilisearch ranks, Postgres filters). search_sync holds
-- the snapshot each index was brought up to (rustyfin_db::repo::search_feed).
CREATE TABLE IF NOT EXISTS search_sync (
    index_uid TEXT PRIMARY KEY,
    cursor TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Removals of linked copies are recorded too, for the index; the link change
-- feed still leaves them out (catalog::changes, link_id IS NULL).
ALTER TABLE media_removals ADD COLUMN IF NOT EXISTS link_id UUID;

CREATE OR REPLACE FUNCTION media_items_removed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO media_removals (item_id, library_id, link_id)
    VALUES (OLD.id, OLD.library_id, OLD.link_id);
    RETURN NULL;
END
$$;
DROP TRIGGER IF EXISTS media_items_removed ON media_items;
CREATE TRIGGER media_items_removed AFTER DELETE ON media_items
    FOR EACH ROW EXECUTE FUNCTION media_items_removed();
