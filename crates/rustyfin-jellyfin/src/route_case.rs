//! Jellyfin's routes match in any case (ASP.NET routing), and clients rely on
//! it: some ask for `/users/{id}/items`, others for `/Users/{id}/Items`. Axum
//! matches exactly, so before routing a request's path is given the case its
//! route is written in (owner, 2026-11-19: "all routes must be case
//! insensitive"). Rustyfin's own `/api/v1` (`rustyfin-api`) does the same
//! for its routes.
//!
//! The routes are read from `router.rs` itself, so a new route is covered
//! without being listed twice; a test checks every `.route(` is read.

use std::sync::LazyLock;

/// A route's path, by segment: `Some(word)` written out, `None` a parameter
/// (`{itemId}`); `rest` when it ends in a `{*path}` catch-all.
struct Template {
    segments: Vec<Option<&'static str>>,
    rest: bool,
}

/// The paths of the routes in `router.rs`, as written.
fn route_paths() -> Vec<&'static str> {
    let source: &'static str = include_str!("router.rs");
    let marker = concat!(".route", "(");
    source
        .split(marker)
        .skip(1)
        .filter_map(|rest| rest.trim_start().strip_prefix('"')?.split('"').next())
        .collect()
}

static TEMPLATES: LazyLock<Vec<Template>> = LazyLock::new(|| {
    route_paths()
        .into_iter()
        .filter(|p| p.starts_with('/'))
        .map(|path| {
            let mut segments: Vec<Option<&'static str>> = path[1..]
                .split('/')
                .map(|s| (!s.starts_with('{')).then_some(s))
                .collect();
            let rest = path.rsplit('/').next().is_some_and(|s| s.starts_with("{*"));
            if rest {
                segments.pop();
            }
            Template { segments, rest }
        })
        .collect()
});

/// The request path in the case of the route it matches, or `None` when it
/// already is (or matches no route). Of the routes it matches, the one with
/// the most written-out words wins, as in Axum (`/Items/Filters` over
/// `/Items/{itemId}`). Parameters keep their case.
pub fn match_case(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.strip_prefix('/')?.split('/').collect();
    let best = TEMPLATES
        .iter()
        .filter(|t| {
            let fits = if t.rest {
                parts.len() > t.segments.len()
            } else {
                parts.len() == t.segments.len()
            };
            fits && t
                .segments
                .iter()
                .zip(&parts)
                .all(|(seg, part)| seg.is_none_or(|w| w.eq_ignore_ascii_case(part)))
        })
        .max_by_key(|t| t.segments.iter().filter(|s| s.is_some()).count())?;
    let mut changed = false;
    let mut out = String::with_capacity(path.len());
    for (i, part) in parts.iter().enumerate() {
        out.push('/');
        match best.segments.get(i).copied().flatten() {
            Some(word) if word != *part => {
                changed = true;
                out.push_str(word);
            }
            _ => out.push_str(part),
        }
    }
    changed.then_some(out)
}

/// Middleware: [`match_case`] applied to the request path. It has to run
/// before routing, so it wraps the whole router (`router::build_router`).
pub async fn route_case(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if let Some(path) = match_case(req.uri().path()) {
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

    #[test]
    fn every_route_in_the_router_is_read() {
        let source = include_str!("router.rs");
        let written = source.matches(concat!(".route", "(")).count();
        assert_eq!(route_paths().len(), written);
        assert!(written > 100, "{written}");
        assert_eq!(TEMPLATES.len(), written);
    }

    #[test]
    fn a_path_takes_its_routes_case() {
        assert_eq!(
            match_case("/system/info/public").as_deref(),
            Some("/System/Info/Public")
        );
        assert_eq!(
            match_case("/USERS/AbC/ITEMS").as_deref(),
            Some("/Users/AbC/Items"),
            "the id keeps its case"
        );
        assert_eq!(match_case("/System/Info/Public"), None, "already so");
        assert_eq!(match_case("/no/such/route"), None);
    }

    #[test]
    fn written_out_words_win_over_parameters() {
        // Both `/Items/{itemId}` and `/Items/Counts` exist; `counts` is the
        // word, not an item id.
        assert_eq!(
            match_case("/items/counts").as_deref(),
            Some("/Items/Counts")
        );
        assert_eq!(
            match_case("/items/0123abcd").as_deref(),
            Some("/Items/0123abcd")
        );
    }
}
