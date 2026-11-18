-- Linked copies stored before every Jellyfin version became a copy of its own
-- (2026-11-18) hold only a film's first version, and their tags are current,
-- so a sync would never fetch the rest. Forget the tags: the next sync fetches
-- every linked film's details once more. Nothing is deleted.
UPDATE media_items SET remote_etag = NULL WHERE link_id IS NOT NULL;
