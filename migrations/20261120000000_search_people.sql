-- People in the search index too (owner, 2026-11-20: Meilisearch for people
-- and artists). Like media_items, a person records the transaction that last
-- changed it, and removals are kept, so the index follows by change stamps
-- (rustyfin_db::repo::search_feed).
ALTER TABLE people
    ADD COLUMN IF NOT EXISTS change_xid xid8 NOT NULL DEFAULT pg_current_xact_id();
CREATE INDEX IF NOT EXISTS idx_people_change ON people (change_xid, id);

CREATE OR REPLACE FUNCTION people_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.change_xid := pg_current_xact_id();
    RETURN NEW;
END
$$;
DROP TRIGGER IF EXISTS people_changed ON people;
CREATE TRIGGER people_changed BEFORE UPDATE ON people
    FOR EACH ROW WHEN (OLD.* IS DISTINCT FROM NEW.*) EXECUTE FUNCTION people_changed();

CREATE TABLE IF NOT EXISTS people_removals (
    person_id UUID NOT NULL,
    removed_xid xid8 NOT NULL DEFAULT pg_current_xact_id(),
    removed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_people_removals_xid ON people_removals (removed_xid);

CREATE OR REPLACE FUNCTION people_removed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO people_removals (person_id) VALUES (OLD.id);
    RETURN NULL;
END
$$;
DROP TRIGGER IF EXISTS people_removed ON people;
CREATE TRIGGER people_removed AFTER DELETE ON people
    FOR EACH ROW EXECUTE FUNCTION people_removed();
