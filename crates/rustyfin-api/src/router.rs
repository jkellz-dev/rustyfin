use std::sync::Arc;

use axum::{
    Router,
    extract::FromRef,
    routing::{get, post},
};
use sqlx::PgPool;

use crate::routes::*;

/// Forward auth state bundling trusted proxy list, header config, and admin group.
#[derive(Debug, Clone)]
pub struct ForwardAuthState {
    pub trusted_proxies: rustyfin_auth::trusted_proxy::TrustedProxies,
    pub headers: rustyfin_auth::forward_auth::ForwardAuthHeaders,
    pub admin_group: Option<String>,
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub server_id: String,
    pub server_name: String,
    pub base_url: String,
    pub port: u16,
    pub tmdb_api_key: Option<String>,
    pub session_manager: Arc<rustyfin_core::session::SessionManager>,
    pub search_service: Arc<rustyfin_search::SearchService>,
    pub forward_auth_config: Option<ForwardAuthState>,
    pub ldap_config: Option<rustyfin_auth::ldap::LdapConfig>,
}

impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

/// Build the complete Axum router with all Jellyfin-compatible API routes.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // ── System (no auth) ────────────────────────────────────
        .route("/System/Info/Public", get(system::get_public_info))
        .route("/System/Ping", get(system::ping).post(system::ping))
        // ── System (auth required) ──────────────────────────────
        .route("/System/Info", get(system::get_info))
        .route("/System/Stats", get(system::get_stats))
        .route(
            "/System/ActivityLog/Entries",
            get(activity_log::get_log_entries),
        )
        // ── Environment ─────────────────────────────────────────
        .route(
            "/Environment/DirectoryContents",
            get(environment::get_directory_contents),
        )
        // ── Startup ─────────────────────────────────────────────
        .route("/Startup/Configuration", get(system::get_startup_config))
        .route("/Startup/Complete", post(system::complete_startup))
        // ── Branding (no auth) ──────────────────────────────────
        .route(
            "/Branding/Configuration",
            get(branding::get_branding_configuration),
        )
        .route("/Branding/Css", get(branding::get_branding_css))
        // ── Auth ────────────────────────────────────────────────
        .route(
            "/Users/AuthenticateByName",
            post(auth::authenticate_by_name),
        )
        // ── Users ───────────────────────────────────────────────
        .route("/Users", get(users::list_users))
        .route("/Users/Me", get(users::get_current_user))
        .route("/Users/Public", get(users::get_public_users))
        .route(
            "/Users/{userId}",
            get(users::get_user_by_id).delete(users::delete_user),
        )
        .route("/Users/{userId}/Policy", post(users::update_user_policy))
        // ── Library views ───────────────────────────────────────
        .route("/UserViews", get(libraries::get_user_views))
        .route(
            "/Users/{userId}/Views",
            get(libraries::get_user_views_legacy),
        )
        .route(
            "/Library/VirtualFolders",
            get(libraries::get_virtual_folders)
                .post(libraries::create_virtual_folder)
                .delete(libraries::delete_virtual_folder),
        )
        .route("/Library/Refresh", post(libraries::refresh_library))
        // ── Items ───────────────────────────────────────────────
        .route("/Items", get(items::get_items))
        .route("/Items/Latest", get(items::get_latest_items))
        .route("/Items/{itemId}", get(items::get_item_by_id))
        .route("/Items/{itemId}/Refresh", post(items::refresh_item))
        .route("/UserItems/Resume", get(items::get_resume_items))
        // Legacy item routes
        .route("/Users/{userId}/Items", get(items::get_user_items_legacy))
        .route(
            "/Users/{userId}/Items/{itemId}",
            get(items::get_user_item_legacy),
        )
        // ── Images ──────────────────────────────────────────────
        .route(
            "/Items/{itemId}/Images/{imageType}",
            get(images::get_item_image),
        )
        .route(
            "/Items/{itemId}/Images/{imageType}/{imageIndex}",
            get(images::get_item_image_by_index),
        )
        // ── Media info ──────────────────────────────────────────
        .route(
            "/Items/{itemId}/PlaybackInfo",
            get(media_info::get_playback_info).post(media_info::post_playback_info),
        )
        // ── Streaming ───────────────────────────────────────────
        .route("/Videos/{itemId}/stream", get(streaming::stream_video))
        .route("/Audio/{itemId}/stream", get(streaming::stream_audio))
        // ── Sessions ──────────────────────────────────────────────
        .route("/Sessions", get(sessions::get_sessions))
        .route("/Sessions/Logout", post(sessions::logout))
        // ── Playback reporting ──────────────────────────────────
        .route("/Sessions/Playing", post(playback::report_playback_start))
        .route(
            "/Sessions/Playing/Progress",
            post(playback::report_playback_progress),
        )
        .route(
            "/Sessions/Playing/Stopped",
            post(playback::report_playback_stopped),
        )
        .route("/Sessions/Playing/Ping", post(playback::playback_ping))
        // ── User data (favorites, played) ───────────────────────
        .route(
            "/UserFavoriteItems/{itemId}",
            post(user_data::mark_favorite).delete(user_data::unmark_favorite),
        )
        .route(
            "/UserPlayedItems/{itemId}",
            post(user_data::mark_played).delete(user_data::mark_unplayed),
        )
        // Legacy user data routes
        .route(
            "/Users/{userId}/FavoriteItems/{itemId}",
            post(user_data::mark_favorite_legacy).delete(user_data::unmark_favorite_legacy),
        )
        .route(
            "/Users/{userId}/PlayedItems/{itemId}",
            post(user_data::mark_played_legacy).delete(user_data::mark_unplayed_legacy),
        )
        // ── Search ──────────────────────────────────────────────
        .route("/Search/Hints", get(search::search_hints))
        // ── WebSocket ────────────────────────────────────────────
        .route("/socket", get(websocket::websocket_handler))
        // ── State ───────────────────────────────────────────────
        .with_state(state)
}
