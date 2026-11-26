use axum::{
    Json,
    extract::{Query, State},
};
use rustyfin_models::data::{CollectionStore, Data, ImageStore, MediaItemStore, SearchIndex};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use rustyfin_models::dto::{SearchHint, SearchHintResult};

use crate::error::{ApiError, DegradeExt};
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
    /// Only results in this library (Jellyfin's `ParentId`).
    pub parent_id: Option<Uuid>,
}

/// `GET /Search/Hints` — Search across all media.
pub async fn search_hints<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchHintResult>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);

    // The Collections view is no library: searching in it means collections,
    // wherever their films are (the admin searches the view a page is in).
    let in_collections =
        query.parent_id == Some(super::libraries::collections_view_id(&state.server_id));
    let item_types: Option<Vec<String>> = if in_collections {
        Some(vec!["BoxSet".to_string()])
    } else {
        query.include_item_types.as_deref().map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(String::from)
                .collect()
        })
    };

    let accessible = state.services.access().limited_to(auth.user_id).await?;

    // Results are filtered below (access, library), so ask for more.
    let wanted = if query.parent_id.is_some() {
        limit * 5
    } else {
        limit
    };
    let hits = state
        .data
        .search()
        .search(&query.search_term, wanted, item_types.as_deref())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Collections of one film are not listed.
    let boxsets: Vec<Uuid> = hits
        .iter()
        .filter(|h| h.item_type == "BoxSet")
        .map(|h| h.id)
        .collect();
    let hidden = state.data.collections().hidden_among(&boxsets).await?;
    // Titles of a linked server that is down are not shown.
    let hit_ids: Vec<Uuid> = hits.iter().map(|h| h.id).collect();
    let shown: std::collections::HashSet<Uuid> = state
        .data
        .media_items()
        .visible_ids(&hit_ids)
        .await?
        .into_iter()
        .collect();
    // Filter by library access, and to the library asked for.
    let hits: Vec<_> = hits
        .into_iter()
        .filter(|hit| !hidden.contains(&hit.id) && shown.contains(&hit.id))
        // The kinds asked for (the index may not filter: Postgres search).
        .filter(|hit| {
            item_types
                .as_ref()
                .is_none_or(|t| t.iter().any(|t| t.eq_ignore_ascii_case(&hit.item_type)))
        })
        .filter(|hit| {
            accessible
                .as_ref()
                .is_none_or(|ids| hit.library_id.is_none_or(|lid| ids.contains(&lid)))
        })
        .filter(|hit| {
            in_collections
                || query
                    .parent_id
                    .is_none_or(|pid| hit.library_id == Some(pid))
        })
        .take(limit as usize)
        .collect();

    // People too (Jellyfin's IncludePeople, on by default), unless the search
    // is narrowed to a library or to other kinds.
    let want_people = query.include_people != Some(false)
        && query.parent_id.is_none()
        && item_types
            .as_ref()
            .is_none_or(|t| t.iter().any(|t| t.eq_ignore_ascii_case("Person")));
    let people = if want_people {
        state
            .data
            .search()
            .search_people(&query.search_term, limit)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
    } else {
        Vec::new()
    };

    let mut scored: Vec<(f64, SearchHint)> = hits
        .into_iter()
        .map(|hit| {
            (
                hit.score,
                SearchHint {
                    id: hit.id,
                    item_id: hit.id,
                    media_type: super::items::media_type_for(&hit.item_type),
                    artists: Vec::new(),
                    primary_image_aspect_ratio: None,
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
                },
            )
        })
        .collect();
    scored.extend(people.into_iter().map(|(p, score)| {
        (
            score,
            SearchHint {
                id: p.id,
                item_id: p.id,
                media_type: super::items::media_type_for("Person"),
                artists: Vec::new(),
                primary_image_aspect_ratio: None,
                name: p.name,
                item_type: "Person".to_string(),
                production_year: None,
                primary_image_tag: None,
                thumb_image_tag: None,
                backdrop_image_tags: None,
                series_name: None,
                index_number: None,
                parent_index_number: None,
                run_time_ticks: None,
                channel_id: None,
            },
        )
    }));
    // Best first across titles and people (Meilisearch's scores); stable, so
    // Postgres answers (all 0) keep titles, then people.
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.truncate(limit as usize);
    let mut hints: Vec<SearchHint> = scored.into_iter().map(|(_, h)| h).collect();

    // Batch load images for search results
    let item_ids: Vec<Uuid> = hints.iter().map(|h| h.id).collect();
    if !item_ids.is_empty() {
        let all_images = state
            .data
            .images()
            .list_by_items(&item_ids)
            .await
            .or_log_default("images::list_by_items");

        // Group by item_id
        let mut images_map: std::collections::HashMap<Uuid, Vec<_>> =
            std::collections::HashMap::new();
        for img in all_images {
            if let Some(item_id) = img.entity_id {
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

    for hint in &mut hints {
        if hint.primary_image_tag.is_none() {
            let placeholder =
                rustyfin_media::image_processing::Placeholder::for_item_type(&hint.item_type);
            hint.primary_image_tag = Some(placeholder.tag().to_string());
            hint.primary_image_aspect_ratio = Some(placeholder.aspect_ratio());
        }
    }
    let total = hints.len() as i32;
    Ok(Json(SearchHintResult {
        search_hints: hints,
        total_record_count: total,
    }))
}
