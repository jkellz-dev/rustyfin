# Database Schema

Rustyfin uses PostgreSQL 14+. The schema is created automatically via migrations at server startup.

## Entity Relationship Diagram

```text
users ──────────< sessions
  │
  │ ──────────< user_data >────────── media_items
  │                                       │
  │                                       ├──< media_streams
  │                                       ├──< images
  │                                       ├──< chapters
  │                                       └──< media_item_people >── people
  │
  └──────────< activity_log

libraries ──────< media_items (via library_id)
media_items ────< media_items (via parent_id, self-referencing hierarchy)
```

## Tables

### `users`

Stores user accounts. Users are auto-provisioned via forward auth headers.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | No | `gen_random_uuid()` | Primary key |
| `username` | VARCHAR(255) | No | | Unique username |
| `display_name` | TEXT | Yes | | Human-readable name |
| `email` | TEXT | Yes | | Email address |
| `is_administrator` | BOOLEAN | No | `false` | Admin privileges |
| `remote_user_id` | TEXT | Yes | | Forward auth identity (unique) |
| `max_parental_rating` | INT | Yes | | Parental rating limit |
| `audio_language_preference` | VARCHAR(255) | Yes | | Preferred audio language |
| `subtitle_language_preference` | VARCHAR(255) | Yes | | Preferred subtitle language |
| `subtitle_mode` | INT | No | `0` | Jellyfin's `SubtitlePlaybackMode`: 0 Default, 1 Always, 2 OnlyForced, 3 None, 4 Smart |
| `play_default_audio_track` | BOOLEAN | No | `true` | Play default audio track |
| `enable_next_episode_auto_play` | BOOLEAN | No | `true` | Auto-play next episode |
| `last_login_date` | TIMESTAMPTZ | Yes | | Last login timestamp |
| `last_activity_date` | TIMESTAMPTZ | Yes | | Last API activity |
| `created_at` | TIMESTAMPTZ | No | `NOW()` | Row creation time |
| `updated_at` | TIMESTAMPTZ | No | `NOW()` | Last update time |
| `configuration` | JSONB | Yes | | The rest of the app's `UserConfiguration` (ordered views, excludes, ...), as sent |
| `may_link` | BOOLEAN | No | `false` | The admin lets this user link a server to this one (`/api/v1/catalog`) |

**Unique constraints**: `username`, `remote_user_id`

### `sessions`

Active authentication sessions.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | No | `gen_random_uuid()` | Primary key |
| `access_token` | TEXT | No | | Unique session token |
| `user_id` | UUID | No | | FK to `users.id` |
| `device_id` | TEXT | No | | Client device identifier |
| `device_name` | TEXT | No | | Client device name |
| `app_name` | TEXT | No | | Client application name |
| `app_version` | TEXT | Yes | | Client app version |
| `is_active` | BOOLEAN | No | `true` | Session active flag |
| `created_at` | TIMESTAMPTZ | No | `NOW()` | Session creation time |
| `last_activity` | TIMESTAMPTZ | No | `NOW()` | Last activity timestamp |
| `expires_at` | TIMESTAMPTZ | Yes | | Expiry (null = no expiry) |

**Indexes**: `idx_sessions_token` (on `access_token`), `idx_sessions_user` (on `user_id`)

### `libraries`

Media library definitions.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | No | `gen_random_uuid()` | Primary key |
| `name` | TEXT | No | | Library display name |
| `library_type` | TEXT | No | | Type: `movies`, `tvshows`, `music`, `mixed` |
| `paths` | TEXT[] | No | | Array of filesystem paths |
| `configured` | BOOLEAN | No | `false` | Set in `rustyfin.toml` (`[[library]]`); changed only there |
| `created_at` | TIMESTAMPTZ | No | `NOW()` | Creation time |
| `updated_at` | TIMESTAMPTZ | No | `NOW()` | Last scan/update time |

### `media_items`

Central media catalog. Self-referencing hierarchy: Library → Series → Season → Episode.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | No | `gen_random_uuid()` | Primary key |
| `item_type` | TEXT | No | | `Movie`, `Series`, `Season`, `Episode`, `CollectionFolder` |
| `library_id` | UUID | Yes | | FK to `libraries.id` (CASCADE) |
| `parent_id` | UUID | Yes | | FK to `media_items.id` (CASCADE) |
| `name` | TEXT | No | | Display name |
| `sort_name` | TEXT | Yes | | Lowercase name without articles |
| `original_title` | TEXT | Yes | | Original language title |
| `overview` | TEXT | Yes | | Plot synopsis |
| `path` | TEXT | Yes | | Filesystem path |
| `community_rating` | REAL | Yes | | User rating (0-10) |
| `critic_rating` | REAL | Yes | | Critic rating (0-100) |
| `official_rating` | TEXT | Yes | | Content rating (PG-13, R, etc.) |
| `genres` | TEXT[] | Yes | | Genre array |
| `studios` | TEXT[] | Yes | | Studio array |
| `tags` | TEXT[] | Yes | | Tag array |
| `production_year` | INT | Yes | | Release year |
| `premiere_date` | TIMESTAMPTZ | Yes | | Release date |
| `end_date` | TIMESTAMPTZ | Yes | | Series end date |
| `runtime_ticks` | BIGINT | Yes | | Duration in 100ns ticks |
| `index_number` | INT | Yes | | Episode/season number |
| `parent_index_number` | INT | Yes | | Season number for episodes |
| `is_folder` | BOOLEAN | No | `false` | True for series, seasons |
| `date_created` | TIMESTAMPTZ | No | `NOW()` | When added to library |
| `date_modified` | TIMESTAMPTZ | Yes | | Last metadata update |
| `date_last_media_added` | TIMESTAMPTZ | Yes | | Last child added (folders) |
| `series_id` | UUID | Yes | | FK to parent series |
| `season_id` | UUID | Yes | | FK to parent season |
| `series_name` | TEXT | Yes | | Denormalized series name |
| `season_name` | TEXT | Yes | | Denormalized season name |
| `container` | TEXT | Yes | | File container (mkv, mp4) |
| `total_bitrate` | INT | Yes | | Total bitrate |
| `width` | INT | Yes | | Video width |
| `height` | INT | Yes | | Video height |
| `size` | BIGINT | Yes | | File size in bytes |
| `etag` | TEXT | Yes | | Content hash |
| `presentation_unique_key` | TEXT | Yes | | Unique key for deduplication |
| `external_ids` | JSONB | Yes | | TMDB/TVDB/IMDb IDs |
| `extra_data` | JSONB | Yes | | Additional metadata (a linked copy: `LinkName`, `LinkSource`, `RemoteSourceId`) |
| `link_id` | UUID | Yes | | A linked server's copy: FK to `links.id` (CASCADE); such rows have no `path` |
| `remote_id` | TEXT | Yes | | The copy's item id on the linked server (`<item>/<source>` for its other versions); unique per link |
| `remote_etag` | TEXT | Yes | | The linked server's `Etag` when last synced; NULL until fully stored |
| `change_xid` | XID8 | No | `pg_current_xact_id()` | The transaction that last changed the item, its streams or its images (set by triggers); the change feed's position |

**Indexes**: `idx_media_items_type`, `idx_media_items_parent`, `idx_media_items_library`, `idx_media_items_series`,
`idx_media_items_path`, `idx_media_items_name`, `idx_media_items_presentation_key`

### `links`

Linked servers (`[[link]]` in the config file; [Linked Servers](linked-servers.md)). Their copies are `media_items`
rows with `link_id`; a copy of a film the server holds is a `Version` of it (`owner_id`).

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | No | `gen_random_uuid()` | Primary key |
| `name` | TEXT | No | | The link's name from the config (unique) |
| `kind` | TEXT | No | | `jellyfin` or `rustyfin` |
| `url` | TEXT | No | | The friend's server |
| `last_sync_at` | TIMESTAMPTZ | Yes | | When the last sync that worked ended |
| `last_error` | TEXT | Yes | | Why the last sync failed; NULL after one that worked |
| `offline_since` | TIMESTAMPTZ | Yes | | Since when its server is down; its titles are hidden from the views (`visible_media_items`) |
| `created_at` | TIMESTAMPTZ | No | `now()` | |

### `visible_media_items` (view)

`media_items` without the rows held from a link that is down (`links.offline_since`), and without any row held from a
link on connections that set `rustyfin.local_only` (those serving a server that links this one; `link_scope`). Every
view of the library reads it; scans, syncs and merges read `media_items`. A migration that adds a column to
`media_items` must re-create the view.

### `media_removals`

Items removed, for the change feed (`/api/v1/catalog`, which leaves out linked copies) and the search index; written
by a trigger on `media_items`.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `item_id` | UUID | No | | The removed item |
| `library_id` | UUID | Yes | | Its library, so a feed shows only removals the user could see |
| `removed_xid` | XID8 | No | `pg_current_xact_id()` | The transaction that removed it |
| `removed_at` | TIMESTAMPTZ | No | `NOW()` | |
| `link_id` | UUID | Yes | | The link it was a copy from, if any |

### `people_removals`

People removed, for the search index; written by a trigger on `people`. `people.change_xid` stamps the transaction that
last changed a person, as on `media_items`.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `person_id` | UUID | No | | The removed person |
| `removed_xid` | XID8 | No | `pg_current_xact_id()` | The transaction that removed them |
| `removed_at` | TIMESTAMPTZ | No | `now()` | |

### `search_sync`

Where each Meilisearch index was brought up to (`rustyfin_data::repo::search_feed`): the changes since are those this
snapshot did not see. No row: the next sync builds the index whole.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `index_uid` | TEXT | No | | The index (`media_items`, `media_items_people`) |
| `cursor` | TEXT | No | | A `pg_snapshot` in text form |
| `updated_at` | TIMESTAMPTZ | No | `now()` | |

### `parked_user_data`

What users had of a removed link's titles (watched, resume point, favourite, rating), kept by provider ids and given
back (`links::restore_parked`, after every merge) when the same title turns up again; the row goes once given back.
An episode is kept by its show's ids and its season and episode numbers.

### `media_streams`

Video, audio, and subtitle tracks within a media item.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | No | `gen_random_uuid()` | Primary key |
| `item_id` | UUID | No | | FK to `media_items.id` (CASCADE) |
| `stream_index` | INT | No | | Track index within file |
| `stream_type` | TEXT | No | | `Video`, `Audio`, `Subtitle` |
| `codec` | TEXT | Yes | | Codec name (hevc, aac, srt) |
| `language` | TEXT | Yes | | ISO 639 language code |
| `title` | TEXT | Yes | | Track title |
| `is_default` | BOOLEAN | No | `false` | Default track flag |
| `is_forced` | BOOLEAN | No | `false` | Forced subtitle flag |
| `is_external` | BOOLEAN | No | `false` | External file flag |
| `width` | INT | Yes | | Video width |
| `height` | INT | Yes | | Video height |
| `bit_rate` | INT | Yes | | Track bitrate |
| `bit_depth` | INT | Yes | | Video bit depth |
| `pixel_format` | TEXT | Yes | | Pixel format (yuv420p) |
| `aspect_ratio` | TEXT | Yes | | Display aspect ratio |
| `real_frame_rate` | REAL | Yes | | Frame rate |
| `color_space` | TEXT | Yes | | Color space (bt709, bt2020nc) |
| `color_primaries` | TEXT | Yes | | Color primaries |
| `color_transfer` | TEXT | Yes | | Transfer characteristics |
| `channels` | INT | Yes | | Audio channel count |
| `channel_layout` | TEXT | Yes | | Channel layout (5.1, stereo) |
| `sample_rate` | INT | Yes | | Audio sample rate |
| `path` | TEXT | Yes | | External subtitle file path |

**Unique constraint**: `(item_id, stream_index)`

### `user_data`

Per-user playback state and preferences for each media item.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `user_id` | UUID | No | | FK to `users.id` (CASCADE) |
| `item_id` | UUID | No | | FK to `media_items.id` (CASCADE) |
| `playback_position_ticks` | BIGINT | No | `0` | Resume position (100ns ticks) |
| `play_count` | INT | No | `0` | Number of complete plays |
| `is_favorite` | BOOLEAN | No | `false` | Favorited flag |
| `played` | BOOLEAN | No | `false` | Fully played flag |
| `rating` | DOUBLE PRECISION | Yes | | User rating |
| `last_played_date` | TIMESTAMPTZ | Yes | | Last playback timestamp |
| `audio_stream_index` | INT | Yes | | Preferred audio track |
| `subtitle_stream_index` | INT | Yes | | Preferred subtitle track |

**Primary key**: `(user_id, item_id)`
**Indexes**: `idx_user_data_favorites` (WHERE `is_favorite = true`), `idx_user_data_played`

### `people`

Cast and crew members.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | No | `gen_random_uuid()` | Primary key |
| `name` | TEXT | No | | Person name |
| `sort_name` | TEXT | Yes | | Sortable name |
| `person_type` | TEXT | No | | `Actor`, `Director`, `Writer`, etc. |
| `overview` | TEXT | Yes | | Biography |
| `external_ids` | JSONB | Yes | | TMDB/IMDb IDs |

### `media_item_people`

Junction table linking media items to people.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `item_id` | UUID | No | | FK to `media_items.id` (CASCADE) |
| `person_id` | UUID | No | | FK to `people.id` (CASCADE) |
| `role` | TEXT | Yes | | Character name (for actors) |
| `sort_order` | INT | Yes | | Display order |

**Primary key**: `(item_id, person_id)`

### `images`

Image files associated with items or users.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | No | `gen_random_uuid()` | Primary key |
| `item_id` | UUID | Yes | | FK to `media_items.id` (CASCADE) |
| `user_id` | UUID | Yes | | FK to `users.id` (CASCADE) |
| `image_type` | TEXT | No | | `Primary`, `Backdrop`, `Banner`, `Logo`, `Thumb`, `Art`, `Disc` |
| `path` | TEXT | No | | Filesystem path to image |
| `blurhash` | TEXT | Yes | | Blurhash placeholder string |
| `width` | INT | Yes | | Image width in pixels |
| `height` | INT | Yes | | Image height in pixels |
| `image_index` | INT | No | `0` | Index for multiple images of same type |

**Check constraint**: `item_id IS NOT NULL OR user_id IS NOT NULL`

### `chapters`

Chapter markers within a media item.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | No | `gen_random_uuid()` | Primary key |
| `item_id` | UUID | No | | FK to `media_items.id` (CASCADE) |
| `chapter_index` | INT | No | | Chapter sequence number |
| `start_position_ticks` | BIGINT | No | | Start time in 100ns ticks |
| `name` | TEXT | Yes | | Chapter name |
| `image_path` | TEXT | Yes | | Chapter thumbnail path |

**Unique constraint**: `(item_id, chapter_index)`

### `device_auth_requests`

RFC 8628 Device Authorization Grant flow. Unused: the code that wrote it was removed, as signing in is the proxy's job;
the table is kept so no migration drops data.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | UUID | No | | Primary key |
| `device_code` | TEXT | No | | Unique device code |
| `user_code` | TEXT | No | | Unique user-facing code |
| `client_info` | JSONB | No | | Client device info |
| `scope` | TEXT | Yes | | Requested scope |
| `status` | TEXT | No | `'pending'` | `pending`, `approved`, `denied` |
| `user_id` | UUID | Yes | | FK to `users.id` (set on approval) |
| `created_at` | TIMESTAMPTZ | No | `NOW()` | Creation time |
| `expires_at` | TIMESTAMPTZ | No | | Expiry time |
| `poll_interval` | INT | No | `5` | Poll interval in seconds |

### `activity_log`

Server activity audit trail.

| Column | Type | Nullable | Default | Description |
|--------|------|----------|---------|-------------|
| `id` | BIGSERIAL | No | | Primary key |
| `name` | TEXT | No | | Activity name |
| `overview` | TEXT | Yes | | Detailed description |
| `log_type` | TEXT | No | | Activity category |
| `severity` | TEXT | No | `'Info'` | Log level |
| `user_id` | UUID | Yes | | FK to `users.id` (SET NULL) |
| `item_id` | UUID | Yes | | Related media item |
| `created_at` | TIMESTAMPTZ | No | `NOW()` | Event timestamp |

## Cascade Behavior

- Deleting a **library** cascades to all its `media_items`
- Deleting a **link** cascades to its copies (`media_items.link_id`)
- Deleting a **media_item** cascades to its `media_streams`, `images`, `chapters`, `user_data`, `media_item_people`, and
  child `media_items`
- Deleting a **user** cascades to their `sessions`, `user_data`, and `images`
- Deleting a **user** sets `activity_log.user_id` to NULL

## Duration Convention

All duration/position fields (`runtime_ticks`, `playback_position_ticks`, `start_position_ticks`) are stored in
**100-nanosecond ticks**, matching the Jellyfin convention:

```text
seconds * 10,000,000 = ticks
ticks / 10,000,000 = seconds

Example: 2 hours = 7200 seconds = 72,000,000,000 ticks
```
