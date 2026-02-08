use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

use rustyfin_auth::middleware::AuthenticatedUser;
use rustyfin_db::repo::users;
use rustyfin_models::dto::{UserConfiguration, UserDto, UserPolicy};

use crate::error::ApiError;
use crate::router::AppState;

fn user_row_to_dto(row: &users::UserRow, server_id: &str) -> UserDto {
    UserDto {
        name: row.username.clone(),
        server_id: server_id.to_string(),
        id: row.id,
        has_password: false,
        has_configured_password: false,
        has_configured_easy_password: false,
        enable_auto_login: false,
        last_login_date: row.last_login_date,
        last_activity_date: row.last_activity_date,
        configuration: UserConfiguration {
            audio_language_preference: row.audio_language_preference.clone(),
            play_default_audio_track: row.play_default_audio_track,
            subtitle_language_preference: row.subtitle_language_preference.clone(),
            subtitle_mode: "Default".to_string(),
            enable_next_episode_auto_play: row.enable_next_episode_auto_play,
            ..Default::default()
        },
        policy: UserPolicy {
            is_administrator: row.is_administrator,
            ..Default::default()
        },
        primary_image_tag: None,
    }
}

/// `GET /Users` — List all users.
pub async fn list_users(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
) -> Result<Json<Vec<UserDto>>, ApiError> {
    let rows = users::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    let dtos: Vec<UserDto> = rows
        .iter()
        .map(|r| user_row_to_dto(r, &state.server_id))
        .collect();
    Ok(Json(dtos))
}

/// `GET /Users/Me` — Get current user.
pub async fn get_current_user(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<UserDto>, ApiError> {
    let user = users::find_by_id(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(user_row_to_dto(&user, &state.server_id)))
}

/// `GET /Users/{id}` — Get user by ID.
pub async fn get_user_by_id(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserDto>, ApiError> {
    let user = users::find_by_id(&state.pool, user_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(user_row_to_dto(&user, &state.server_id)))
}

/// `GET /Users/Public` — Public user list for login screen.
pub async fn get_public_users(
    State(state): State<AppState>,
) -> Result<Json<Vec<UserDto>>, ApiError> {
    let rows = users::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    let dtos: Vec<UserDto> = rows
        .iter()
        .map(|r| user_row_to_dto(r, &state.server_id))
        .collect();
    Ok(Json(dtos))
}
