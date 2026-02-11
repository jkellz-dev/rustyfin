use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
};

use rustyfin_auth::{forward_auth, ldap, middleware::parse_mediabrowser_header, session};
use rustyfin_db::repo::{activity_log, users};
use rustyfin_models::dto::{
    AuthenticateByNameRequest, AuthenticationResult, SessionInfoDto, UserConfiguration, UserDto,
    UserPolicy,
};

use crate::error::ApiError;
use crate::router::AppState;

/// `POST /Users/AuthenticateByName` — authenticate via forward auth or LDAPS bind.
///
/// 1. If forward auth is configured and request comes from a trusted proxy,
///    extract identity from configured headers.
/// 2. Otherwise, if LDAPS is configured, verify credentials via LDAP bind.
/// 3. If neither matches, return 401.
pub async fn authenticate_by_name(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
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

    tracing::debug!(
        "AuthenticateByName: connecting_ip={}, forward_auth_configured={}, ldap_configured={}",
        addr.ip(),
        state.forward_auth_config.is_some(),
        state.ldap_config.is_some(),
    );

    if let Some(ref fa_config) = state.forward_auth_config {
        let trusted = fa_config.trusted_proxies.is_trusted(&addr.ip());
        let has_header = forward_auth::extract_identity(&headers, &fa_config.headers).is_some();
        tracing::debug!(
            "Forward auth check: ip_trusted={trusted}, remote_user_header_present={has_header}"
        );
    }

    let is_first_user = users::count(&state.pool)
        .await
        .map_err(ApiError::Database)?
        == 0;

    // Try forward auth (only from trusted proxies)
    let user = if let Some(ref fa_config) = state.forward_auth_config
        && fa_config.trusted_proxies.is_trusted(&addr.ip())
        && let Some(identity) = forward_auth::extract_identity(&headers, &fa_config.headers)
    {
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
            let _ = users::update_policy(&state.pool, user.id, true, user.enable_all_libraries).await;
        }

        // Auto-admin from groups
        if let Some(ref admin_group) = fa_config.admin_group
            && identity.groups.iter().any(|g| g == admin_group)
            && !user.is_administrator
        {
            let _ = users::update_policy(&state.pool, user.id, true, user.enable_all_libraries).await;
        }

        user
    } else if let Some(ref ldap_config) = state.ldap_config {
        // LDAPS authentication path
        let username = body.username.as_deref().ok_or(ApiError::Unauthorized)?;
        let password = body.pw.as_deref().ok_or(ApiError::Unauthorized)?;

        let ldap_result = ldap::authenticate(ldap_config, username, password)
            .await
            .map_err(|e| {
                tracing::warn!("LDAP auth failed for '{username}': {e}");
                // Log failed attempt
                let pool = state.pool.clone();
                let username = username.to_string();
                tokio::spawn(async move {
                    let _ = activity_log::insert(
                        &pool,
                        &format!("Authentication failed for {username}"),
                        None,
                        "AuthenticationFailed",
                        "Error",
                        None,
                        None,
                    )
                    .await;
                });
                ApiError::Unauthorized
            })?;

        let user = users::upsert_from_ldap(
            &state.pool,
            &ldap_result.username,
            ldap_result.display_name.as_deref(),
            ldap_result.email.as_deref(),
        )
        .await
        .map_err(ApiError::Database)?;

        // Reject disabled users
        if user.is_disabled {
            return Err(ApiError::Unauthorized);
        }

        // First user becomes admin
        if is_first_user {
            let _ = users::update_policy(&state.pool, user.id, true, user.enable_all_libraries).await;
        }

        // Auto-admin from LDAP group membership
        if ldap_result.is_admin && !user.is_administrator {
            let _ = users::update_policy(&state.pool, user.id, true, user.enable_all_libraries).await;
        }

        user
    } else {
        return Err(ApiError::Unauthorized);
    };

    // Create session
    let device_id = mb_auth.device_id.as_deref().unwrap_or("unknown");
    let device_name = mb_auth.device.as_deref().unwrap_or("Unknown Device");
    let app_name = mb_auth.client.as_deref().unwrap_or("Unknown Client");
    let app_version = mb_auth.version.as_deref();

    let (token, session_id) = session::create_session(
        &state.pool,
        user.id,
        device_id,
        device_name,
        app_name,
        app_version,
    )
    .await
    .map_err(ApiError::Database)?;

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

    let ldap_enabled = state.ldap_config.is_some();

    let user_dto = UserDto {
        name: user.username.clone(),
        server_id: state.server_id.clone(),
        id: user.id,
        has_password: ldap_enabled,
        has_configured_password: ldap_enabled,
        has_configured_easy_password: false,
        enable_auto_login: false,
        last_login_date: user.last_login_date,
        last_activity_date: user.last_activity_date,
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
        play_state: None,
        supports_remote_control: false,
        server_id: state.server_id.clone(),
    };

    // Log authentication
    let _ = activity_log::insert(
        &state.pool,
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
