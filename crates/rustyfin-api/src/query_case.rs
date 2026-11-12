//! Case-insensitive query-string keys.
//!
//! Jellyfin runs on ASP.NET, which binds query parameter names
//! case-insensitively, and real clients send camelCase (`includeItemTypes`,
//! `searchTerm`, `startIndex`). Our serde query structs use PascalCase, so a
//! camelCase key would otherwise be silently ignored (or rejected when the field
//! is required). This middleware upper-cases the first letter of each key
//! before extraction so every handler sees the canonical PascalCase form.
//!
//! Keys containing `_` (`api_key`) are left alone: they are snake_case by
//! convention and read verbatim by the auth layer.

use axum::{
    extract::Request,
    http::{Uri, uri::PathAndQuery},
    middleware::Next,
    response::Response,
};

/// Upper-case the first ASCII letter of every query key; leave values untouched.
pub fn normalize_query(query: &str) -> String {
    query
        .split('&')
        .map(|pair| {
            let (key, rest) = match pair.find('=') {
                Some(i) => pair.split_at(i),
                None => (pair, ""),
            };
            if key.contains('_') {
                return pair.to_string();
            }
            let mut chars = key.chars();
            match chars.next() {
                Some(first) if first.is_ascii_lowercase() => {
                    format!("{}{}{rest}", first.to_ascii_uppercase(), chars.as_str())
                }
                _ => pair.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Axum middleware applying [`normalize_query`] to the request URI.
pub async fn normalize_query_keys(mut req: Request, next: Next) -> Response {
    if let Some(query) = req.uri().query() {
        let normalized = normalize_query(query);
        if normalized != query {
            let path = req.uri().path();
            if let Ok(pq) = format!("{path}?{normalized}").parse::<PathAndQuery>() {
                let mut parts = req.uri().clone().into_parts();
                parts.path_and_query = Some(pq);
                if let Ok(uri) = Uri::from_parts(parts) {
                    *req.uri_mut() = uri;
                }
            }
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalizes_camel_case_keys() {
        assert_eq!(
            normalize_query("includeItemTypes=Movie&startIndex=5"),
            "IncludeItemTypes=Movie&StartIndex=5"
        );
    }

    #[test]
    fn leaves_pascal_case_keys_alone() {
        assert_eq!(
            normalize_query("Recursive=true&Limit=3"),
            "Recursive=true&Limit=3"
        );
    }

    #[test]
    fn leaves_snake_case_keys_alone() {
        assert_eq!(
            normalize_query("api_key=abc&limit=2"),
            "api_key=abc&Limit=2"
        );
    }

    #[test]
    fn does_not_touch_values() {
        assert_eq!(
            normalize_query("searchTerm=dark+knight&x=a=b"),
            "SearchTerm=dark+knight&X=a=b"
        );
    }

    #[test]
    fn handles_valueless_and_empty_keys() {
        assert_eq!(normalize_query("recursive&&=1"), "Recursive&&=1");
    }

    #[test]
    fn keeps_percent_encoding() {
        assert_eq!(normalize_query("name=A%26B"), "Name=A%26B");
    }
}
