use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_db::repo::{activity_log, users};
use rustyfin_models::dto::{UpdateUserPolicyRequest, UserConfiguration, UserDto, UserPolicy};

use crate::error::ApiError;
use crate::router::AppState;

fn user_row_to_dto(row: &users::UserRow, server_id: &str, ldap_enabled: bool) -> UserDto {
    UserDto {
        name: row.username.clone(),
        server_id: server_id.to_string(),
        id: row.id,
        has_password: ldap_enabled,
        has_configured_password: ldap_enabled,
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
            is_disabled: row.is_disabled,
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
    let ldap_enabled = state.ldap_config.is_some();
    let rows = users::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    let dtos: Vec<UserDto> = rows
        .iter()
        .map(|r| user_row_to_dto(r, &state.server_id, ldap_enabled))
        .collect();
    Ok(Json(dtos))
}

/// `GET /Users/Me` — Get current user.
pub async fn get_current_user(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<UserDto>, ApiError> {
    let ldap_enabled = state.ldap_config.is_some();
    let user = users::find_by_id(&state.pool, auth.user_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(user_row_to_dto(&user, &state.server_id, ldap_enabled)))
}

/// `GET /Users/{id}` — Get user by ID.
pub async fn get_user_by_id(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserDto>, ApiError> {
    let ldap_enabled = state.ldap_config.is_some();
    let user = users::find_by_id(&state.pool, user_id)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(user_row_to_dto(&user, &state.server_id, ldap_enabled)))
}

/// `GET /Users/Public` — Public user list for login screen.
/// When LDAPS is configured, returns user list so clients show a user picker.
/// When forward-auth-only, returns empty list (proxy handles login).
pub async fn get_public_users(
    State(state): State<AppState>,
) -> Result<Json<Vec<UserDto>>, ApiError> {
    if state.ldap_config.is_none() {
        // Forward-auth-only: proxy handles login, no user picker needed
        return Ok(Json(Vec::new()));
    }

    let rows = users::list_all(&state.pool)
        .await
        .map_err(ApiError::Database)?;
    let dtos: Vec<UserDto> = rows
        .iter()
        .map(|r| user_row_to_dto(r, &state.server_id, true))
        .collect();
    Ok(Json(dtos))
}

/// `DELETE /Users/{userId}` — Delete a user (admin only). Cannot delete self.
pub async fn delete_user(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if admin.0.user_id == user_id {
        return Err(ApiError::BadRequest("Cannot delete yourself".to_string()));
    }
    let deleted = users::delete(&state.pool, user_id)
        .await
        .map_err(ApiError::Database)?;
    if !deleted {
        return Err(ApiError::NotFound);
    }

    let _ = activity_log::insert(
        &state.pool,
        "User deleted",
        None,
        "UserDeleted",
        "Informational",
        Some(admin.0.user_id),
        None,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Users/{userId}/Policy` — Update user admin policy (admin only).
pub async fn update_user_policy(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(user_id): Path<Uuid>,
    Json(req): Json<UpdateUserPolicyRequest>,
) -> Result<StatusCode, ApiError> {
    let updated = users::update_policy(&state.pool, user_id, req.is_administrator)
        .await
        .map_err(ApiError::Database)?;
    if !updated {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
