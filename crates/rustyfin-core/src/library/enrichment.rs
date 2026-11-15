use std::path::{Path, PathBuf};

use sqlx::PgPool;
use uuid::Uuid;

use rustyfin_db::repo::{media_items, people};

use rustyfin_common::nfo::{
    NfoMetadata, find_album_nfo, find_artist_nfo, find_episode_nfo, find_movie_nfo,
    find_series_nfo, parse_album_nfo, parse_artist_nfo, parse_episode_nfo, parse_movie_nfo,
    parse_series_nfo,
};

pub struct EnrichmentService {
    pool: PgPool,
    people_cache_dir: Option<PathBuf>,
}

impl EnrichmentService {
    pub fn new(pool: PgPool, people_cache_dir: Option<PathBuf>) -> Self {
        Self {
            pool,
            people_cache_dir,
        }
    }

    /// Enrich a movie item with metadata from NFO.
    pub async fn enrich_movie(
        &self,
        item_id: Uuid,
        video_path: &Path,
        _title: &str,
        _year: Option<i32>,
    ) {
        let meta = find_movie_nfo(video_path).and_then(|p| parse_movie_nfo(&p));
        let meta = match video_path.parent() {
            Some(dir) => with_title_settings(meta, dir),
            None => meta,
        };
        let Some(meta) = meta else { return };
        self.apply_metadata(item_id, &meta).await;
    }

    /// Enrich a series item with metadata from NFO.
    pub async fn enrich_series(&self, item_id: Uuid, series_dir: &Path, _series_name: &str) {
        let meta = find_series_nfo(series_dir).and_then(|p| parse_series_nfo(&p));
        let meta = with_title_settings(meta, series_dir);
        let Some(meta) = meta else { return };
        self.apply_metadata(item_id, &meta).await;
    }

    /// Enrich an episode item with metadata from NFO.
    pub async fn enrich_episode(&self, item_id: Uuid, video_path: &Path) {
        let meta = find_episode_nfo(video_path).and_then(|p| parse_episode_nfo(&p));

        let Some(meta) = meta else { return };
        self.apply_metadata(item_id, &meta).await;
    }

    /// Enrich a music artist item with metadata from NFO.
    pub async fn enrich_artist(&self, item_id: Uuid, artist_dir: &Path) {
        let meta = find_artist_nfo(artist_dir).and_then(|p| parse_artist_nfo(&p));
        let Some(meta) = meta else { return };
        self.apply_metadata(item_id, &meta).await;
    }

    /// Enrich a music album item with metadata from NFO.
    pub async fn enrich_album(&self, item_id: Uuid, album_dir: &Path) {
        let meta = find_album_nfo(album_dir).and_then(|p| parse_album_nfo(&p));
        let Some(meta) = meta else { return };
        self.apply_metadata(item_id, &meta).await;
    }

    /// Re-enrich an item by loading it from DB and dispatching by type.
    pub async fn refresh_item(&self, item_id: Uuid) {
        let item = match media_items::find_by_id(&self.pool, item_id).await {
            Ok(Some(item)) => item,
            _ => return,
        };

        match item.item_type.as_str() {
            "Movie" => {
                if let Some(ref path) = item.path {
                    self.enrich_movie(item_id, Path::new(path), &item.name, item.production_year)
                        .await;
                }
            }
            "Series" => {
                // Series items don't have a path, but we can try to find one from children
                // For now, try the parent dir of any child episode
                if let Ok(children) =
                    media_items::list_by_parent(&self.pool, Some(item_id), &[], 0, 1).await
                    && let Some(child) = children.first()
                    && let Some(ref child_path) = child.path
                    && let Some(series_dir) =
                        Path::new(child_path).parent().and_then(|p| p.parent())
                {
                    self.enrich_series(item_id, series_dir, &item.name).await;
                }
            }
            "Episode" => {
                if let Some(ref path) = item.path {
                    self.enrich_episode(item_id, Path::new(path)).await;
                }
            }
            "MusicArtist" => {
                if let Ok(children) =
                    media_items::list_by_parent(&self.pool, Some(item_id), &[], 0, 1).await
                    && let Some(child) = children.first()
                    && let Some(ref child_path) = child.path
                    && let Some(artist_dir) =
                        Path::new(child_path).parent().and_then(|p| p.parent())
                {
                    self.enrich_artist(item_id, artist_dir).await;
                }
            }
            "MusicAlbum" => {
                if let Ok(children) =
                    media_items::list_by_parent(&self.pool, Some(item_id), &[], 0, 1).await
                    && let Some(child) = children.first()
                    && let Some(ref child_path) = child.path
                    && let Some(album_dir) = Path::new(child_path).parent()
                {
                    self.enrich_album(item_id, album_dir).await;
                }
            }
            _ => {}
        }
    }

    async fn apply_metadata(&self, item_id: Uuid, meta: &NfoMetadata) {
        // Build external_ids JSON
        let external_ids = build_external_ids(meta);

        let sort_name = meta
            .title
            .as_ref()
            .or(meta.sort_name.as_ref())
            .map(|t| rustyfin_common::naming::make_sort_name(t));

        let item_meta = media_items::ItemMetadata {
            name: meta.title.clone(),
            sort_name,
            overview: meta.overview.clone(),
            original_title: meta.original_title.clone(),
            community_rating: meta.community_rating,
            critic_rating: meta.critic_rating,
            official_rating: meta.official_rating.clone(),
            genres: if meta.genres.is_empty() {
                None
            } else {
                Some(meta.genres.clone())
            },
            studios: if meta.studios.is_empty() {
                None
            } else {
                Some(meta.studios.clone())
            },
            tags: None,
            premiere_date: meta.premiere_date,
            end_date: meta.end_date,
            production_year: meta.production_year,
            external_ids,
        };

        if let Err(e) = media_items::update_metadata(&self.pool, item_id, &item_meta).await {
            tracing::error!("Failed to update metadata for {item_id}: {e}");
            return;
        }

        // Handle people: clear and re-link
        if !meta.people.is_empty() {
            if let Err(e) = people::delete_links_by_item(&self.pool, item_id).await {
                tracing::error!("Failed to delete people links for {item_id}: {e}");
                return;
            }

            for person in &meta.people {
                match people::find_or_create(
                    &self.pool,
                    &person.name,
                    &person.person_type,
                    person.image_url.as_deref(),
                    person.provider_person_id.as_deref(),
                )
                .await
                {
                    Ok(person_row) => {
                        if let Err(e) = people::link_to_item(
                            &self.pool,
                            item_id,
                            person_row.id,
                            person.role.as_deref(),
                            person.sort_order,
                        )
                        .await
                        {
                            tracing::error!(
                                "Failed to link person {} to {item_id}: {e}",
                                person.name
                            );
                        }

                        // Insert image row if headshot cache file exists
                        if let Some(ref cache_dir) = self.people_cache_dir
                            && let Some(file) = rustyfin_common::images::headshot_file_name(
                                person_row.provider_person_id.as_deref(),
                                person_row.image_url.as_deref(),
                            )
                        {
                            let headshot_path = cache_dir.join(file);
                            if headshot_path.is_file() {
                                let existing = rustyfin_db::repo::images::find_by_item_and_type(
                                    &self.pool,
                                    person_row.id,
                                    "Primary",
                                    0,
                                )
                                .await;
                                if matches!(existing, Ok(None)) {
                                    let new_image = rustyfin_db::repo::images::NewImage {
                                        entity_id: person_row.id,
                                        image_type: "Primary".to_string(),
                                        path: headshot_path.to_string_lossy().into_owned(),
                                        image_index: 0,
                                    };
                                    if let Err(e) =
                                        rustyfin_db::repo::images::insert(&self.pool, &new_image)
                                            .await
                                    {
                                        tracing::error!(
                                            "Failed to insert headshot image for {}: {e}",
                                            person.name
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to find/create person {}: {e}", person.name);
                    }
                }
            }
        }
    }

    /// Backfill headshot image rows for people who have a cached headshot file
    /// but no Primary image row yet.  Called after every library scan so that
    /// newly-downloaded headshots are picked up without requiring a file-change
    /// triggered re-enrichment.
    pub async fn backfill_people_headshots(&self) {
        let Some(ref cache_dir) = self.people_cache_dir else {
            return;
        };

        let people = match people::list_missing_headshots(&self.pool).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to list people missing headshots: {e}");
                return;
            }
        };

        let mut inserted = 0u32;
        for person in &people {
            let Some(file) = rustyfin_common::images::headshot_file_name(
                person.provider_person_id.as_deref(),
                person.image_url.as_deref(),
            ) else {
                continue;
            };
            let headshot_path = cache_dir.join(file);
            if !headshot_path.is_file() {
                continue;
            }
            let new_image = rustyfin_db::repo::images::NewImage {
                entity_id: person.id,
                image_type: "Primary".to_string(),
                path: headshot_path.to_string_lossy().into_owned(),
                image_index: 0,
            };
            if let Err(e) = rustyfin_db::repo::images::insert(&self.pool, &new_image).await {
                tracing::error!("Failed to insert headshot for {}: {e}", person.name);
            } else {
                inserted += 1;
            }
        }
        if inserted > 0 {
            tracing::info!("Backfilled {inserted} people headshot image(s)");
        }
    }
}

/// `meta` (from the NFO, if any) with the title's `.rustyfin.toml` in `dir`
/// laid over it: its ids and title win.
fn with_title_settings(meta: Option<NfoMetadata>, dir: &Path) -> Option<NfoMetadata> {
    let Some(settings) = rustyfin_common::title_settings::load(dir) else {
        return meta;
    };
    let mut meta = meta.unwrap_or_default();
    if let Some(id) = settings.ids.tmdb {
        meta.tmdb_id = Some(id.to_string());
    }
    if let Some(id) = settings.ids.tvdb {
        meta.tvdb_id = Some(id.to_string());
    }
    if let Some(id) = settings.ids.imdb {
        meta.imdb_id = Some(id);
    }
    if let Some(title) = settings.metadata.title {
        meta.title = Some(title);
    }
    Some(meta)
}

fn build_external_ids(meta: &NfoMetadata) -> Option<serde_json::Value> {
    let mut ids = serde_json::Map::new();
    if let Some(ref id) = meta.tmdb_id {
        ids.insert("Tmdb".to_string(), serde_json::Value::String(id.clone()));
    }
    if let Some(ref id) = meta.tvdb_id {
        ids.insert("Tvdb".to_string(), serde_json::Value::String(id.clone()));
    }
    if let Some(ref id) = meta.imdb_id {
        ids.insert("Imdb".to_string(), serde_json::Value::String(id.clone()));
    }
    if let Some(ref id) = meta.musicbrainz_artist_id {
        ids.insert(
            "MusicBrainzArtist".to_string(),
            serde_json::Value::String(id.clone()),
        );
    }
    if let Some(ref id) = meta.musicbrainz_album_id {
        ids.insert(
            "MusicBrainzAlbum".to_string(),
            serde_json::Value::String(id.clone()),
        );
    }
    if let Some(ref id) = meta.musicbrainz_release_group_id {
        ids.insert(
            "MusicBrainzReleaseGroup".to_string(),
            serde_json::Value::String(id.clone()),
        );
    }
    if ids.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(ids))
    }
}
