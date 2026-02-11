use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::{images, user_library_access};
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
    auth: AuthenticatedUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchHintResult>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);

    let item_types: Option<Vec<String>> = query.include_item_types.as_deref().map(|s| {
        s.split(',')
            .filter(|t| !t.is_empty())
            .map(String::from)
            .collect()
    });

    let accessible = user_library_access::get_accessible_library_ids(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?;

    let hits = state
        .search_service
        .search(&query.search_term, limit, item_types.as_deref())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Filter by library access
    let hits: Vec<_> = hits
        .into_iter()
        .filter(|hit| {
            accessible
                .as_ref()
                .is_none_or(|ids| hit.library_id.is_none_or(|lid| ids.contains(&lid)))
        })
        .collect();

    let mut hints: Vec<SearchHint> = hits
        .into_iter()
        .map(|hit| SearchHint {
            id: hit.id,
            name: hit.name,
            item_type: hit.item_type,
            production_year: hit.production_year,
            primary_image_tag: None,
            thumb_image_tag: None,
            backdrop_image_tags: None,
            series_name: hit.series_name,
            index_number: hit.index_number,
            parent_index_number: hit.parent_index_number,
            run_time_ticks: hit.runtime_ticks,
            channel_id: None,
        })
        .collect();

    // Batch load images for search results
    let item_ids: Vec<Uuid> = hints.iter().map(|h| h.id).collect();
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
