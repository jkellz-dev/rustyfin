use axum::{Json, extract::State, http::HeaderMap};

use rustyfin_auth::{forward_auth, middleware::parse_mediabrowser_header, session};
use rustyfin_db::repo::users;
use rustyfin_models::dto::{
    AuthenticateByNameRequest, AuthenticationResult, SessionInfoDto, UserConfiguration, UserDto,
    UserPolicy,
};

use crate::error::ApiError;
use crate::router::AppState;

/// `POST /Users/AuthenticateByName` — Jellyfin client compatibility shim.
///
/// With forward auth, the reverse proxy has already authenticated the user.
/// We extract the identity from forward auth headers and create a session.
/// If no forward auth headers are present, we check if a username was provided
/// and auto-provision the user (for development / trusted networks).
pub async fn authenticate_by_name(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AuthenticateByNameRequest>,
) -> Result<Json<AuthenticationResult>, ApiError> {
    // Extract device info from MediaBrowser auth header
    let mb_auth = headers
        .get("Authorization")
        .or_else(|| headers.get("X-Emby-Authorization"))
        .and_then(|v| v.to_str().ok())
        .map(parse_mediabrowser_header)
        .unwrap_or_default();

    // Try forward auth headers first
    let user = if let Some(identity) = forward_auth::extract_identity(&headers) {
        // User authenticated by reverse proxy
        let is_first_user = users::count(&state.pool)
            .await
            .map_err(ApiError::Database)?
            == 0;

        let user = users::upsert_from_forward_auth(
            &state.pool,
            &identity.remote_user,
            &identity.remote_user,
            identity.display_name.as_deref(),
            identity.email.as_deref(),
        )
        .await
        .map_err(ApiError::Database)?;

        // First user becomes admin
        if is_first_user {
            sqlx::query("UPDATE users SET is_administrator = true WHERE id = $1")
                .bind(user.id)
                .execute(&state.pool)
                .await
                .map_err(ApiError::Database)?;
        }

        user
    } else if let Some(username) = &body.username {
        // Fallback: look up user by username (for dev/trusted networks)
        users::find_by_username(&state.pool, username)
            .await
            .map_err(ApiError::Database)?
            .ok_or(ApiError::Unauthorized)?
    } else {
        return Err(ApiError::Unauthorized);
    };

    // Create session
    let token = session::create_session(
        &state.pool,
        user.id,
        mb_auth.device_id.as_deref().unwrap_or("unknown"),
        mb_auth.device.as_deref().unwrap_or("Unknown Device"),
        mb_auth.client.as_deref().unwrap_or("Unknown Client"),
        mb_auth.version.as_deref(),
    )
    .await
    .map_err(ApiError::Database)?;

    let user_dto = UserDto {
        name: user.username.clone(),
        server_id: state.server_id.clone(),
        id: user.id,
        has_password: false,
        has_configured_password: false,
        has_configured_easy_password: false,
        enable_auto_login: false,
        last_login_date: user.last_login_date,
        last_activity_date: user.last_activity_date,
        configuration: UserConfiguration::default(),
        policy: UserPolicy {
            is_administrator: user.is_administrator,
            ..Default::default()
        },
        primary_image_tag: None,
    };

    let session_info = SessionInfoDto {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.id,
        user_name: Some(user.username),
        client: mb_auth.client.unwrap_or_else(|| "Unknown".to_string()),
        device_name: mb_auth.device.unwrap_or_else(|| "Unknown".to_string()),
        device_id: mb_auth.device_id.unwrap_or_else(|| "unknown".to_string()),
        application_version: mb_auth.version,
        last_activity_date: chrono::Utc::now(),
        is_active: true,
        now_playing_item: None,
        play_state: None,
        supports_remote_control: false,
        server_id: state.server_id.clone(),
    };

    Ok(Json(AuthenticationResult {
        user: user_dto,
        session_info,
        access_token: token,
        server_id: state.server_id.clone(),
    }))
}
