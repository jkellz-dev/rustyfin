//! Lyrics and media segments (roadmap 11a: Plezy asks for both).

use crate::auth::middleware::AuthenticatedUser;
use axum::{
    Json,
    extract::{Path, State},
};
use rustyfin_models::data::{Data, MediaItemStore};
use rustyfin_models::dto::lyrics::{LyricDto, LyricLineDto, LyricMetadata};
use rustyfin_models::dto::query_result::QueryResult;
use uuid::Uuid;

use crate::error::ApiError;
use crate::router::AppState;

/// `GET /Audio/{itemId}/Lyrics`: the track's sidecar `.lrc`/`.txt` or tagged
/// lyrics; 404 when it has none, as Jellyfin.
pub async fn get_lyrics<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<LyricDto>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let item = state
        .data
        .media_items()
        .find_by_id(item_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if item.item_type != "Audio" {
        return Err(ApiError::NotFound);
    }
    let path = item.path.ok_or(ApiError::NotFound)?;
    let lines = tokio::task::spawn_blocking(move || {
        rustyfin_services::lyrics::read_lyrics(std::path::Path::new(&path))
    })
    .await
    .map_err(|e| ApiError::Internal(format!("reading lyrics: {e}")))?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(LyricDto {
        metadata: LyricMetadata::default(),
        lyrics: lines
            .into_iter()
            .map(|l| LyricLineDto {
                text: l.text,
                start: l.start_ticks,
            })
            .collect(),
    }))
}

/// `GET /MediaSegments/{itemId}`: Rustyfin detects no intros or credits, so
/// an item's segments are an empty list (Jellyfin's answer without a segment
/// provider); 404 for an item the caller cannot see.
pub async fn get_media_segments<D: Data>(
    State(state): State<AppState<D>>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<QueryResult<serde_json::Value>>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    Ok(Json(QueryResult::empty()))
}
