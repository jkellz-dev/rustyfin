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
    /// Serving a server that links this one: `pool` leaves out what links
    /// hold, and so does everything else (`link_scope`). Set by
    /// [`build_router`] for its second copy of the routes.
    pub local_only: bool,
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
    pub people_cache_dir: Option<PathBuf>,
    pub scan_in_flight: Arc<RwLock<HashSet<uuid::Uuid>>>,
    pub scan_schedules: serde_json::Value,
    pub user_image_url_template: Option<String>,
    pub task_scheduler: Option<Arc<rustyfin_core::TaskScheduler>>,
    /// Fired by `POST /System/Shutdown` and `/System/Restart`; the server binary
    /// waits on it to begin a graceful exit.
    pub shutdown: Arc<tokio::sync::Notify>,
    /// Failed password logins, per name and per address (roadmap 11e).
    pub login_limiter: Arc<crate::login_limit::LoginLimiter>,
    /// Linked servers, whose copies this server relays (roadmap 16b).
    pub links: Arc<rustyfin_core::link::Links>,
    /// `[playback] prefer_direct_play`: of a title's copies, those the app
    /// can play as is come before better ones that would be converted.
    pub prefer_direct_play: bool,
}

impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

/// Build the complete Axum router with all Jellyfin-compatible API routes.
/// The router. Rustyfin's own routes (`/api/v1`) match in any case: their
/// path is put in lower case before routing, which a layer inside the router
/// would be too late for, so the routes sit behind a fallback that does it.
///
/// A server linking this one is served by a second copy of the routes that
/// sees only this server's own media (`link_scope`).
pub fn build_router(state: AppState) -> Router {
    let local_state = AppState {
        pool: crate::link_scope::local_only_pool(&state.pool),
        local_only: true,
        ..state.clone()
    };
    let scope = crate::link_scope::LinkScope::new(state.pool.clone(), routes(local_state));
    let routes = routes(state);
    Router::new().fallback_service(
        tower::ServiceBuilder::new()
            .layer(axum::middleware::from_fn(crate::api_v1::lower_case_path))
            .layer(axum::middleware::from_fn(crate::route_case::route_case))
            .layer(axum::middleware::from_fn_with_state(
                scope,
                crate::link_scope::dispatch,
            ))
            .service(routes),
    )
}

fn routes(state: AppState) -> Router {
    Router::new()
        // ── System (no auth) ────────────────────────────────────
        .route("/System/Info/Public", get(system::get_public_info))
        .route("/System/Ping", get(system::ping).post(system::ping))
        // ── System (auth required) ──────────────────────────────
        .route("/System/Info", get(system::get_info))
        .route("/System/Endpoint", get(system::get_endpoint))
        .route("/System/Stats", get(system::get_stats))
        .route("/System/Shutdown", post(system::shutdown))
        .route("/System/Restart", post(system::restart))
        .route("/System/Logs", get(system::get_logs))
        .route("/System/Logs/Log", get(system::get_log_file))
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
        .route("/Branding/Css.css", get(branding::get_branding_css))
        .route("/Branding/Splashscreen", get(stubs::problem_not_found))
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
        // ── Auth provider info (admin, stubs) ─────────────────
        .route("/Auth/Providers", get(stubs::empty_array_admin))
        .route(
            "/Auth/PasswordResetProviders",
            get(stubs::empty_array_admin),
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
        .route("/Users/{userId}/Password", post(stubs::no_content_auth))
        .route(
            "/Users/{userId}/Configuration",
            post(users::update_user_configuration),
        )
        .route("/Users/New", post(stubs::create_user_disabled))
        // ── User images ──────────────────────────────────────
        .route(
            "/Users/{userId}/Images/{imageType}",
            get(users::get_user_image)
                .post(stubs::no_content_auth)
                .delete(stubs::no_content_auth),
        )
        .route(
            "/Users/{userId}/Images/{imageType}/{imageIndex}",
            get(users::get_user_image_by_index),
        )
        // ── Media update notifications ─────────────────────────
        .route("/Library/Media/Updated", post(webhooks::post_updated_media))
        // ── Library views ───────────────────────────────────────
        .route("/UserViews", get(libraries::get_user_views))
        .route(
            "/UserViews/GroupingOptions",
            get(libraries::get_user_views_grouping_options),
        )
        .route(
            "/Users/{userId}/GroupingOptions",
            get(libraries::get_grouping_options),
        )
        .route("/Library/PhysicalPaths", get(libraries::get_physical_paths))
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
        .route("/Library/MediaFolders", get(libraries::get_media_folders))
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
        .route(
            "/Library/VirtualFolders/Name",
            post(libraries::rename_virtual_folder),
        )
        .route(
            "/Library/VirtualFolders/Paths",
            post(libraries::add_media_path).delete(libraries::remove_media_path),
        )
        .route(
            "/Library/VirtualFolders/Paths/Update",
            post(libraries::update_media_path),
        )
        .route(
            "/Library/VirtualFolders/LibraryOptions",
            post(libraries::update_library_options),
        )
        // ── Items ───────────────────────────────────────────────
        .route("/Items", get(items::get_items))
        .route("/Items/Counts", get(items::get_item_counts))
        .route("/Items/Latest", get(items::get_latest_items))
        .route("/Items/Root", get(libraries::get_root_folder))
        .route("/Items/{itemId}", get(items::get_item_by_id))
        .route("/Items/{itemId}/Refresh", post(items::refresh_item))
        .route("/UserItems/Resume", get(items::get_resume_items))
        .route(
            "/Users/{userId}/Items/Resume",
            get(items::get_user_resume_items_legacy),
        )
        // Legacy item routes
        .route("/Users/{userId}/Items", get(items::get_user_items_legacy))
        .route(
            "/Users/{userId}/Items/Latest",
            get(items::get_user_latest_items_legacy),
        )
        .route(
            "/Users/{userId}/Items/Root",
            get(libraries::get_root_folder_legacy),
        )
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
        .route("/Items/{itemId}/Images", get(images::get_item_images_list))
        // ── Entity image stubs (genres, studios, persons, artists, music genres) ──
        .route(
            "/Genres/{name}/Images/{imageType}",
            get(genres::get_genre_image),
        )
        .route(
            "/Genres/{name}/Images/{imageType}/{imageIndex}",
            get(genres::get_genre_image_by_index),
        )
        .route(
            "/Studios/{name}/Images/{imageType}",
            get(studios::get_studio_image),
        )
        .route(
            "/Studios/{name}/Images/{imageType}/{imageIndex}",
            get(studios::get_studio_image_by_index),
        )
        .route(
            "/Persons/{name}/Images/{imageType}",
            get(persons::get_person_image),
        )
        .route(
            "/Persons/{name}/Images/{imageType}/{imageIndex}",
            get(persons::get_person_image_by_index),
        )
        .route(
            "/Artists/{name}/Images/{imageType}",
            get(artists::get_artist_image),
        )
        .route(
            "/Artists/{name}/Images/{imageType}/{imageIndex}",
            get(artists::get_artist_image_by_index),
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
            "/Items/{itemId}/File",
            get(streaming::item_file).head(streaming::head_stream),
        )
        .route("/Items/{itemId}/Download", get(streaming::download_item))
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
        .route(
            "/Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/Stream.{format}",
            get(subtitles::get_subtitle_stream_source_format),
        )
        .route(
            "/Videos/{itemId}/{mediaSourceId}/Subtitles/{streamIndex}/{startPositionTicks}/Stream.{format}",
            get(subtitles::get_subtitle_stream_at),
        )
        // ── Sessions ──────────────────────────────────────────────
        .route("/Sessions", get(sessions::get_sessions))
        .route(
            "/Devices",
            get(devices::get_devices).delete(devices::delete_device),
        )
        .route("/Sessions/Logout", post(sessions::logout))
        .route("/Sessions/Capabilities", post(sessions::post_capabilities))
        .route(
            "/Sessions/Capabilities/Full",
            post(sessions::post_full_capabilities),
        )
        // ── Playback reporting ──────────────────────────────────
        .route("/Sessions/Playing", post(playback::report_playback_start))
        .route(
            "/Users/{userId}/PlayStats",
            get(play_history::get_play_stats),
        )
        .route(
            "/Users/{userId}/PlayHistory",
            get(play_history::get_play_history),
        )
        .route(
            "/Sessions/Playing/Progress",
            post(playback::report_playback_progress),
        )
        .route(
            "/Sessions/Playing/Stopped",
            post(playback::report_playback_stopped),
        )
        .route("/Sessions/Playing/Ping", post(playback::playback_ping))
        .route("/Sessions/Viewing", post(stubs::no_content_auth))
        // ── Remote control ───────────────────────────────────────
        .route(
            "/Sessions/{sessionId}/Playing",
            post(remote_control::send_play_command),
        )
        .route(
            "/Sessions/{sessionId}/Playing/{command}",
            post(remote_control::send_playstate_command),
        )
        .route(
            "/Sessions/{sessionId}/Command/{command}",
            post(remote_control::send_general_command),
        )
        .route(
            "/Sessions/{sessionId}/Command",
            post(remote_control::send_full_general_command),
        )
        .route(
            "/Sessions/{sessionId}/Message",
            post(remote_control::send_message_command),
        )
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
            "/UserItems/{itemId}/UserData",
            get(user_data::get_user_item_data).post(user_data::update_user_item_data),
        )
        .route(
            "/Users/{userId}/Items/{itemId}/UserData",
            get(user_data::get_user_item_data_legacy).post(user_data::update_user_item_data_legacy),
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
        .route("/Shows/Upcoming", get(shows::get_upcoming))
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
        .route("/Items/{itemId}/LocalTrailers", get(extras::local_trailers))
        .route(
            "/Users/{userId}/Items/{itemId}/Intros",
            get(stubs::empty_query_result_auth),
        )
        .route(
            "/Users/{userId}/Items/{itemId}/LocalTrailers",
            get(extras::local_trailers_legacy),
        )
        .route(
            "/Users/{userId}/Items/{itemId}/SpecialFeatures",
            get(extras::special_features_legacy),
        )
        .route(
            "/Items/{itemId}/SpecialFeatures",
            get(extras::special_features),
        )
        .route(
            "/Items/{itemId}/RemoteSearch/Subtitles/{subtitleId}",
            get(stubs::search_remote_subtitles).post(stubs::download_remote_subtitles),
        )
        .route("/Items/{itemId}/Similar", get(similar::get_similar))
        .route("/UserImage", get(users::get_user_image_by_query))
        .route("/Playback/BitrateTest", get(streaming::bitrate_test))
        .route("/FallbackFont/Fonts", get(stubs::empty_font_list))
        .route("/FallbackFont/Fonts/{name}", get(stubs::problem_not_found))
        .route(
            "/Videos/{itemId}/AdditionalParts",
            get(stubs::empty_query_result_auth),
        )
        .route(
            "/Videos/{itemId}/Trickplay/{width}/tiles.m3u8",
            get(stubs::problem_not_found),
        )
        .route(
            "/Videos/{itemId}/Trickplay/{width}/{index}",
            get(stubs::problem_not_found),
        )
        // Jellyfin answers Similar under each type's path too.
        .route("/Movies/{itemId}/Similar", get(similar::get_similar))
        .route("/Trailers/{itemId}/Similar", get(similar::get_similar))
        .route("/Shows/{seriesId}/Similar", get(similar::get_similar))
        .route("/Albums/{itemId}/Similar", get(similar::get_similar))
        .route("/Artists/{name}/Similar", get(similar::get_similar))
        // ── Instant mixes ──────────────────────────────────────────
        .route("/Items/{itemId}/InstantMix", get(instant_mix::from_item))
        .route("/Songs/{itemId}/InstantMix", get(instant_mix::from_item))
        .route("/Albums/{itemId}/InstantMix", get(instant_mix::from_item))
        .route("/Artists/{name}/InstantMix", get(instant_mix::from_item))
        .route("/Playlists/{playlistId}/InstantMix", get(instant_mix::from_item))
        .route("/Artists/InstantMix", get(instant_mix::from_artist_query))
        .route("/MusicGenres/{name}/InstantMix", get(instant_mix::from_genre_name))
        .route("/MusicGenres/InstantMix", get(instant_mix::from_genre_id))
        .route(
            "/Items/{itemId}/ThemeSongs",
            get(theme_media::get_theme_songs),
        )
        .route(
            "/Items/{itemId}/ThemeVideos",
            get(theme_media::get_theme_videos),
        )
        .route(
            "/Items/{itemId}/ThemeMedia",
            get(theme_media::get_theme_media),
        )
        .route("/Items/{itemId}/Ancestors", get(theme_media::get_ancestors))
        .route("/Audio/{itemId}/Lyrics", get(lyrics::get_lyrics))
        .route("/MediaSegments/{itemId}", get(lyrics::get_media_segments))
        .route("/Items/Suggestions", get(suggestions::get_suggestions))
        .route("/Movies/Recommendations", get(similar::get_recommendations))
        .route("/Artists", get(artists::get_artists))
        .route("/Artists/AlbumArtists", get(artists::get_album_artists))
        .route("/Artists/{name}", get(artists::get_artist_by_name))
        .route("/Genres", get(genres::get_genres))
        .route("/Genres/{genreName}", get(genres::get_genre))
        .route("/Studios", get(studios::get_studios))
        .route("/Studios/{name}", get(studios::get_studio))
        .route("/Persons", get(persons::get_persons))
        .route("/Persons/{name}", get(persons::get_person))
        // ── Years ──────────────────────────────────────────────────
        .route("/Years", get(years::get_years))
        .route("/Years/{year}", get(years::get_year))
        // ── MusicGenres ────────────────────────────────────────────
        .route("/MusicGenres", get(music_genres::get_music_genres))
        .route(
            "/MusicGenres/{genreName}",
            get(music_genres::get_music_genre),
        )
        // ── ScheduledTasks (admin) ─────────────────────────────────
        .route("/ScheduledTasks", get(scheduled_tasks::get_tasks))
        .route("/ScheduledTasks/{taskId}", get(scheduled_tasks::get_task))
        .route(
            "/ScheduledTasks/Running/{taskId}",
            post(scheduled_tasks::start_task).delete(scheduled_tasks::stop_task),
        )
        // ── Time sync (no auth) ────────────────────────────────────
        .route("/GetUtcTime", get(stubs::get_utc_time))
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
            "/Items/{itemId}/Collections",
            get(collections::get_item_collections),
        )
        .route(
            "/Collections/{collectionId}/Items",
            post(collections::add_collection_items).delete(collections::remove_collection_items),
        )
        // ── Rustyfin's own API, apart from Jellyfin's ─────────────
        .nest(crate::api_v1::PREFIX, crate::api_v1::router())
        // ── WebSocket ────────────────────────────────────────────
        .route("/socket", get(websocket::websocket_handler))
        // ── State ───────────────────────────────────────────────
        // Every matched API route is counted and timed (metrics::track_requests).
        .route_layer(axum::middleware::from_fn(crate::metrics::track_requests))
        .fallback(unmatched_route)
        .with_state(state)
        .layer(axum::middleware::from_fn(crate::request_log::log_request))
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
