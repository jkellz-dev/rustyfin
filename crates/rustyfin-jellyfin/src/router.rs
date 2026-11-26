use rustyfin_models::data::Data;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Router,
    routing::{delete, get, post},
};
use tokio::sync::RwLock;

use crate::routes::*;

/// Forward auth state bundling trusted proxy list, header config, and admin group.
#[derive(Debug, Clone)]
pub struct ForwardAuthState {
    pub trusted_proxies: rustyfin_services::proxy::TrustedProxies,
    pub headers: rustyfin_services::proxy::ForwardAuthHeaders,
    pub admin_group: Option<String>,
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState<D> {
    /// The data layer (`CLAUDE.md`, "Layers").
    pub data: D,
    /// The rules over `data` (`rustyfin-services`).
    pub services: rustyfin_services::Services<D>,
    /// Serving a server that links this one: `data` leaves out what links
    /// hold, and so does everything else (`link_scope`). Set by
    /// [`build_router`] for its second copy of the routes.
    pub local_only: bool,
    pub server_id: String,
    pub server_name: String,
    pub base_url: String,
    pub port: u16,
    pub tmdb_api_key: Option<String>,
    pub session_manager: Arc<rustyfin_services::session::SessionManager<D>>,
    pub forward_auth_config: Option<ForwardAuthState>,
    pub transcode_manager: Arc<rustyfin_media::transcode::TranscodeManager>,
    pub image_cache_dir: PathBuf,
    pub people_cache_dir: Option<PathBuf>,
    pub scan_in_flight: Arc<RwLock<HashSet<uuid::Uuid>>>,
    pub scan_schedules: serde_json::Value,
    pub user_image_url_template: Option<String>,
    pub task_scheduler: Option<Arc<rustyfin_services::TaskScheduler<D>>>,
    /// Fired by `POST /System/Shutdown` and `/System/Restart`; the server binary
    /// waits on it to begin a graceful exit.
    pub shutdown: Arc<tokio::sync::Notify>,
    /// Linked servers, whose copies this server relays (roadmap 16b).
    pub links: Arc<rustyfin_services::link::Links>,
    /// `[playback] prefer_direct_play`: of a title's copies, those the app
    /// can play as is come before better ones that would be converted.
    pub prefer_direct_play: bool,
}

impl<D: Data> crate::auth::middleware::DataState for AppState<D> {
    type Data = D;
    fn data(&self) -> &D {
        &self.data
    }
}

/// The Jellyfin API's router: every route not under `/api/v1` (that is
/// Rustyfin's own, `rustyfin-api`). Routes match in any case (`route_case`):
/// a request's path is given its route's case before routing, which a layer
/// inside the router would be too late for, so the routes sit behind a
/// fallback that does it.
///
/// A server linking this one is served by a second copy of the routes that
/// sees only this server's own media (`link_scope`).
///
/// `observe` is what the server does to every route (metrics, the request
/// log): it gets the routes, their state given, before Jellyfin's query-name
/// fix-up wraps them.
pub fn build_router<D: Data>(state: AppState<D>, observe: fn(Router) -> Router) -> Router {
    let local_data = state.data.local_only();
    let local_state = AppState {
        services: state.services.with_data(local_data.clone()),
        data: local_data,
        local_only: true,
        ..state.clone()
    };
    let scope = crate::link_scope::LinkScope::new(state.data.clone(), routes(local_state, observe));
    let vouch_state = state.clone();
    let routes = routes(state, observe);
    Router::new().fallback_service(
        tower::ServiceBuilder::new()
            // Ids in answers as Jellyfin writes them (32 hex digits).
            .layer(axum::middleware::from_fn(crate::guid_format::simple_guids))
            .layer(axum::middleware::from_fn(crate::route_case::route_case))
            // Tailscale (the trusted proxy) is the authentication.
            .layer(axum::middleware::from_fn_with_state(
                vouch_state,
                crate::identity::vouch::<D>,
            ))
            .layer(axum::middleware::from_fn_with_state(
                scope,
                crate::link_scope::dispatch::<D>,
            ))
            .service(routes),
    )
}

fn routes<D: Data>(state: AppState<D>, observe: fn(Router) -> Router) -> Router {
    let routes = Router::new()
        // ── System (no auth) ────────────────────────────────────
        .route("/System/Info/Public", get(system::get_public_info::<D>))
        .route("/System/Ping", get(system::ping::<D>).post(system::ping::<D>))
        // ── System (auth required) ──────────────────────────────
        .route("/System/Info", get(system::get_info::<D>))
        .route("/System/Endpoint", get(system::get_endpoint))
        .route("/System/Stats", get(system::get_stats::<D>))
        .route("/System/Shutdown", post(system::shutdown::<D>))
        .route("/System/Restart", post(system::restart::<D>))
        .route("/System/Logs", get(system::get_logs))
        .route("/System/Logs/Log", get(system::get_log_file))
        .route(
            "/System/ActivityLog/Entries",
            get(activity_log::get_log_entries::<D>),
        )
        // ── Environment ─────────────────────────────────────────
        .route(
            "/Environment/DirectoryContents",
            get(environment::get_directory_contents),
        )
        // ── Startup ─────────────────────────────────────────────
        .route("/Startup/Configuration", get(system::get_startup_config::<D>))
        .route("/Startup/Complete", post(system::complete_startup))
        .route("/Startup/User", get(system::get_startup_user::<D>))
        .route("/Startup/FirstUser", get(system::get_startup_user::<D>))
        // ── Branding (no auth) ──────────────────────────────────
        .route(
            "/Branding/Configuration",
            get(branding::get_branding_configuration),
        )
        .route("/Branding/Css", get(branding::get_branding_css))
        .route("/Branding/Css.css", get(branding::get_branding_css))
        .route("/Branding/Splashscreen", get(stubs::problem_not_found))
        // ── Auth ────────────────────────────────────────────────
        .route(
            "/Users/AuthenticateByName",
            post(auth::authenticate_by_name::<D>),
        )
        .route(
            "/Users/AuthenticateWithQuickConnect",
            post(auth::authenticate_by_name::<D>),
        )
        // ── API keys (admin) ──────────────────────────────────
        .route(
            "/Auth/Keys",
            get(api_keys::list_api_keys::<D>).post(api_keys::create_api_key::<D>),
        )
        .route("/Auth/Keys/{id}", delete(api_keys::delete_api_key::<D>))
        // ── Auth provider info (admin, stubs) ─────────────────
        .route("/Auth/Providers", get(configuration::get_auth_providers))
        .route(
            "/Auth/PasswordResetProviders",
            get(stubs::empty_array_admin),
        )
        // ── Users ───────────────────────────────────────────────
        .route("/Users", get(users::list_users::<D>))
        .route("/Users/Me", get(users::get_current_user::<D>))
        .route("/Users/Public", get(users::get_public_users))
        .route(
            "/Users/{userId}",
            get(users::get_user_by_id::<D>).delete(users::delete_user::<D>),
        )
        .route("/Users/{userId}/Policy", post(users::update_user_policy::<D>))
        // Jellyfin's authentication actions are no-ops (owner, 2026-11-22).
        .route("/Users/{userId}/Password", post(stubs::no_content_auth))
        .route("/Users/{userId}/EasyPassword", post(stubs::no_content_auth))
        .route("/Users/Password", post(stubs::no_content_auth))
        .route("/Users/ForgotPassword", post(stubs::forgot_password))
        .route("/Users/ForgotPassword/Pin", post(stubs::forgot_password_pin))
        .route(
            "/Users/{userId}/Configuration",
            post(users::update_user_configuration::<D>),
        )
        .route(
            "/Users/Configuration",
            post(users::update_own_configuration::<D>),
        )
        .route("/Users/New", post(stubs::create_user_disabled))
        // ── User images ──────────────────────────────────────
        .route(
            "/Users/{userId}/Images/{imageType}",
            get(users::get_user_image::<D>)
                .post(stubs::no_content_auth)
                .delete(stubs::no_content_auth),
        )
        .route(
            "/Users/{userId}/Images/{imageType}/{imageIndex}",
            get(users::get_user_image_by_index::<D>),
        )
        // ── Media update notifications ─────────────────────────
        .route("/Library/Media/Updated", post(webhooks::post_updated_media::<D>))
        // Jellyfin's older library notifications, by provider id.
        .route("/Library/Movies/Added", post(webhooks::post_updated_movies::<D>))
        .route("/Library/Movies/Updated", post(webhooks::post_updated_movies::<D>))
        .route("/Library/Series/Added", post(webhooks::post_updated_series::<D>))
        .route("/Library/Series/Updated", post(webhooks::post_updated_series::<D>))
        // ── Library views ───────────────────────────────────────
        .route("/UserViews", get(libraries::get_user_views::<D>))
        .route(
            "/UserViews/GroupingOptions",
            get(libraries::get_user_views_grouping_options::<D>),
        )
        .route(
            "/Users/{userId}/GroupingOptions",
            get(libraries::get_grouping_options::<D>),
        )
        .route("/Library/PhysicalPaths", get(libraries::get_physical_paths::<D>))
        .route(
            "/Users/{userId}/Views",
            get(libraries::get_user_views_legacy::<D>),
        )
        .route(
            "/Library/VirtualFolders",
            get(libraries::get_virtual_folders::<D>)
                .post(libraries::create_virtual_folder::<D>)
                .delete(libraries::delete_virtual_folder::<D>),
        )
        .route("/Library/MediaFolders", get(libraries::get_media_folders::<D>))
        .route("/Library/Refresh", post(libraries::refresh_library::<D>))
        .route("/Library/Schedules", get(libraries::get_schedules::<D>))
        .route(
            "/Library/VirtualFolders/{id}/ScanSchedule",
            post(libraries::update_scan_schedule::<D>),
        )
        .route(
            "/Library/VirtualFolders/Update",
            post(libraries::update_virtual_folder::<D>),
        )
        .route(
            "/Library/VirtualFolders/Name",
            post(libraries::rename_virtual_folder::<D>),
        )
        .route(
            "/Library/VirtualFolders/Paths",
            post(libraries::add_media_path::<D>).delete(libraries::remove_media_path::<D>),
        )
        .route(
            "/Library/VirtualFolders/Paths/Update",
            post(libraries::update_media_path::<D>),
        )
        .route(
            "/Library/VirtualFolders/LibraryOptions",
            post(libraries::update_library_options::<D>),
        )
        // ── Items ───────────────────────────────────────────────
        .route("/Items", get(items::get_items::<D>))
        .route("/Items/Counts", get(items::get_item_counts::<D>))
        .route("/Items/Latest", get(items::get_latest_items::<D>))
        .route("/Items/Root", get(libraries::get_root_folder::<D>))
        .route("/Items/{itemId}", get(items::get_item_by_id::<D>))
        .route("/Items/{itemId}/Refresh", post(items::refresh_item::<D>))
        .route("/UserItems/Resume", get(items::get_resume_items::<D>))
        .route(
            "/Users/{userId}/Items/Resume",
            get(items::get_user_resume_items_legacy::<D>),
        )
        // Legacy item routes
        .route("/Users/{userId}/Items", get(items::get_user_items_legacy::<D>))
        .route(
            "/Users/{userId}/Items/Latest",
            get(items::get_user_latest_items_legacy::<D>),
        )
        .route(
            "/Users/{userId}/Items/Root",
            get(libraries::get_root_folder_legacy::<D>),
        )
        .route(
            "/Users/{userId}/Items/{itemId}",
            get(items::get_user_item_legacy::<D>),
        )
        // ── Images ──────────────────────────────────────────────
        .route(
            "/Items/{itemId}/Images/{imageType}",
            get(images::get_item_image::<D>),
        )
        .route(
            "/Items/{itemId}/Images/{imageType}/{imageIndex}",
            get(images::get_item_image_by_index::<D>),
        )
        .route(
            "/Items/{itemId}/Images/{imageType}/{imageIndex}/{tag}/{format}/{maxWidth}/{maxHeight}/{percentPlayed}/{unplayedCount}",
            get(images::get_item_image_by_path::<D>),
        )
        .route("/Items/{itemId}/Images", get(images::get_item_images_list::<D>))
        // ── Entity image stubs (genres, studios, persons, artists, music genres) ──
        .route(
            "/Genres/{name}/Images/{imageType}",
            get(genres::get_genre_image::<D>),
        )
        .route(
            "/Genres/{name}/Images/{imageType}/{imageIndex}",
            get(genres::get_genre_image_by_index::<D>),
        )
        .route(
            "/Studios/{name}/Images/{imageType}",
            get(studios::get_studio_image::<D>),
        )
        .route(
            "/Studios/{name}/Images/{imageType}/{imageIndex}",
            get(studios::get_studio_image_by_index::<D>),
        )
        .route(
            "/Persons/{name}/Images/{imageType}",
            get(persons::get_person_image::<D>),
        )
        .route(
            "/Persons/{name}/Images/{imageType}/{imageIndex}",
            get(persons::get_person_image_by_index::<D>),
        )
        .route(
            "/Artists/{name}/Images/{imageType}",
            get(artists::get_artist_image::<D>),
        )
        .route(
            "/Artists/{name}/Images/{imageType}/{imageIndex}",
            get(artists::get_artist_image_by_index::<D>),
        )
        .route(
            "/MusicGenres/{name}/Images/{imageType}",
            get(stubs::not_found),
        )
        .route(
            "/MusicGenres/{name}/Images/{imageType}/{imageIndex}",
            get(stubs::not_found),
        )
        // ── Media info ──────────────────────────────────────────
        .route(
            "/Items/{itemId}/PlaybackInfo",
            get(media_info::get_playback_info::<D>).post(media_info::post_playback_info::<D>),
        )
        // ── Streaming ───────────────────────────────────────────
        .route(
            "/Videos/{itemId}/stream",
            get(streaming::stream_video::<D>).head(streaming::head_stream::<D>),
        )
        .route(
            "/Videos/{itemId}/stream.{container}",
            get(streaming::stream_video_container::<D>).head(streaming::head_stream_container::<D>),
        )
        .route(
            "/Items/{itemId}/File",
            get(streaming::item_file::<D>).head(streaming::head_stream::<D>),
        )
        .route("/Items/{itemId}/Download", get(streaming::download_item::<D>))
        .route(
            "/Audio/{itemId}/stream",
            get(streaming::stream_audio::<D>).head(streaming::head_stream::<D>),
        )
        .route(
            "/Audio/{itemId}/stream.{container}",
            get(streaming::stream_audio_container::<D>).head(streaming::head_stream_container::<D>),
        )
        // ── HLS Transcoding ───────────────────────────────────────
        .route(
            "/Videos/{itemId}/master.m3u8",
            get(hls::get_master_playlist::<D>).head(hls::head_master_playlist),
        )
        .route("/Videos/{itemId}/main.m3u8", get(hls::get_variant_playlist::<D>))
        .route(
            "/Videos/{itemId}/hls1/{playSessionId}/{segmentFile}",
            get(hls::get_segment::<D>),
        )
        .route(
            "/Videos/ActiveEncodings",
            delete(hls::delete_active_encodings::<D>),
        )
        // ── Audio HLS Transcoding ────────────────────────────────────
        .route(
            "/Audio/{itemId}/master.m3u8",
            get(hls::get_audio_master_playlist::<D>),
        )
        .route(
            "/Audio/{itemId}/main.m3u8",
            get(hls::get_audio_variant_playlist::<D>),
        )
        .route(
            "/Audio/{itemId}/hls1/{playSessionId}/{segmentFile}",
            get(hls::get_audio_segment::<D>),
        )
        .route(
            "/Audio/{itemId}/universal",
            get(universal_audio::get_universal_audio::<D>).head(universal_audio::get_universal_audio::<D>),
        )
        // ── Subtitles ─────────────────────────────────────────────
        .route(
            "/Videos/{itemId}/Subtitles/{streamIndex}/Stream.vtt",
            get(subtitles::get_subtitle_stream::<D>),
        )
        .route(
            "/Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/Stream.vtt",
            get(subtitles::get_subtitle_stream_with_source::<D>),
        )
        .route(
            "/Videos/{itemId}/Subtitles/{streamIndex}/Stream.{format}",
            get(subtitles::get_subtitle_stream_format::<D>),
        )
        .route(
            "/Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/Stream.{format}",
            get(subtitles::get_subtitle_stream_source_format::<D>),
        )
        .route(
            "/Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/{startPositionTicks}/Stream.{format}",
            get(subtitles::get_subtitle_stream_at::<D>),
        )
        // ── Sessions ──────────────────────────────────────────────
        .route("/Sessions", get(sessions::get_sessions::<D>))
        .route(
            "/Devices",
            get(devices::get_devices::<D>).delete(devices::delete_device),
        )
        .route("/Devices/Info", get(devices::get_device_info::<D>))
        .route("/Devices/Options", get(devices::get_device_options::<D>))
        .route("/Sessions/Logout", post(sessions::logout))
        .route("/Sessions/Capabilities", post(sessions::post_capabilities::<D>))
        .route(
            "/Sessions/Capabilities/Full",
            post(sessions::post_full_capabilities::<D>),
        )
        // ── Playback reporting ──────────────────────────────────
        .route("/Sessions/Playing", post(playback::report_playback_start::<D>))
        .route(
            "/Users/{userId}/PlayStats",
            get(play_history::get_play_stats::<D>),
        )
        .route(
            "/Users/{userId}/PlayHistory",
            get(play_history::get_play_history::<D>),
        )
        .route(
            "/Sessions/Playing/Progress",
            post(playback::report_playback_progress::<D>),
        )
        .route(
            "/Sessions/Playing/Stopped",
            post(playback::report_playback_stopped::<D>),
        )
        .route("/Sessions/Playing/Ping", post(playback::playback_ping::<D>))
        .route("/Sessions/Viewing", post(stubs::no_content_auth))
        .route(
            "/Sessions/{sessionId}/Viewing",
            post(remote_control::send_display_content::<D>),
        )
        // Jellyfin's old play reports, before the JSON bodies above.
        .route(
            "/PlayingItems/{itemId}",
            post(playback::legacy_playing_start::<D>).delete(playback::legacy_playing_stopped::<D>),
        )
        .route(
            "/PlayingItems/{itemId}/Progress",
            post(playback::legacy_playing_progress::<D>),
        )
        .route(
            "/Users/{userId}/PlayingItems/{itemId}",
            post(playback::legacy_user_playing_start::<D>)
                .delete(playback::legacy_user_playing_stopped::<D>),
        )
        .route(
            "/Users/{userId}/PlayingItems/{itemId}/Progress",
            post(playback::legacy_user_playing_progress::<D>),
        )
        // Live TV leftovers: Rustyfin hands out no open tokens, so nothing opens.
        .route("/LiveStreams/Open", post(stubs::problem_not_found_auth))
        .route("/LiveStreams/Close", post(stubs::no_content_auth))
        // ── Remote control ───────────────────────────────────────
        .route(
            "/Sessions/{sessionId}/Playing",
            post(remote_control::send_play_command::<D>),
        )
        .route(
            "/Sessions/{sessionId}/Playing/{command}",
            post(remote_control::send_playstate_command::<D>),
        )
        .route(
            "/Sessions/{sessionId}/Command/{command}",
            post(remote_control::send_general_command::<D>),
        )
        .route(
            "/Sessions/{sessionId}/System/{command}",
            post(remote_control::send_general_command::<D>),
        )
        // A Rustyfin session carries one user (the one Tailscale names), so
        // adding or removing others is a no-op.
        .route(
            "/Sessions/{sessionId}/User/{userId}",
            post(stubs::no_content_auth).delete(stubs::no_content_auth),
        )
        .route(
            "/Sessions/{sessionId}/Command",
            post(remote_control::send_full_general_command::<D>),
        )
        .route(
            "/Sessions/{sessionId}/Message",
            post(remote_control::send_message_command::<D>),
        )
        // ── User data (favorites, played) ───────────────────────
        .route(
            "/UserFavoriteItems/{itemId}",
            post(user_data::mark_favorite::<D>).delete(user_data::unmark_favorite::<D>),
        )
        .route(
            "/UserPlayedItems/{itemId}",
            post(user_data::mark_played::<D>).delete(user_data::mark_unplayed::<D>),
        )
        // Legacy user data routes
        .route(
            "/Users/{userId}/FavoriteItems/{itemId}",
            post(user_data::mark_favorite_legacy::<D>).delete(user_data::unmark_favorite_legacy::<D>),
        )
        .route(
            "/Users/{userId}/PlayedItems/{itemId}",
            post(user_data::mark_played_legacy::<D>).delete(user_data::mark_unplayed_legacy::<D>),
        )
        .route(
            "/UserItems/{itemId}/UserData",
            get(user_data::get_user_item_data::<D>).post(user_data::update_user_item_data::<D>),
        )
        .route(
            "/Users/{userId}/Items/{itemId}/UserData",
            get(user_data::get_user_item_data_legacy::<D>).post(user_data::update_user_item_data_legacy::<D>),
        )
        .route(
            "/UserItems/{itemId}/Rating",
            post(user_data::update_rating::<D>).delete(user_data::delete_rating::<D>),
        )
        .route(
            "/Users/{userId}/Items/{itemId}/Rating",
            post(user_data::update_rating_legacy::<D>).delete(user_data::delete_rating_legacy::<D>),
        )
        // ── Search ──────────────────────────────────────────────
        .route("/Search/Hints", get(search::search_hints::<D>))
        // ── Display Preferences ──────────────────────────────────
        .route(
            "/DisplayPreferences/{displayPreferencesId}",
            get(display_preferences::get_display_preferences::<D>)
                .post(display_preferences::update_display_preferences::<D>),
        )
        // ── TV Shows ─────────────────────────────────────────────
        .route("/Shows/NextUp", get(shows::get_next_up::<D>))
        .route("/Shows/Upcoming", get(shows::get_upcoming::<D>))
        .route("/Shows/{seriesId}/Seasons", get(shows::get_seasons::<D>))
        .route("/Shows/{seriesId}/Episodes", get(shows::get_episodes::<D>))
        // ── Quick Connect ────────────────────────────────────────
        .route("/QuickConnect/Enabled", get(quick_connect::get_enabled))
        .route(
            "/QuickConnect/Initiate",
            post(quick_connect::initiate).get(quick_connect::initiate),
        )
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
            get(configuration::get_configuration::<D>).post(configuration::update_configuration),
        )
        .route(
            "/System/Configuration/MetadataOptions/Default",
            get(configuration::get_default_metadata_options),
        )
        .route(
            "/System/Configuration/{key}",
            get(configuration::get_named_configuration::<D>)
                .post(configuration::update_named_configuration),
        )
        // ── Filters ──────────────────────────────────────────────
        .route("/Items/Filters", get(filters::get_filters_legacy::<D>))
        .route("/Items/Filters2", get(filters::get_filters::<D>))
        // ── Stub endpoints ───────────────────────────────────────
        .route("/Plugins", get(stubs::empty_array_admin))
        .route("/Packages", get(stubs::empty_array_admin))
        .route("/Repositories", get(stubs::empty_array_admin))
        .route(
            "/Items/{itemId}/Intros",
            get(stubs::empty_query_result_auth),
        )
        .route("/Items/{itemId}/LocalTrailers", get(extras::local_trailers::<D>))
        .route(
            "/Users/{userId}/Items/{itemId}/Intros",
            get(stubs::empty_query_result_auth),
        )
        .route(
            "/Users/{userId}/Items/{itemId}/LocalTrailers",
            get(extras::local_trailers_legacy::<D>),
        )
        .route(
            "/Users/{userId}/Items/{itemId}/SpecialFeatures",
            get(extras::special_features_legacy::<D>),
        )
        .route(
            "/Items/{itemId}/SpecialFeatures",
            get(extras::special_features::<D>),
        )
        .route(
            "/Items/{itemId}/RemoteSearch/Subtitles/{subtitleId}",
            get(stubs::search_remote_subtitles).post(stubs::download_remote_subtitles),
        )
        .route("/Items/{itemId}/Similar", get(similar::get_similar::<D>))
        .route("/UserImage", get(users::get_user_image_by_query::<D>))
        .route("/Playback/BitrateTest", get(streaming::bitrate_test))
        .route("/FallbackFont/Fonts", get(stubs::empty_font_list))
        .route("/FallbackFont/Fonts/{name}", get(stubs::problem_not_found_auth))
        .route(
            "/Videos/{itemId}/AdditionalParts",
            get(stubs::empty_query_result_auth),
        )
        .route(
            "/Videos/{itemId}/Trickplay/{width}/tiles.m3u8",
            get(stubs::problem_not_found_auth),
        )
        .route(
            "/Videos/{itemId}/Trickplay/{width}/{index}",
            get(stubs::problem_not_found_auth),
        )
        // Jellyfin answers Similar under each type's path too.
        .route("/Movies/{itemId}/Similar", get(similar::get_similar::<D>))
        .route("/Trailers/{itemId}/Similar", get(similar::get_similar::<D>))
        // No trailer libraries (trailers are a title's extras): always empty.
        .route("/Trailers", get(stubs::empty_query_result_auth))
        .route(
            "/Items/{itemId}/CriticReviews",
            get(stubs::empty_query_result_auth),
        )
        // Rustyfin does not hand out its machine's network addresses.
        .route("/System/WakeOnLanInfo", get(stubs::empty_array_auth))
        .route("/Shows/{seriesId}/Similar", get(similar::get_similar::<D>))
        .route("/Albums/{itemId}/Similar", get(similar::get_similar::<D>))
        .route("/Artists/{name}/Similar", get(similar::get_similar::<D>))
        // ── Instant mixes ──────────────────────────────────────────
        .route("/Items/{itemId}/InstantMix", get(instant_mix::from_item::<D>))
        .route("/Songs/{itemId}/InstantMix", get(instant_mix::from_item::<D>))
        .route("/Albums/{itemId}/InstantMix", get(instant_mix::from_item::<D>))
        .route("/Artists/{name}/InstantMix", get(instant_mix::from_item::<D>))
        .route("/Playlists/{playlistId}/InstantMix", get(instant_mix::from_item::<D>))
        .route("/Artists/InstantMix", get(instant_mix::from_artist_query::<D>))
        .route("/MusicGenres/{name}/InstantMix", get(instant_mix::from_genre_name::<D>))
        .route("/MusicGenres/InstantMix", get(instant_mix::from_genre_id::<D>))
        .route(
            "/Items/{itemId}/ThemeSongs",
            get(theme_media::get_theme_songs::<D>),
        )
        .route(
            "/Items/{itemId}/ThemeVideos",
            get(theme_media::get_theme_videos::<D>),
        )
        .route(
            "/Items/{itemId}/ThemeMedia",
            get(theme_media::get_theme_media::<D>),
        )
        .route("/Items/{itemId}/Ancestors", get(theme_media::get_ancestors::<D>))
        .route("/Audio/{itemId}/Lyrics", get(lyrics::get_lyrics::<D>))
        .route("/MediaSegments/{itemId}", get(lyrics::get_media_segments::<D>))
        .route("/Items/Suggestions", get(suggestions::get_suggestions::<D>))
        .route("/Movies/Recommendations", get(similar::get_recommendations::<D>))
        .route("/Artists", get(artists::get_artists::<D>))
        .route("/Artists/AlbumArtists", get(artists::get_album_artists::<D>))
        .route("/Artists/{name}", get(artists::get_artist_by_name::<D>))
        .route("/Genres", get(genres::get_genres::<D>))
        .route("/Genres/{genreName}", get(genres::get_genre::<D>))
        .route("/Studios", get(studios::get_studios::<D>))
        .route("/Studios/{name}", get(studios::get_studio::<D>))
        .route("/Persons", get(persons::get_persons::<D>))
        .route("/Persons/{name}", get(persons::get_person::<D>))
        // ── Years ──────────────────────────────────────────────────
        .route("/Years", get(years::get_years::<D>))
        .route("/Years/{year}", get(years::get_year::<D>))
        // ── MusicGenres ────────────────────────────────────────────
        .route("/MusicGenres", get(music_genres::get_music_genres::<D>))
        .route(
            "/MusicGenres/{genreName}",
            get(music_genres::get_music_genre::<D>),
        )
        // ── ScheduledTasks (admin) ─────────────────────────────────
        .route("/ScheduledTasks", get(scheduled_tasks::get_tasks::<D>))
        .route("/ScheduledTasks/{taskId}", get(scheduled_tasks::get_task::<D>))
        .route(
            "/ScheduledTasks/Running/{taskId}",
            post(scheduled_tasks::start_task::<D>).delete(scheduled_tasks::stop_task),
        )
        // ── Time sync (no auth) ────────────────────────────────────
        .route("/GetUtcTime", get(stubs::get_utc_time))
        .route(
            "/Users/{userId}/Suggestions",
            get(suggestions::get_suggestions::<D>),
        )
        // ── Playlists ──────────────────────────────────────────────
        .route("/Playlists", post(playlists::create_playlist::<D>))
        .route(
            "/Playlists/{playlistId}",
            get(playlists::get_playlist::<D>).post(playlists::update_playlist::<D>),
        )
        .route(
            "/Playlists/{playlistId}/Users",
            get(playlists::get_playlist_users::<D>),
        )
        .route(
            "/Playlists/{playlistId}/Users/{userId}",
            get(playlists::get_playlist_user::<D>)
                .post(playlists::set_playlist_user::<D>)
                .delete(playlists::remove_playlist_user::<D>),
        )
        .route(
            "/Playlists/{playlistId}/Items",
            get(playlists::get_playlist_items::<D>)
                .post(playlists::add_playlist_items::<D>)
                .delete(playlists::remove_playlist_items::<D>),
        )
        .route(
            "/Playlists/{playlistId}/Items/{itemId}/Move/{newIndex}",
            post(playlists::move_playlist_item::<D>),
        )
        // ── Collections ────────────────────────────────────────────
        .route("/Collections", post(collections::create_collection::<D>))
        .route(
            "/Items/{itemId}/Collections",
            get(collections::get_item_collections::<D>),
        )
        .route(
            "/Collections/{collectionId}/Items",
            post(collections::add_collection_items::<D>).delete(collections::remove_collection_items::<D>),
        )
        // ── WebSocket ────────────────────────────────────────────
        .route("/socket", get(websocket::websocket_handler::<D>))
        // ── State ───────────────────────────────────────────────
        .fallback(unmatched_route)
        .with_state(state);
    // Every matched route is counted, timed and logged by the server.
    observe(routes)
        // Jellyfin clients send camelCase query keys; our extractors expect PascalCase.
        .layer(axum::middleware::from_fn(
            crate::query_case::normalize_query_keys,
        ))
}

/// Requests no route matches. Logged at info with the method and path (never
/// the query, which can carry a token), so a client calling an API Rustyfin
/// lacks shows up in the log instead of only as "not found" in the client.
async fn unmatched_route(
    method: axum::http::Method,
    uri: axum::http::Uri,
) -> axum::http::StatusCode {
    tracing::info!("No route for {method} {}", uri.path());
    axum::http::StatusCode::NOT_FOUND
}
