use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use rustyfin_services::users::{PolicyChange, SettingsChange};
use uuid::Uuid;

use rustyfin_auth::middleware::{AdminUser, AuthenticatedUser};
use rustyfin_data::repo::users;
use rustyfin_models::dto::{UpdateUserPolicyRequest, UserConfiguration, UserDto, UserPolicy};

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
        // Jellyfin always has one; a user who never did anything gets their creation.
        last_activity_date: Some(
            row.last_activity_date
                .or(row.last_login_date)
                .unwrap_or(row.created_at),
        ),
        configuration: user_configuration(row),
        policy: UserPolicy {
            is_administrator: row.is_administrator,
            is_disabled: row.is_disabled,
            enable_all_folders: row.enable_all_libraries,
            ..Default::default()
        },
        primary_image_tag: None,
    }
}

/// Jellyfin's `SubtitlePlaybackMode` names, by their number.
const SUBTITLE_MODES: [&str; 5] = ["Default", "Always", "OnlyForced", "None", "Smart"];

/// The user's `UserConfiguration`: what the app last saved, with the settings
/// that have their own columns read from them.
pub(crate) fn user_configuration(row: &users::UserRow) -> UserConfiguration {
    let mut config = row
        .configuration
        .clone()
        .and_then(|v| serde_json::from_value::<UserConfiguration>(v).ok())
        .unwrap_or_default();
    config.audio_language_preference = row.audio_language_preference.clone();
    config.play_default_audio_track = row.play_default_audio_track;
    config.subtitle_language_preference = row
        .subtitle_language_preference
        .clone()
        .or_else(|| Some(String::new()));
    config.subtitle_mode = usize::try_from(row.subtitle_mode)
        .ok()
        .and_then(|i| SUBTITLE_MODES.get(i))
        .unwrap_or(&SUBTITLE_MODES[0])
        .to_string();
    config.enable_next_episode_auto_play = row.enable_next_episode_auto_play;
    if config.cast_receiver_id.is_empty() {
        config.cast_receiver_id = UserConfiguration::default().cast_receiver_id;
    }
    config
}

/// `POST /Users/{userId}/Configuration`: save the app's settings for a user
/// (the user themself, or an administrator).
/// `?userId=` on `POST /Users/Configuration`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserIdQuery {
    pub user_id: Option<Uuid>,
}

/// `POST /Users/Configuration`: as `POST /Users/{userId}/Configuration`, for
/// `?userId=` or else the signed-in user.
pub async fn update_own_configuration(
    state: State<AppState>,
    auth: AuthenticatedUser,
    axum::extract::Query(q): axum::extract::Query<UserIdQuery>,
    config: Json<UserConfiguration>,
) -> Result<StatusCode, ApiError> {
    let user_id = q.user_id.unwrap_or(auth.user_id);
    update_user_configuration(state, auth, Path(user_id), config).await
}

pub async fn update_user_configuration(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(user_id): Path<Uuid>,
    Json(config): Json<UserConfiguration>,
) -> Result<StatusCode, ApiError> {
    let blob = serde_json::to_value(&config)
        .map_err(|e| ApiError::Internal(format!("user configuration: {e}")))?;
    let mode = SUBTITLE_MODES
        .iter()
        .position(|m| m.eq_ignore_ascii_case(&config.subtitle_mode))
        .unwrap_or(0) as i32;
    let change = SettingsChange {
        audio_language: config.audio_language_preference.clone(),
        subtitle_language: config.subtitle_language_preference.clone(),
        subtitle_mode: mode,
        play_default_audio_track: config.play_default_audio_track,
        next_episode_auto_play: config.enable_next_episode_auto_play,
        configuration: blob,
    };
    state
        .services
        .users()
        .save_settings(super::access::viewer(&auth), user_id, &change)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A user as Jellyfin shows them, with the libraries they are limited to.
async fn user_dto(state: &AppState, user: &users::UserRow) -> UserDto {
    let mut dto = user_row_to_dto(user, &state.server_id);
    if let Some(ids) = state.services.users().enabled_libraries(user).await {
        dto.policy.enabled_folders = ids.into_iter().map(|id| id.to_string()).collect();
    }
    dto
}

/// `GET /Users` — List all users.
pub async fn list_users(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
) -> Result<Json<Vec<UserDto>>, ApiError> {
    let mut dtos = Vec::new();
    for user in state.services.users().list().await? {
        dtos.push(user_dto(&state, &user).await);
    }
    Ok(Json(dtos))
}

/// `GET /Users/Me` — Get current user.
pub async fn get_current_user(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<UserDto>, ApiError> {
    let user = state.services.users().get(auth.user_id).await?;
    Ok(Json(user_dto(&state, &user).await))
}

/// `GET /Users/{id}` — Get user by ID.
pub async fn get_user_by_id(
    State(state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserDto>, ApiError> {
    let user = state.services.users().get(user_id).await?;
    Ok(Json(user_dto(&state, &user).await))
}

/// `GET /Users/Public` — the login screen's user picker: always empty, as
/// the proxy in front signs people in.
pub async fn get_public_users() -> Json<Vec<UserDto>> {
    Json(Vec::new())
}

/// `DELETE /Users/{userId}` — Delete a user (admin only). Cannot delete self.
pub async fn delete_user(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .services
        .users()
        .delete(admin.0.user_id, user_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /Users/{userId}/Policy` — Update user policy (admin only).
/// Jellyfin clients send the full UserPolicy object as the request body.
pub async fn update_user_policy(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(user_id): Path<Uuid>,
    Json(req): Json<UpdateUserPolicyRequest>,
) -> Result<StatusCode, ApiError> {
    let change = PolicyChange {
        is_administrator: req.is_administrator,
        all_libraries: req.enable_all_folders,
        libraries: req
            .enabled_folders
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect(),
    };
    state.services.users().set_policy(user_id, &change).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /Users/{userId}/Images/{imageType}` — Redirect to configured user image URL.
pub async fn get_user_image(
    State(state): State<AppState>,
    Path((user_id, _image_type)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    let Some(ref template) = state.user_image_url_template else {
        return Err(ApiError::NotFound);
    };

    let user = state.services.users().get(user_id).await?;

    let url = template
        .replace("{username}", &user.username)
        .replace("{user_id}", &user_id.to_string());

    Ok(Redirect::temporary(&url).into_response())
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct UserImageQuery {
    #[serde(alias = "UserId")]
    #[serde(rename = "userId")]
    pub user_id: Option<Uuid>,
}

/// `GET /UserImage?userId=` — a user's picture by query (Jellyfin 10.9+).
pub async fn get_user_image_by_query(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<UserImageQuery>,
) -> Result<Response, ApiError> {
    let user_id = query.user_id.ok_or(ApiError::NotFound)?;
    get_user_image(State(state), Path((user_id, "Primary".to_string()))).await
}

/// `GET /Users/{userId}/Images/{imageType}/{imageIndex}` — Same as above, ignoring index.
pub async fn get_user_image_by_index(
    State(state): State<AppState>,
    Path((user_id, image_type, _image_index)): Path<(Uuid, String, i32)>,
) -> Result<Response, ApiError> {
    get_user_image(State(state), Path((user_id, image_type))).await
}
