use rustyfin_models::data::{ActivityLogStore, Data, SessionStore, UserStore};
use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
};

use rustyfin_auth::{forward_auth, ldap, middleware::parse_mediabrowser_header, session};
use rustyfin_data::repo::users;
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

    let is_first_user = state.data.users().count().await? == 0;

    // Logins a trusted forward-auth proxy vouches for are never throttled;
    // every other attempt (passwords, untrusted callers) is.
    let vouched = state.forward_auth_config.as_ref().is_some_and(|fa| {
        fa.trusted_proxies.is_trusted(&addr.ip())
            && forward_auth::extract_identity(&headers, &fa.headers).is_some()
    });
    // Behind the proxy it names everyone: named nobody, nobody signs in.
    if state.forward_auth_config.is_some() && !vouched {
        return Err(ApiError::Unauthorized);
    }
    let name = body.username.as_deref();
    if !vouched
        && let Err(wait) = state
            .login_limiter
            .check(addr.ip(), name, std::time::Instant::now())
    {
        tracing::warn!("Login throttled for {} ({name:?})", addr.ip());
        metrics::counter!("rustyfin_login_throttled_total").increment(1);
        return Err(ApiError::TooManyRequests(wait.as_secs().max(1)));
    }
    let user = match resolve_login(&state, &headers, addr, &body, is_first_user).await {
        Ok(user) => {
            state.login_limiter.record_success(name);
            user
        }
        Err(ApiError::Unauthorized) => {
            if !vouched {
                state
                    .login_limiter
                    .record_failure(addr.ip(), name, std::time::Instant::now());
                metrics::counter!("rustyfin_login_failures_total").increment(1);
            }
            return Err(ApiError::Unauthorized);
        }
        Err(e) => return Err(e),
    };

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
        play_state: Some(rustyfin_models::dto::PlayerStateInfo::idle()),
        transcoding_info: None,
        supports_remote_control: false,
        server_id: state.server_id.clone(),
        playable_media_types: Some(vec!["Video".to_string(), "Audio".to_string()]),
        supported_commands: Some(vec![]),
        supports_media_control: true,
        now_viewing_item: None,
        capabilities: Some(rustyfin_models::dto::ClientCapabilitiesDto::basic()),
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

/// Who is logging in: the forward-auth identity from a trusted proxy, else an
/// LDAP bind, else nobody (401).
async fn resolve_login(
    state: &AppState,
    headers: &HeaderMap,
    addr: SocketAddr,
    body: &AuthenticateByNameRequest,
    is_first_user: bool,
) -> Result<users::UserRow, ApiError> {
    Ok(
        if let Some(ref fa_config) = state.forward_auth_config
            && fa_config.trusted_proxies.is_trusted(&addr.ip())
            && let Some(identity) = forward_auth::extract_identity(headers, &fa_config.headers)
        {
            // A disabled account does not sign in.
            state
                .services
                .users()
                .vouched(
                    &crate::identity::vouched_by(&identity),
                    fa_config.admin_group.as_deref(),
                    true,
                )
                .await?
                .ok_or(ApiError::Unauthorized)?
        } else if let Some(ref ldap_config) = state.ldap_config {
            // LDAPS authentication path
            let username = body.username.as_deref().ok_or(ApiError::Unauthorized)?;
            let password = body.pw.as_deref().ok_or(ApiError::Unauthorized)?;

            let ldap_result = ldap::authenticate(ldap_config, username, password)
                .await
                .map_err(|e| {
                    tracing::warn!("LDAP auth failed for '{username}': {e}");
                    // Log failed attempt
                    let data = state.data.clone();
                    let username = username.to_string();
                    tokio::spawn(async move {
                        let _ = data
                            .activity_log()
                            .insert(
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

            let user = state
                .data
                .users()
                .upsert_from_ldap(
                    &ldap_result.username,
                    ldap_result.display_name.as_deref(),
                    ldap_result.email.as_deref(),
                )
                .await?;

            // Reject disabled users
            if user.is_disabled {
                return Err(ApiError::Unauthorized);
            }

            // First user becomes admin
            if is_first_user {
                let _ = state
                    .data
                    .users()
                    .update_policy(user.id, true, user.enable_all_libraries)
                    .await;
            }

            // Auto-admin from LDAP group membership
            if ldap_result.is_admin && !user.is_administrator {
                let _ = state
                    .data
                    .users()
                    .update_policy(user.id, true, user.enable_all_libraries)
                    .await;
            }

            user
        } else {
            return Err(ApiError::Unauthorized);
        },
    )
}
