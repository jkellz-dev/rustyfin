-- Rustyfin initial schema
-- Users are provisioned via forward auth (reverse proxy). No local passwords.

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username VARCHAR(255) NOT NULL UNIQUE,
    display_name TEXT,
    email TEXT,
    is_administrator BOOLEAN NOT NULL DEFAULT false,
    -- Forward auth identifier (e.g. from X-Forwarded-User or Remote-User header)
    remote_user_id TEXT UNIQUE,
    -- Preferences
    max_parental_rating INT,
    audio_language_preference VARCHAR(255),
    subtitle_language_preference VARCHAR(255),
    subtitle_mode INT NOT NULL DEFAULT 0,
    play_default_audio_track BOOLEAN NOT NULL DEFAULT true,
    enable_next_episode_auto_play BOOLEAN NOT NULL DEFAULT true,
    -- Timestamps
    last_login_date TIMESTAMPTZ,
    last_activity_date TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Sessions / access tokens
CREATE TABLE sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    access_token TEXT NOT NULL UNIQUE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL,
    device_name TEXT NOT NULL,
    app_name TEXT NOT NULL,
    app_version TEXT,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_activity TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ
);

CREATE INDEX idx_sessions_token ON sessions(access_token);
CREATE INDEX idx_sessions_user ON sessions(user_id);

-- Device Authorization Grant (RFC 8628) for TV/limited-input clients
CREATE TABLE device_auth_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_code TEXT NOT NULL UNIQUE,
    user_code TEXT NOT NULL UNIQUE,
    client_info JSONB NOT NULL,
    scope TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    user_id UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    poll_interval INT NOT NULL DEFAULT 5
);

-- Libraries
CREATE TABLE libraries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    library_type TEXT NOT NULL,
    paths TEXT[] NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Media items (core content)
CREATE TABLE media_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    item_type TEXT NOT NULL,
    library_id UUID REFERENCES libraries(id) ON DELETE CASCADE,
    parent_id UUID REFERENCES media_items(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    sort_name TEXT,
    original_title TEXT,
    overview TEXT,
    path TEXT,
    community_rating REAL,
    critic_rating REAL,
    official_rating TEXT,
    genres TEXT[],
    studios TEXT[],
    tags TEXT[],
    production_year INT,
    premiere_date TIMESTAMPTZ,
    end_date TIMESTAMPTZ,
    runtime_ticks BIGINT,
    index_number INT,
    parent_index_number INT,
    is_folder BOOLEAN NOT NULL DEFAULT false,
    date_created TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    date_modified TIMESTAMPTZ,
    date_last_media_added TIMESTAMPTZ,
    series_id UUID REFERENCES media_items(id),
    season_id UUID REFERENCES media_items(id),
    series_name TEXT,
    season_name TEXT,
    container TEXT,
    total_bitrate INT,
    width INT,
    height INT,
    size BIGINT,
    etag TEXT,
    presentation_unique_key TEXT,
    external_ids JSONB DEFAULT '{}',
    extra_data JSONB DEFAULT '{}'
);

CREATE INDEX idx_media_items_type ON media_items(item_type);
CREATE INDEX idx_media_items_parent ON media_items(parent_id);
CREATE INDEX idx_media_items_library ON media_items(library_id);
CREATE INDEX idx_media_items_series ON media_items(series_id);
CREATE INDEX idx_media_items_path ON media_items(path);
CREATE INDEX idx_media_items_name ON media_items(name);
CREATE INDEX idx_media_items_presentation_key ON media_items(presentation_unique_key);

-- Media streams (video, audio, subtitle tracks per item)
CREATE TABLE media_streams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    stream_index INT NOT NULL,
    stream_type TEXT NOT NULL,
    codec TEXT,
    language TEXT,
    title TEXT,
    is_default BOOLEAN NOT NULL DEFAULT false,
    is_forced BOOLEAN NOT NULL DEFAULT false,
    is_external BOOLEAN NOT NULL DEFAULT false,
    width INT,
    height INT,
    bit_rate INT,
    bit_depth INT,
    pixel_format TEXT,
    aspect_ratio TEXT,
    real_frame_rate REAL,
    color_space TEXT,
    color_primaries TEXT,
    color_transfer TEXT,
    channels INT,
    channel_layout TEXT,
    sample_rate INT,
    path TEXT,
    UNIQUE(item_id, stream_index)
);

-- User data (per-user item state: played, favorite, progress)
CREATE TABLE user_data (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    playback_position_ticks BIGINT NOT NULL DEFAULT 0,
    play_count INT NOT NULL DEFAULT 0,
    is_favorite BOOLEAN NOT NULL DEFAULT false,
    played BOOLEAN NOT NULL DEFAULT false,
    rating DOUBLE PRECISION,
    last_played_date TIMESTAMPTZ,
    audio_stream_index INT,
    subtitle_stream_index INT,
    PRIMARY KEY (user_id, item_id)
);

CREATE INDEX idx_user_data_favorites ON user_data(user_id, is_favorite) WHERE is_favorite = true;
CREATE INDEX idx_user_data_played ON user_data(user_id, played);

-- Chapters
CREATE TABLE chapters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    chapter_index INT NOT NULL,
    start_position_ticks BIGINT NOT NULL,
    name TEXT,
    image_path TEXT,
    UNIQUE(item_id, chapter_index)
);

-- People (actors, directors, etc.)
CREATE TABLE people (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    sort_name TEXT,
    person_type TEXT NOT NULL,
    overview TEXT,
    external_ids JSONB DEFAULT '{}'
);

CREATE TABLE media_item_people (
    item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    person_id UUID NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    role TEXT,
    sort_order INT,
    PRIMARY KEY (item_id, person_id)
);

-- Images
CREATE TABLE images (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    item_id UUID REFERENCES media_items(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    image_type TEXT NOT NULL,
    path TEXT NOT NULL,
    blurhash TEXT,
    width INT,
    height INT,
    image_index INT NOT NULL DEFAULT 0,
    CHECK (item_id IS NOT NULL OR user_id IS NOT NULL)
);

-- Activity log
CREATE TABLE activity_log (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    overview TEXT,
    log_type TEXT NOT NULL,
    severity TEXT NOT NULL DEFAULT 'Info',
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    item_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
