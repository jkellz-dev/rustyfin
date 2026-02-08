use std::path::Path;

use sqlx::PgPool;
use uuid::Uuid;

use rustyfin_db::repo::{media_items, people};

use super::nfo::{
    NfoMetadata, find_episode_nfo, find_movie_nfo, find_series_nfo, parse_episode_nfo,
    parse_movie_nfo, parse_series_nfo,
};
use super::tmdb::TmdbClient;

pub struct EnrichmentService {
    pool: PgPool,
    tmdb: Option<TmdbClient>,
}

impl EnrichmentService {
    pub fn new(pool: PgPool, tmdb_api_key: Option<String>) -> Self {
        let tmdb = tmdb_api_key.filter(|k| !k.is_empty()).map(TmdbClient::new);
        Self { pool, tmdb }
    }

    /// Enrich a movie item with metadata from NFO or TMDB.
    pub async fn enrich_movie(
        &self,
        item_id: Uuid,
        video_path: &Path,
        title: &str,
        year: Option<i32>,
    ) {
        // Try NFO first
        let meta = find_movie_nfo(video_path).and_then(|p| parse_movie_nfo(&p));

        let meta = match meta {
            Some(m) => m,
            None => {
                // Fall back to TMDB
                match &self.tmdb {
                    Some(client) => match client.search_movie(title, year).await {
                        Some(m) => m,
                        None => return,
                    },
                    None => return,
                }
            }
        };

        self.apply_metadata(item_id, &meta).await;
    }

    /// Enrich a series item with metadata from NFO or TMDB.
    pub async fn enrich_series(&self, item_id: Uuid, series_dir: &Path, series_name: &str) {
        let meta = find_series_nfo(series_dir).and_then(|p| parse_series_nfo(&p));

        let meta = match meta {
            Some(m) => m,
            None => match &self.tmdb {
                Some(client) => match client.search_series(series_name).await {
                    Some(m) => m,
                    None => return,
                },
                None => return,
            },
        };

        self.apply_metadata(item_id, &meta).await;
    }

    /// Enrich an episode item with metadata from NFO.
    pub async fn enrich_episode(&self, item_id: Uuid, video_path: &Path) {
        let meta = find_episode_nfo(video_path).and_then(|p| parse_episode_nfo(&p));

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
            _ => {}
        }
    }

    async fn apply_metadata(&self, item_id: Uuid, meta: &NfoMetadata) {
        // Build external_ids JSON
        let external_ids = build_external_ids(meta);

        let item_meta = media_items::ItemMetadata {
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
                match people::find_or_create(&self.pool, &person.name, &person.person_type).await {
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
                    }
                    Err(e) => {
                        tracing::error!("Failed to find/create person {}: {e}", person.name);
                    }
                }
            }
        }
    }
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
    if ids.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(ids))
    }
}
