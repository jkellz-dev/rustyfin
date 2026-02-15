-- Playlists (user-owned, ordered)
CREATE TABLE playlists (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_type TEXT NOT NULL DEFAULT 'Video',
    open_access BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_playlists_owner ON playlists(owner_user_id);

CREATE TABLE playlist_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    playlist_id UUID NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    sort_order INT NOT NULL
);

CREATE INDEX idx_playlist_items_playlist ON playlist_items(playlist_id, sort_order);

CREATE TABLE playlist_shares (
    playlist_id UUID NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    can_edit BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (playlist_id, user_id)
);

-- Collections (stored as media_items with Type='BoxSet', items linked via junction)
CREATE TABLE collection_items (
    collection_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, item_id)
);

CREATE INDEX idx_collection_items_collection ON collection_items(collection_id);
