use rustyfin_models::data::{ActivityLogStore, Data, SessionStore};
use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
};

use crate::auth::{middleware::parse_mediabrowser_header, session};
use crate::dto::{
    AuthenticateByNameRequest, AuthenticationResult, SessionInfoDto, UserConfiguration, UserDto,
    UserPolicy,
};

use crate::error::ApiError;
use crate::router::AppState;

/// `POST /Users/AuthenticateByName` — a session for whoever the trusted
/// proxy in front says is asking. The name and password the app sends are
/// ignored: Rustyfin checks no passwords. Nobody vouched for: 401.
pub async fn authenticate_by_name<D: Data>(
    State(state): State<AppState<D>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(_credentials): Json<AuthenticateByNameRequest>,
) -> Result<Json<AuthenticationResult>, ApiError> {
    // Extract device info from MediaBrowser auth header. A proxy may own
    // `Authorization` (`Bearer`, a linked server's OIDC token): then the
    // MediaBrowser part is in `X-Emby-Authorization`.
    let values: Vec<&str> = ["Authorization", "X-Emby-Authorization"]
        .iter()
        .filter_map(|h| headers.get(*h).and_then(|v| v.to_str().ok()))
        .collect();
    let mb_auth = values
        .iter()
        .find(|v| v.starts_with("MediaBrowser ") || v.starts_with("Emby "))
        .or(values.first())
        .map(|v| parse_mediabrowser_header(v))
        .unwrap_or_default();

    // Behind the proxy it names everyone: named nobody, nobody signs in.
    let Some(fa) = state
        .forward_auth_config
        .as_ref()
        .filter(|fa| fa.trusted_proxies.is_trusted(&addr.ip()))
    else {
        tracing::debug!("AuthenticateByName from {}: not a trusted proxy", addr.ip());
        return Err(ApiError::Unauthorized);
    };
    let Some(identity) = rustyfin_services::proxy::extract_identity(
        |name| headers.get(name).and_then(|v| v.to_str().ok()),
        &fa.headers,
    ) else {
        tracing::debug!("AuthenticateByName from {}: no identity header", addr.ip());
        return Err(ApiError::Unauthorized);
    };
    // A disabled account does not sign in.
    let user = state
        .services
        .users()
        .vouched(&identity, fa.admin_group.as_deref(), true)
        .await?
        .ok_or(ApiError::Unauthorized)?;

    // Create session
    let device_id = mb_auth.device_id.as_deref().unwrap_or("unknown");
    let device_name = mb_auth.device.as_deref().unwrap_or("Unknown Device");
    let app_name = mb_auth.client.as_deref().unwrap_or("Unknown Client");
    let app_version = mb_auth.version.as_deref();

    // Signing in is a no-op (owner, 2026-11-22): a device that has a session
    // keeps it and its token, so addresses an app built with the token (a
    // queue of tracks) keep working.
    let existing = state
        .data
        .sessions()
        .find_active_for_device(user.id, device_id)
        .await?;
    let (token, session_id) = match existing {
        Some(s) => (s.access_token, s.id),
        None => {
            session::create_session(
                &state.data,
                user.id,
                device_id,
                device_name,
                app_name,
                app_version,
            )
            .await?
        }
    };

    // Register session with in-memory manager
    state
        .session_manager
        .register_session(
            session_id,
            user.id,
            &user.username,
            device_id,
            device_name,
            app_name,
            app_version,
        )
        .await;

    let user_dto = UserDto {
        name: user.username.clone(),
        server_id: state.server_id.clone(),
        id: user.id,
        has_password: false,
        has_configured_password: false,
        has_configured_easy_password: false,
        enable_auto_login: false,
        last_login_date: user.last_login_date,
        last_activity_date: Some(
            user.last_activity_date
                .or(user.last_login_date)
                .unwrap_or(user.created_at),
        ),
        configuration: UserConfiguration::default(),
        policy: UserPolicy {
            is_administrator: user.is_administrator,
            is_disabled: user.is_disabled,
            ..Default::default()
        },
        primary_image_tag: None,
    };

    let session_info = SessionInfoDto {
        id: session_id.to_string(),
        user_id: user.id,
        user_name: Some(user.username),
        client: app_name.to_string(),
        device_name: device_name.to_string(),
        device_id: device_id.to_string(),
        application_version: app_version.map(String::from),
        last_activity_date: chrono::Utc::now(),
        is_active: true,
        now_playing_item: None,
        play_state: Some(crate::dto::PlayerStateInfo::idle()),
        transcoding_info: None,
        supports_remote_control: false,
        server_id: state.server_id.clone(),
        playable_media_types: Some(vec!["Video".to_string(), "Audio".to_string()]),
        supported_commands: Some(vec![]),
        supports_media_control: true,
        now_viewing_item: None,
        capabilities: Some(crate::dto::ClientCapabilitiesDto::basic()),
        remote_end_point: None,
        now_playing_queue: Some(Vec::new()),
        user_primary_image_tag: None,
        additional_users: Vec::new(),
        now_playing_queue_full_items: Vec::new(),
        has_custom_device_name: false,
        last_playback_check_in: chrono::Utc::now(),
    };

    // Log authentication
    let _ = state
        .data
        .activity_log()
        .insert(
            "User authenticated",
            None,
            "AuthenticationSucceeded",
            "Informational",
            Some(user.id),
            None,
        )
        .await;

    Ok(Json(AuthenticationResult {
        user: user_dto,
        session_info,
        access_token: token,
        server_id: state.server_id.clone(),
    }))
}
