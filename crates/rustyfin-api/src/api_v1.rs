//! Rustyfin's own API, apart from the Jellyfin routes (owner, 2026-11-19: "I
//! want any new api routes that are rustyfin specific to be isolated from
//! any of the old Jellyfin routes").
//!
//! Everything here is under `/api/v1`: lower-case paths, snake_case JSON and
//! snake_case query keys (the Jellyfin routes' camelCase fix-up,
//! `query_case`, is not applied). Requests sign in the same way as on the
//! Jellyfin routes (a session token from forward auth or LDAPS). A Jellyfin
//! client never calls these; Rustyfin's admin and linked Rustyfin servers do.

use axum::Router;
use axum::routing::{get, post};

use crate::router::AppState;
use crate::routes::{link_catalog, link_relay, links, user_permissions};

/// Where the API is mounted.
pub const PREFIX: &str = "/api/v1";

/// The routes, relative to [`PREFIX`].
pub fn router() -> Router<AppState> {
    Router::new()
        // Linked servers, for the admin (roadmap 16b).
        .route("/links", get(links::list))
        .route("/links/{id}/sync", post(links::sync_now))
        // A linked copy's transcode, from the friend's server.
        .route("/links/{id}/relay/{*path}", get(link_relay::relay_media))
        // The change feed a linked Rustyfin syncs from (roadmap 16b.2r).
        .route("/catalog", get(link_catalog::catalog))
        // What a user may do beyond Jellyfin's policy.
        .route(
            "/users/{id}/permissions",
            get(user_permissions::get).patch(user_permissions::patch),
        )
}
