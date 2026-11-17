-- What each user played, and for how long (owner, 2026-11-17: a user's page
-- with minutes per kind of media and a filterable list of plays). One row per
-- play, built from the playback reports clients send; the item's name and kind
-- are kept so the list survives the item being deleted.
CREATE TABLE IF NOT EXISTS play_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id UUID REFERENCES media_items(id) ON DELETE SET NULL,
    item_type TEXT NOT NULL,
    item_name TEXT NOT NULL,
    -- The show of an episode, the artists of a track.
    parent_name TEXT,
    -- The client's play session id, else "<session>:<item>".
    play_key TEXT NOT NULL,
    client TEXT,
    device_name TEXT,
    play_method TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    played_ms BIGINT NOT NULL DEFAULT 0,
    paused BOOLEAN NOT NULL DEFAULT false,
    ended BOOLEAN NOT NULL DEFAULT false,
    position_ticks BIGINT
);

CREATE INDEX IF NOT EXISTS play_history_user_started
    ON play_history (user_id, started_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS play_history_open
    ON play_history (user_id, play_key) WHERE NOT ended;
