//! Which paths are this API's, in any case (owner, 2026-11-19: Rustyfin's
//! routes match in any case, as Jellyfin's do). A path is written in lower
//! case in [`ROUTES`]; a request's path is compared word by word, ignoring
//! case, and an id (a `{...}` in the route) is kept as it came.

/// Where the API is mounted.
pub const PREFIX: &str = "/api/v1";

/// Every route, after [`PREFIX`] (a test holds the router to this list).
pub const ROUTES: &[&str] = &[
    "/links",
    "/links/{id}/sync",
    "/catalog",
    "/users/permissions",
    "/users/{id}/permissions",
];

/// The path as the router knows it (fixed words in lower case) when `path`
/// is one of this API's routes; `None` for any other path.
pub fn canonical(path: &str) -> Option<String> {
    let rest = strip_prefix_ignoring_case(path, PREFIX)?;
    let given: Vec<&str> = rest.split('/').collect();
    ROUTES.iter().find_map(|route| {
        let words: Vec<&str> = route.split('/').collect();
        if words.len() != given.len() {
            return None;
        }
        let mut out = String::from(PREFIX);
        for (word, part) in words.iter().zip(&given) {
            if word.is_empty() {
                if !part.is_empty() {
                    return None;
                }
                continue;
            }
            out.push('/');
            if word.starts_with('{') {
                if part.is_empty() {
                    return None;
                }
                out.push_str(part);
            } else if word.eq_ignore_ascii_case(part) {
                out.push_str(word);
            } else {
                return None;
            }
        }
        Some(out)
    })
}

/// Whether a request for `path` is this API's.
pub fn owns(path: &str) -> bool {
    canonical(path).is_some()
}

fn strip_prefix_ignoring_case<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let head = path.get(..prefix.len())?;
    let rest = &path[prefix.len()..];
    (head.eq_ignore_ascii_case(prefix) && rest.starts_with('/')).then_some(rest)
}

/// Middleware before routing: the request's path as the router knows it.
pub async fn lower_case(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if let Some(path) = canonical(req.uri().path())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Owner, 2026-11-19: "new endpoints must be lowercase".
    #[test]
    fn every_route_is_lower_case() {
        for route in ROUTES {
            let fixed: String = route
                .split('/')
                .filter(|seg| !seg.starts_with('{'))
                .collect::<Vec<_>>()
                .join("/");
            assert_eq!(fixed, fixed.to_lowercase(), "{route} is not lower case");
        }
        assert_eq!(PREFIX, PREFIX.to_lowercase());
    }

    #[test]
    fn paths_match_in_any_case_and_keep_their_ids() {
        assert_eq!(canonical("/API/V1/Links").as_deref(), Some("/api/v1/links"));
        assert_eq!(
            canonical("/Api/v1/Users/ABC/Permissions").as_deref(),
            Some("/api/v1/users/ABC/permissions")
        );
        assert_eq!(
            canonical("/api/v1/users/permissions").as_deref(),
            Some("/api/v1/users/permissions")
        );
        assert_eq!(
            canonical("/api/v1/LINKS/x/Sync").as_deref(),
            Some("/api/v1/links/x/sync")
        );
    }

    #[test]
    fn other_paths_are_not_this_apis() {
        // A linked copy's relay is the Jellyfin side's.
        assert!(!owns("/api/v1/links/x/relay/videos/a/master.m3u8"));
        assert!(!owns("/api/v1/links/"));
        assert!(!owns("/api/v1/links/x"));
        assert!(!owns("/api/v1/nothing"));
        assert!(!owns("/Items/Links"));
        assert!(!owns("/api-docs/v1"));
        assert!(!owns("/apiv1/links"));
        assert!(!owns("/api/v1"));
    }

    /// The router's paths are [`ROUTES`] (rustfmt may put a path on the
    /// next line).
    #[test]
    fn the_router_serves_exactly_the_listed_routes() {
        let source: &'static str = include_str!("router.rs");
        let marker = concat!(".route", "(");
        let mut paths: Vec<&str> = source
            .split(marker)
            .skip(1)
            .filter_map(|rest| rest.trim_start().strip_prefix('"')?.split('"').next())
            .collect();
        paths.sort_unstable();
        let mut routes = ROUTES.to_vec();
        routes.sort_unstable();
        assert_eq!(paths, routes);
    }
}
