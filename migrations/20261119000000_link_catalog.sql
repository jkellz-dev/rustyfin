-- Rustyfin-to-Rustyfin links (roadmap 16b.2r, owner 2026-11-18): a linking
-- server is an ordinary user the admin lets link, and it syncs from a change
-- feed instead of re-listing every item.

-- The admin's permission; off by default.
ALTER TABLE users ADD COLUMN IF NOT EXISTS may_link BOOLEAN NOT NULL DEFAULT false;

-- The transaction that last changed each item. The feed's cursor is a
-- snapshot: what changed since is every item whose change that snapshot did
-- not see, so a long transaction committing late is never missed.
ALTER TABLE media_items
    ADD COLUMN IF NOT EXISTS change_xid xid8 NOT NULL DEFAULT pg_current_xact_id();
CREATE INDEX IF NOT EXISTS idx_media_items_change ON media_items (change_xid, id)
    WHERE link_id IS NULL;

CREATE OR REPLACE FUNCTION media_items_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.change_xid := pg_current_xact_id();
    RETURN NEW;
END
$$;
DROP TRIGGER IF EXISTS media_items_changed ON media_items;
CREATE TRIGGER media_items_changed BEFORE UPDATE ON media_items
    FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*) EXECUTE FUNCTION media_items_changed();

-- A new or changed stream or image changes its item (what a linked copy is
-- ranked and shown by).
CREATE OR REPLACE FUNCTION media_item_part_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    UPDATE media_items SET change_xid = pg_current_xact_id()
     WHERE id = NEW.item_id AND change_xid <> pg_current_xact_id();
    RETURN NULL;
END
$$;
DROP TRIGGER IF EXISTS media_streams_changed ON media_streams;
CREATE TRIGGER media_streams_changed AFTER INSERT OR UPDATE ON media_streams
    FOR EACH ROW EXECUTE FUNCTION media_item_part_changed();
-- Images belong to an entity (an item, person, studio...): only an item's count.
CREATE OR REPLACE FUNCTION media_item_image_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    UPDATE media_items SET change_xid = pg_current_xact_id()
     WHERE id = NEW.entity_id AND change_xid <> pg_current_xact_id();
    RETURN NULL;
END
$$;
DROP TRIGGER IF EXISTS images_changed ON images;
CREATE TRIGGER images_changed AFTER INSERT OR UPDATE ON images
    FOR EACH ROW WHEN (NEW.entity_id IS NOT NULL) EXECUTE FUNCTION media_item_image_changed();

-- Items removed, for the feed's removals. Copies held from other links are
-- never offered, so their removal is not recorded either.
CREATE TABLE IF NOT EXISTS media_removals (
    item_id UUID NOT NULL,
    library_id UUID,
    removed_xid xid8 NOT NULL DEFAULT pg_current_xact_id(),
    removed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_media_removals_xid ON media_removals (removed_xid);

CREATE OR REPLACE FUNCTION media_items_removed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO media_removals (item_id, library_id) VALUES (OLD.id, OLD.library_id);
    RETURN NULL;
END
$$;
DROP TRIGGER IF EXISTS media_items_removed ON media_items;
CREATE TRIGGER media_items_removed AFTER DELETE ON media_items
    FOR EACH ROW WHEN (OLD.link_id IS NULL) EXECUTE FUNCTION media_items_removed();
