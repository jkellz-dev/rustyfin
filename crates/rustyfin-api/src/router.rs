use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Router,
    extract::FromRef,
    routing::{delete, get, post},
};
use sqlx::PgPool;
use tokio::sync::RwLock;

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
    pub transcode_manager: Arc<rustyfin_media::transcode::TranscodeManager>,
    pub image_cache_dir: PathBuf,
    pub scan_in_flight: Arc<RwLock<HashSet<uuid::Uuid>>>,
    pub scan_schedules: serde_json::Value,
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
        // ── API keys (admin) ──────────────────────────────────
        .route(
            "/Auth/Keys",
            get(api_keys::list_api_keys).post(api_keys::create_api_key),
        )
        .route("/Auth/Keys/{id}", delete(api_keys::delete_api_key))
        // ── Users ───────────────────────────────────────────────
        .route("/Users", get(users::list_users))
        .route("/Users/Me", get(users::get_current_user))
        .route("/Users/Public", get(users::get_public_users))
        .route(
            "/Users/{userId}",
            get(users::get_user_by_id).delete(users::delete_user),
        )
        .route("/Users/{userId}/Policy", post(users::update_user_policy))
        // ── Media update notifications ─────────────────────────
        .route("/Library/Media/Updated", post(webhooks::post_updated_media))
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
        .route("/Library/Schedules", get(libraries::get_schedules))
        .route(
            "/Library/VirtualFolders/{id}/ScanSchedule",
            post(libraries::update_scan_schedule),
        )
        .route(
            "/Library/VirtualFolders/Update",
            post(libraries::update_virtual_folder),
        )
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
        .route(
            "/Videos/{itemId}/stream",
            get(streaming::stream_video).head(streaming::head_stream),
        )
        .route(
            "/Videos/{itemId}/stream.{container}",
            get(streaming::stream_video_container).head(streaming::head_stream_container),
        )
        .route(
            "/Audio/{itemId}/stream",
            get(streaming::stream_audio).head(streaming::head_stream),
        )
        .route(
            "/Audio/{itemId}/stream.{container}",
            get(streaming::stream_audio_container).head(streaming::head_stream_container),
        )
        // ── HLS Transcoding ───────────────────────────────────────
        .route(
            "/Videos/{itemId}/master.m3u8",
            get(hls::get_master_playlist).head(hls::head_master_playlist),
        )
        .route("/Videos/{itemId}/main.m3u8", get(hls::get_variant_playlist))
        .route(
            "/Videos/{itemId}/hls1/{playSessionId}/{segmentFile}",
            get(hls::get_segment),
        )
        .route(
            "/Videos/ActiveEncodings",
            delete(hls::delete_active_encodings),
        )
        // ── Audio HLS Transcoding ────────────────────────────────────
        .route(
            "/Audio/{itemId}/master.m3u8",
            get(hls::get_audio_master_playlist),
        )
        .route(
            "/Audio/{itemId}/main.m3u8",
            get(hls::get_audio_variant_playlist),
        )
        .route(
            "/Audio/{itemId}/hls1/{playSessionId}/{segmentFile}",
            get(hls::get_audio_segment),
        )
        .route(
            "/Audio/{itemId}/universal",
            get(universal_audio::get_universal_audio).head(universal_audio::get_universal_audio),
        )
        // ── Subtitles ─────────────────────────────────────────────
        .route(
            "/Videos/{itemId}/Subtitles/{streamIndex}/Stream.vtt",
            get(subtitles::get_subtitle_stream),
        )
        .route(
            "/Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/Stream.vtt",
            get(subtitles::get_subtitle_stream_with_source),
        )
        .route(
            "/Videos/{itemId}/Subtitles/{streamIndex}/Stream.{format}",
            get(subtitles::get_subtitle_stream_format),
        )
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
        .route(
            "/UserItems/{itemId}/Rating",
            post(user_data::update_rating).delete(user_data::delete_rating),
        )
        .route(
            "/Users/{userId}/Items/{itemId}/Rating",
            post(user_data::update_rating_legacy).delete(user_data::delete_rating_legacy),
        )
        // ── Search ──────────────────────────────────────────────
        .route("/Search/Hints", get(search::search_hints))
        // ── Display Preferences ──────────────────────────────────
        .route(
            "/DisplayPreferences/{displayPreferencesId}",
            get(display_preferences::get_display_preferences)
                .post(display_preferences::update_display_preferences),
        )
        // ── TV Shows ─────────────────────────────────────────────
        .route("/Shows/NextUp", get(shows::get_next_up))
        .route("/Shows/Upcoming", get(stubs::empty_query_result_auth))
        .route("/Shows/{seriesId}/Seasons", get(shows::get_seasons))
        .route("/Shows/{seriesId}/Episodes", get(shows::get_episodes))
        // ── Quick Connect ────────────────────────────────────────
        .route("/QuickConnect/Enabled", get(quick_connect::get_enabled))
        .route("/QuickConnect/Initiate", post(quick_connect::initiate))
        .route("/QuickConnect/Connect", get(quick_connect::connect))
        .route("/QuickConnect/Authorize", post(quick_connect::authorize))
        // ── Localization ─────────────────────────────────────────
        .route("/Localization/Cultures", get(localization::get_cultures))
        .route("/Localization/Countries", get(localization::get_countries))
        .route(
            "/Localization/ParentalRatings",
            get(localization::get_parental_ratings),
        )
        .route("/Localization/Options", get(localization::get_options))
        // ── Configuration ────────────────────────────────────────
        .route(
            "/System/Configuration",
            get(configuration::get_configuration).post(configuration::update_configuration),
        )
        .route(
            "/System/Configuration/{key}",
            get(configuration::get_named_configuration)
                .post(configuration::update_named_configuration),
        )
        // ── Filters ──────────────────────────────────────────────
        .route("/Items/Filters", get(filters::get_filters_legacy))
        .route("/Items/Filters2", get(filters::get_filters))
        // ── Stub endpoints ───────────────────────────────────────
        .route("/Plugins", get(stubs::empty_array_admin))
        .route("/Packages", get(stubs::empty_array_admin))
        .route("/Repositories", get(stubs::empty_array_admin))
        .route(
            "/Items/{itemId}/Intros",
            get(stubs::empty_query_result_auth),
        )
        .route(
            "/Items/{itemId}/LocalTrailers",
            get(stubs::empty_array_auth),
        )
        .route(
            "/Items/{itemId}/SpecialFeatures",
            get(stubs::empty_array_auth),
        )
        .route("/Items/{itemId}/Similar", get(similar::get_similar))
        .route("/Items/Suggestions", get(suggestions::get_suggestions))
        .route("/Movies/Recommendations", get(similar::get_recommendations))
        .route("/Artists", get(artists::get_artists))
        .route("/Artists/AlbumArtists", get(artists::get_album_artists))
        .route("/Genres", get(genres::get_genres))
        .route("/Studios", get(studios::get_studios))
        .route("/Persons", get(persons::get_persons))
        .route(
            "/Users/{userId}/Suggestions",
            get(suggestions::get_suggestions),
        )
        // ── Playlists ──────────────────────────────────────────────
        .route("/Playlists", post(playlists::create_playlist))
        .route(
            "/Playlists/{playlistId}/Items",
            get(playlists::get_playlist_items)
                .post(playlists::add_playlist_items)
                .delete(playlists::remove_playlist_items),
        )
        .route(
            "/Playlists/{playlistId}/Items/{itemId}/Move/{newIndex}",
            post(playlists::move_playlist_item),
        )
        // ── Collections ────────────────────────────────────────────
        .route("/Collections", post(collections::create_collection))
        .route(
            "/Collections/{collectionId}/Items",
            post(collections::add_collection_items).delete(collections::remove_collection_items),
        )
        // ── WebSocket ────────────────────────────────────────────
        .route("/socket", get(websocket::websocket_handler))
        // ── State ───────────────────────────────────────────────
        .with_state(state)
}
