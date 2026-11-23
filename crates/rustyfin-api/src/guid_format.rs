//! Ids as Jellyfin writes them: every Guid as 32 lowercase hex digits, no
//! dashes ("N" format). Clients rely on it: Finamp packs an id into 16 bytes
//! two hex digits at a time, and a dash breaks it (owner, 2026-11-22: an album
//! would queue but not play). Rustyfin's types hold `Uuid`s, which serde writes
//! hyphenated, in many places; rather than mark each, every JSON answer has
//! its strings that are exactly a hyphenated UUID rewritten. Requests take
//! either form (`Uuid` parses both).

use std::borrow::Cow;

use axum::{
    body::Body,
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Layer: rewrite the ids in JSON answers.
pub async fn simple_guids(req: Request, next: Next) -> Response {
    let res = next.run(req).await;
    let json = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("application/json"));
    if !json {
        return res;
    }
    let (mut parts, body) = res.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    match simplify(&bytes) {
        Cow::Borrowed(_) => Response::from_parts(parts, Body::from(bytes)),
        Cow::Owned(out) => {
            parts.headers.remove(header::CONTENT_LENGTH);
            Response::from_parts(parts, Body::from(out))
        }
    }
}

/// `text` (JSON) with every string that is exactly a hyphenated UUID written
/// as 32 lowercase hex digits.
pub fn simplify_str(text: &str) -> Cow<'_, str> {
    match simplify(text.as_bytes()) {
        Cow::Borrowed(_) => Cow::Borrowed(text),
        // Only ASCII hex digits replace ASCII: still UTF-8.
        Cow::Owned(out) => Cow::Owned(String::from_utf8(out).expect("ASCII for ASCII")),
    }
}

fn is_hyphenated(s: &[u8]) -> bool {
    s.len() == 36
        && s.iter().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => *b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

/// As [`simplify_str`], on bytes.
pub fn simplify(json: &[u8]) -> Cow<'_, [u8]> {
    let mut out: Option<Vec<u8>> = None;
    let mut copied = 0;
    let mut i = 0;
    while i + 37 < json.len() {
        if json[i] == b'"'
            && (i == 0 || json[i - 1] != b'\\')
            && json[i + 37] == b'"'
            && is_hyphenated(&json[i + 1..i + 37])
        {
            let o = out.get_or_insert_with(|| Vec::with_capacity(json.len()));
            o.extend_from_slice(&json[copied..=i]);
            o.extend(
                json[i + 1..i + 37]
                    .iter()
                    .filter(|b| **b != b'-')
                    .map(u8::to_ascii_lowercase),
            );
            o.push(b'"');
            i += 38;
            copied = i;
            continue;
        }
        i += 1;
    }
    match out {
        None => Cow::Borrowed(json),
        Some(mut o) => {
            o.extend_from_slice(&json[copied..]);
            Cow::Owned(o)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_uuid_strings_lose_their_dashes() {
        let before = r#"{"Id":"0F8FAD5B-D9CB-469F-A165-70867728950E","Ids":["7c9e6679-7425-40de-944b-e07fc1f90ae7"],"Name":"x"}"#;
        assert_eq!(
            simplify_str(before),
            r#"{"Id":"0f8fad5bd9cb469fa16570867728950e","Ids":["7c9e6679742540de944be07fc1f90ae7"],"Name":"x"}"#
        );
    }

    #[test]
    fn other_strings_are_left_alone() {
        for s in [
            r#"{"Path":"/Items/7c9e6679-7425-40de-944b-e07fc1f90ae7/File"}"#,
            r#"{"Name":"7c9e6679-7425-40de-944b-e07fc1f90ae7 (live)"}"#,
            r#"{"Id":"7c9e6679742540de944be07fc1f90ae7"}"#,
            r#"{"Note":"say \"7c9e6679-7425-40de-944b-e07fc1f90ae7\""}"#,
            "",
            "\"\"",
        ] {
            assert!(matches!(simplify_str(s), Cow::Borrowed(_)), "{s}");
        }
    }
}
