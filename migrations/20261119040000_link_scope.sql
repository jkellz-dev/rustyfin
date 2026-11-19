-- A server linking this one sees only this server's own media (owner,
-- 2026-11-19: "Only users who are directly using my instance of rustyfin
-- should see linked libraries"). Its requests run on connections that set
-- rustyfin.local_only (rustyfin_api::link_scope), and the view leaves out
-- every row held from a link there; elsewhere, only a down link's rows.
--
-- The view's columns are media_items' as they are now: a migration that adds
-- a column to media_items must run this CREATE OR REPLACE VIEW again (a test
-- compares the two).
CREATE OR REPLACE VIEW visible_media_items AS
    SELECT * FROM media_items
    WHERE link_id IS NULL
       OR (current_setting('rustyfin.local_only', true) IS DISTINCT FROM 'on'
           AND link_id <> ALL (ARRAY(SELECT id FROM links WHERE offline_since IS NOT NULL)));
