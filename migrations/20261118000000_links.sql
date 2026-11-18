-- Linked servers (roadmap 16b, owner 2026-11-18): a friend's Jellyfin or
-- Rustyfin shares libraries with this server, and their films merge into the
-- local libraries. One row per link named in the config file.
CREATE TABLE IF NOT EXISTS links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    url TEXT NOT NULL,
    last_sync_at TIMESTAMPTZ,
    -- Why the last sync failed; NULL after one that worked.
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A linked copy is an ordinary item with no path: its link, its id on the
-- friend's server and the tag that changes when the friend's copy does. A
-- copy of a film this server already holds is a 'Version' of that film
-- (owner_id), so lists show the film once.
ALTER TABLE media_items
    ADD COLUMN IF NOT EXISTS link_id UUID REFERENCES links (id) ON DELETE CASCADE,
    ADD COLUMN IF NOT EXISTS remote_id TEXT,
    ADD COLUMN IF NOT EXISTS remote_etag TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_media_items_link_remote
    ON media_items (link_id, remote_id) WHERE link_id IS NOT NULL;
