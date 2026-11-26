//! Rustyfin's own API, apart from the Jellyfin routes (owner, 2026-11-19: "I
//! want any new api routes that are rustyfin specific to be isolated from
//! any of the old Jellyfin routes").
//!
//! `/api` is free in Jellyfin: none of Jellyfin 10.10.7's 309 API paths starts
//! with it (its docs are at `/api-docs`, another first segment). Should a
//! Jellyfin release take it, the prefix becomes `/rfapi/v1` (owner,
//! 2026-11-19).
//!
//! Everything here is under `/api/v1`: paths written in lower case (a test
//! below holds every route to it) and matched in any case ([`match_case`]),
//! snake_case JSON and
//! snake_case query keys (the Jellyfin routes' camelCase fix-up,
//! `query_case`, is not applied). Requests sign in the same way as on the
//! Jellyfin routes (a session token from a forward-auth sign-in). A Jellyfin
//! client never calls these; Rustyfin's admin and linked Rustyfin servers do.

use axum::Router;
use axum::routing::{get, post};

use crate::router::AppState;
use crate::routes::{link_catalog, link_relay, links, user_permissions};

/// Where the API is mounted.
pub const PREFIX: &str = "/api/v1";

/// Every fixed word in the routes below (a test checks none is missing).
const WORDS: &[&str] = &["links", "sync", "relay", "catalog", "users", "permissions"];

/// A request path for this API, its fixed words in lower case, so routes
/// match in any case (owner, 2026-11-19), like Jellyfin's ASP.NET routes.
/// Ids keep their case (they parse in any case) and so does everything after
/// `relay`, which is passed on as it came. `None` for any other path.
pub fn match_case(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    let (Some(""), Some(api), Some(version)) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    let prefix: Vec<&str> = PREFIX.trim_start_matches('/').split('/').collect();
    if !api.eq_ignore_ascii_case(prefix[0]) || !version.eq_ignore_ascii_case(prefix[1]) {
        return None;
    }
    let mut out = String::from(PREFIX);
    let mut passing_on = false;
    for part in parts {
        out.push('/');
        let lower = part.to_ascii_lowercase();
        if !passing_on && WORDS.contains(&lower.as_str()) {
            passing_on = lower == "relay";
            out.push_str(&lower);
        } else {
            out.push_str(part);
        }
    }
    Some(out)
}

/// Middleware: [`match_case`] applied to the request path. It has to run
/// before routing, so it wraps the whole router (`router::build_router`).
pub async fn lower_case_path(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if let Some(path) = match_case(req.uri().path())
        && path != req.uri().path()
    {
        let rebuilt = match req.uri().query() {
            Some(q) => format!("{path}?{q}"),
            None => path,
        };
        if let Ok(pq) = rebuilt.parse::<axum::http::uri::PathAndQuery>() {
            let mut parts = req.uri().clone().into_parts();
            parts.path_and_query = Some(pq);
            if let Ok(uri) = axum::http::Uri::from_parts(parts) {
                *req.uri_mut() = uri;
            }
        }
    }
    next.run(req).await
}

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
        .route("/users/permissions", get(user_permissions::list))
        .route(
            "/users/{id}/permissions",
            get(user_permissions::get).patch(user_permissions::patch),
        )
}

#[cfg(test)]
mod tests {
    /// The paths of every `.route(` in this file (rustfmt may put the path
    /// on the next line).
    fn route_paths() -> Vec<&'static str> {
        let source: &'static str = include_str!("api_v1.rs");
        let marker = concat!(".route", "(");
        source
            .split(marker)
            .skip(1)
            .filter_map(|rest| rest.trim_start().strip_prefix('"')?.split('"').next())
            .collect()
    }

    /// Owner, 2026-11-19: "new endpoints must be lowercase".
    #[test]
    fn every_route_here_is_lower_case() {
        let paths = route_paths();
        assert!(paths.len() >= 5, "{paths:?}");
        for path in paths {
            let fixed: String = path
                .split('/')
                .filter(|seg| !seg.starts_with('{'))
                .collect::<Vec<_>>()
                .join("/");
            assert_eq!(fixed, fixed.to_lowercase(), "{path} is not lower case");
        }
        assert_eq!(super::PREFIX, super::PREFIX.to_lowercase());
    }

    #[test]
    fn every_word_in_the_routes_is_known_to_match_case() {
        for path in route_paths() {
            for word in path
                .split('/')
                .filter(|w| !w.is_empty() && !w.starts_with('{'))
            {
                assert!(
                    super::WORDS.contains(&word),
                    "{word} (in {path}) is not in WORDS"
                );
            }
        }
    }

    #[test]
    fn paths_match_in_any_case_but_keep_what_they_pass_on() {
        use super::match_case;
        assert_eq!(
            match_case("/API/V1/Links").as_deref(),
            Some("/api/v1/links")
        );
        assert_eq!(
            match_case("/Api/v1/Users/ABC/Permissions").as_deref(),
            Some("/api/v1/users/ABC/permissions")
        );
        assert_eq!(
            match_case("/api/v1/LINKS/x/RELAY/videos/Ab/Master.m3u8").as_deref(),
            Some("/api/v1/links/x/relay/videos/Ab/Master.m3u8"),
            "after relay, the friend's path as it came"
        );
        assert_eq!(match_case("/Items/Links"), None);
        assert_eq!(match_case("/api-docs/v1"), None);
        assert_eq!(match_case("/apiv1/links"), None);
    }
}
