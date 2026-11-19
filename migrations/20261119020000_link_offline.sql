-- A friend's server that is down (owner, 2026-11-19): "Their titles should be
-- omitted from the views." links.offline_since is set while it is down.
ALTER TABLE links ADD COLUMN IF NOT EXISTS offline_since TIMESTAMPTZ;

-- What the views read: every item but those held from a link that is down.
-- Its rows stay as they are (ids, watched state, playlists) and come back
-- when it does. The offline list is one uncorrelated subquery, so it is read
-- once per statement.
--
-- The view's columns are media_items' as they are now: a migration that adds
-- a column to media_items must run this CREATE OR REPLACE VIEW again (a test
-- compares the two).
CREATE OR REPLACE VIEW visible_media_items AS
    SELECT * FROM media_items
    WHERE link_id IS NULL
       OR link_id <> ALL (ARRAY(SELECT id FROM links WHERE offline_since IS NOT NULL));
