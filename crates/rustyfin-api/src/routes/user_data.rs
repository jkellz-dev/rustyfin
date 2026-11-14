use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::user_data;
use rustyfin_models::dto::UserItemDataDto;

use crate::error::ApiError;
use crate::router::AppState;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct RatingParams {
    pub likes: Option<bool>,
}

/// Jellyfin's `UpdateUserItemDataDto`: every field optional, only those sent
/// change. Clients send PascalCase; some send camelCase.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateUserItemData {
    #[serde(alias = "played")]
    pub played: Option<bool>,
    #[serde(alias = "playCount")]
    pub play_count: Option<i32>,
    #[serde(alias = "playbackPositionTicks")]
    pub playback_position_ticks: Option<i64>,
    #[serde(alias = "isFavorite")]
    pub is_favorite: Option<bool>,
    #[serde(alias = "rating")]
    pub rating: Option<f64>,
    #[serde(alias = "likes")]
    pub likes: Option<bool>,
    #[serde(alias = "lastPlayedDate")]
    pub last_played_date: Option<chrono::DateTime<chrono::Utc>>,
}

/// `GET /UserItems/{itemId}/UserData`: the caller's data, zeros when untouched.
pub async fn get_user_item_data(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let ud = user_data::get(&state.pool, auth.user_id, item_id)
        .await
        .map_err(ApiError::Database)?;
    let mut dto = ud.as_ref().map(to_dto).unwrap_or_default();
    dto.item_id = Some(item_id.simple().to_string());
    dto.key = Some(item_id.simple().to_string());
    Ok(Json(dto))
}

/// `POST /UserItems/{itemId}/UserData`: update the fields sent (Jellyfin 10.9+;
/// clients such as Plezzy mark played this way).
pub async fn update_user_item_data(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Json(body): Json<UpdateUserItemData>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    // `Likes` without a rating maps onto the rating, as the Rating route does.
    let rating = body
        .rating
        .or(body.likes.map(|l| if l { 10.0 } else { 1.0 }));
    let ud = user_data::update_partial(
        &state.pool,
        auth.user_id,
        item_id,
        body.played,
        body.play_count,
        body.playback_position_ticks,
        body.is_favorite,
        rating,
        body.last_played_date,
    )
    .await
    .map_err(ApiError::Database)?;
    let mut dto = to_dto(&ud);
    dto.item_id = Some(item_id.simple().to_string());
    dto.key = Some(item_id.simple().to_string());
    Ok(Json(dto))
}

/// `GET /Users/{userId}/Items/{itemId}/UserData` — legacy form.
pub async fn get_user_item_data_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    get_user_item_data(state, auth, Path(item_id)).await
}

/// `POST /Users/{userId}/Items/{itemId}/UserData` — legacy form.
pub async fn update_user_item_data_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
    body: Json<UpdateUserItemData>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    update_user_item_data(state, auth, Path(item_id), body).await
}
/// `POST /UserFavoriteItems/{itemId}` — Mark as favorite.
pub async fn mark_favorite(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
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
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
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
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
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
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
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

pub async fn update_rating(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(params): Query<RatingParams>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let ud = user_data::set_rating(&state.pool, auth.user_id, item_id, params.likes)
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(to_dto(&ud)))
}

pub async fn delete_rating(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let ud = user_data::set_rating(&state.pool, auth.user_id, item_id, None)
        .await
        .map_err(ApiError::Database)?;
    Ok(Json(to_dto(&ud)))
}

pub async fn update_rating_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
    query: Query<RatingParams>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    update_rating(state, auth, Path(item_id), query).await
}

pub async fn delete_rating_legacy(
    state: State<AppState>,
    auth: AuthenticatedUser,
    Path((_user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserItemDataDto>, ApiError> {
    delete_rating(state, auth, Path(item_id)).await
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
