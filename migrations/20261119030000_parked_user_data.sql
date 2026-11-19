-- What users had of a removed link's titles (owner, 2026-11-19: "Keep watched
-- history"). Kept by provider ids, since the rows go with the link, and given
-- back when the same title turns up again (on this server or through another
-- link): see links::restore_parked.
CREATE TABLE IF NOT EXISTS parked_user_data (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Movie, Series or Episode.
    item_type TEXT NOT NULL,
    -- The title's provider ids; an episode's are its series'.
    external_ids JSONB NOT NULL DEFAULT '{}',
    season_number INT,
    episode_number INT,
    name TEXT NOT NULL,
    playback_position_ticks BIGINT NOT NULL DEFAULT 0,
    play_count INT NOT NULL DEFAULT 0,
    is_favorite BOOLEAN NOT NULL DEFAULT false,
    played BOOLEAN NOT NULL DEFAULT false,
    rating DOUBLE PRECISION,
    last_played_date TIMESTAMPTZ,
    audio_stream_index INT,
    subtitle_stream_index INT,
    parked_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_parked_user_data_user ON parked_user_data (user_id);
