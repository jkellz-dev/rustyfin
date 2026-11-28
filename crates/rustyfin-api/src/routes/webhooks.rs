//! `POST /api/v1/webhooks/media`: media managers say a title's files
//! changed (owner, 2026-11-27). One URL for films, shows and music: the
//! payload says which. Its title folder (the film's, the show's or the
//! artist's) is rescanned, or its titles removed when the folder is gone.
//!
//! The payload is the webhook format such programs send: `eventType`, and
//! the title under `movie` (`folderPath`), `series` (`path`) or `artist`
//! (`path`). Other keys are not read. A program's paths may differ from this
//! server's: `[[path_map]]` in `rustyfin.toml` translates them.
//!
//! Signed in by an API key (`[[api_key]]`), not by the proxy: the key goes in
//! `X-Api-Key`, the `api_key` query parameter, or as the password of HTTP
//! basic authentication (the webhook form's username and password).

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use base64::Engine;
use rustyfin_models::data::Data;
use rustyfin_services::scans::Notified;
use serde::Serialize;
use serde_json::Value;

use crate::error::ApiError;
use crate::state::ApiState;

/// What the webhook answers.
#[derive(Debug, Serialize)]
pub struct Answer {
    /// `ok` (a test), `ignored` (nothing changed on disk yet, or no title
    /// folder named) or `rescanning`.
    pub status: &'static str,
    /// The title folders named, as the program sent them.
    pub folders: Vec<String>,
}

/// `POST /api/v1/webhooks/media`
pub async fn media<D: Data>(
    State(state): State<ApiState<D>>,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Answer>), ApiError> {
    let key = key_of(&headers, uri.query()).ok_or(ApiError::Unauthorized)?;
    let holder = state
        .services
        .api_keys()
        .find(&key)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let event = body
        .get("eventType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let folders = title_folders(&body);
    let outcome = state
        .services
        .scans()
        .media_manager_event(event, &folders)
        .await?;
    tracing::info!(
        "Media webhook ({}): {event} {}: {outcome:?}",
        holder.name,
        folders.join(", ")
    );
    let (status, word) = match outcome {
        Notified::Test => (StatusCode::OK, "ok"),
        Notified::Ignored => (StatusCode::OK, "ignored"),
        Notified::Rescanning => (StatusCode::ACCEPTED, "rescanning"),
    };
    Ok((
        status,
        Json(Answer {
            status: word,
            folders,
        }),
    ))
}

/// The key a request carries: `X-Api-Key`, then `api_key` in the query,
/// then a basic or bearer `Authorization`.
fn key_of(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    let given = |v: &str| (!v.trim().is_empty()).then(|| v.trim().to_string());
    if let Some(key) = headers
        .get("X-Api-Key")
        .and_then(|v| v.to_str().ok())
        .and_then(given)
    {
        return Some(key);
    }
    if let Some(key) = query.and_then(|q| {
        url::form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k.eq_ignore_ascii_case("api_key"))
            .and_then(|(_, v)| given(&v))
    }) {
        return Some(key);
    }
    let auth = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, value) = auth.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") {
        return given(value);
    }
    if scheme.eq_ignore_ascii_case("basic") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(value.trim())
            .ok()?;
        let decoded = String::from_utf8(decoded).ok()?;
        let (_, password) = decoded.split_once(':')?;
        return given(password);
    }
    None
}

/// The title folders a payload names: a film's folder, a show's or an
/// artist's.
fn title_folders(body: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for (title, key) in [
        ("movie", "folderPath"),
        ("series", "path"),
        ("artist", "path"),
    ] {
        if let Some(path) = body
            .get(title)
            .and_then(|t| t.get(key))
            .and_then(Value::as_str)
            .filter(|p| !p.trim().is_empty())
        {
            out.push(path.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn the_key_comes_from_the_header_the_query_or_basic_auth() {
        let mut h = HeaderMap::new();
        h.insert("X-Api-Key", HeaderValue::from_static("from-header"));
        assert_eq!(
            key_of(&h, Some("api_key=q")).as_deref(),
            Some("from-header")
        );
        assert_eq!(
            key_of(&HeaderMap::new(), Some("a=1&api_key=from-query")).as_deref(),
            Some("from-query")
        );
        let mut basic = HeaderMap::new();
        // "anyone:s3cret"
        basic.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic YW55b25lOnMzY3JldA=="),
        );
        assert_eq!(key_of(&basic, None).as_deref(), Some("s3cret"));
        let mut bearer = HeaderMap::new();
        bearer.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer tok"),
        );
        assert_eq!(key_of(&bearer, None).as_deref(), Some("tok"));
        assert_eq!(key_of(&HeaderMap::new(), None), None);
    }

    #[test]
    fn a_film_a_show_and_an_artist_name_their_folders() {
        let film = serde_json::json!({
            "eventType": "Download",
            "movie": {"folderPath": "/media/movies/Film (2020)", "title": "Film"},
            "movieFile": {"path": "/media/movies/Film (2020)/Film (2020).mkv"}
        });
        assert_eq!(title_folders(&film), ["/media/movies/Film (2020)"]);
        let show = serde_json::json!({"series": {"path": "/media/tv/Show"}, "episodeFiles": []});
        assert_eq!(title_folders(&show), ["/media/tv/Show"]);
        let music = serde_json::json!({"artist": {"path": "/music/Artist"}});
        assert_eq!(title_folders(&music), ["/music/Artist"]);
        assert!(title_folders(&serde_json::json!({"eventType": "Test"})).is_empty());
    }
}
