use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::user_data;
use rustyfin_models::dto::UserItemDataDto;

use crate::error::ApiError;
use crate::router::AppState;

/// `POST /UserFavoriteItems/{itemId}` — Mark as favorite.
pub async fn mark_favorite(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    let ud = user_data::set_favorite(&state.pool, auth.user_id, item_id, true)
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(to_dto(&ud)))
}

/// `DELETE /UserFavoriteItems/{itemId}` — Unmark favorite.
pub async fn unmark_favorite(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    let ud = user_data::set_favorite(&state.pool, auth.user_id, item_id, false)
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(to_dto(&ud)))
}

/// `POST /UserPlayedItems/{itemId}` — Mark as played.
pub async fn mark_played(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    let ud = user_data::set_played(&state.pool, auth.user_id, item_id, true)
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(to_dto(&ud)))
}

/// `DELETE /UserPlayedItems/{itemId}` — Mark as unplayed.
pub async fn mark_unplayed(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    let ud = user_data::set_played(&state.pool, auth.user_id, item_id, false)
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(to_dto(&ud)))
}

// Legacy routes

/// `POST /Users/{userId}/FavoriteItems/{itemId}` — Legacy mark favorite.
pub async fn mark_favorite_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    mark_favorite(state, auth, Path(item_id)).await
}

/// `DELETE /Users/{userId}/FavoriteItems/{itemId}` — Legacy unmark favorite.
pub async fn unmark_favorite_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    unmark_favorite(state, auth, Path(item_id)).await
}

/// `POST /Users/{userId}/PlayedItems/{itemId}` — Legacy mark played.
pub async fn mark_played_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    mark_played(state, auth, Path(item_id)).await
}

/// `DELETE /Users/{userId}/PlayedItems/{itemId}` — Legacy mark unplayed.
pub async fn mark_unplayed_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    mark_unplayed(state, auth, Path(item_id)).await
}

fn to_dto(ud: &user_data::UserDataRow) -> UserItemDataDto {
    const MIN_LIKE_VALUE: f64 = 6.5;
    UserItemDataDto {
        playback_position_ticks: ud.playback_position_ticks,
        play_count: ud.play_count,
        is_favorite: ud.is_favorite,
        played: ud.played,
        rating: ud.rating,
        likes: ud.rating.map(|r| r >= MIN_LIKE_VALUE),
        last_played_date: ud.last_played_date,
        ..Default::default()
    }
}
