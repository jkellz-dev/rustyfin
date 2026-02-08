use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, media_items};
use rustyfin_models::dto::{SearchHint, SearchHintResult};

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SearchQuery {
    pub search_term: String,
    pub limit: Option<i64>,
    pub include_item_types: Option<String>,
    pub include_media: Option<bool>,
    pub include_people: Option<bool>,
    pub include_genres: Option<bool>,
    pub include_studios: Option<bool>,
    pub include_artists: Option<bool>,
}

/// `GET /Search/Hints` — Search across all media.
pub async fn search_hints(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchHintResult>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);

    let items = media_items::search_by_name(&state.pool, &query.search_term, limit)
        .await
        .map_err(ApiError::Database)?;

    let mut hints: Vec<SearchHint> = items
        .iter()
        .map(|item| SearchHint {
            id: item.id,
            name: item.name.clone(),
            item_type: item.item_type.clone(),
            production_year: item.production_year,
            primary_image_tag: None,
            thumb_image_tag: None,
            backdrop_image_tags: None,
            series_name: item.series_name.clone(),
            index_number: item.index_number,
            parent_index_number: item.parent_index_number,
            run_time_ticks: item.runtime_ticks,
            channel_id: None,
        })
        .collect();

    // Batch load images for search results
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    if !item_ids.is_empty() {
        let all_images = images::list_by_items(&state.pool, &item_ids)
            .await
            .unwrap_or_default();

        // Group by item_id
        let mut images_map: std::collections::HashMap<Uuid, Vec<_>> =
            std::collections::HashMap::new();
        for img in all_images {
            if let Some(item_id) = img.item_id {
                images_map.entry(item_id).or_default().push(img);
            }
        }

        for hint in &mut hints {
            if let Some(item_images) = images_map.get(&hint.id) {
                let mut backdrop_tags = Vec::new();
                for img in item_images {
                    let tag = img.id.to_string();
                    match img.image_type.as_str() {
                        "Primary" => hint.primary_image_tag = Some(tag),
                        "Thumb" => hint.thumb_image_tag = Some(tag),
                        "Backdrop" => backdrop_tags.push(tag),
                        _ => {}
                    }
                }
                if !backdrop_tags.is_empty() {
                    hint.backdrop_image_tags = Some(backdrop_tags);
                }
            }
        }
    }

    let total = hints.len() as i32;
    Ok(Json(SearchHintResult {
        search_hints: hints,
        total_record_count: total,
    }))
}
