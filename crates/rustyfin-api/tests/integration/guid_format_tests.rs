//! Owner, 2026-11-22: pressing play on an album in Finamp queued it but did
//! not start, with "FormatException: Invalid radix-16 number (at character
//! 1) b-" from `FinampStorableQueueInfo.packIds`. Finamp packs each id as 16
//! bytes from 32 hex digits, two at a time; Rustyfin wrote ids as
//! hyphenated UUIDs, so a pair landed on a dash. Jellyfin writes every Guid
//! as 32 lowercase hex digits ("N" format).

use crate::common;

use serde_json::Value;

fn hyphenated(v: &Value, path: &str, found: &mut Vec<String>) {
    match v {
        Value::String(s) if uuid::Uuid::try_parse(s).is_ok() && s.contains('-') => {
            found.push(format!("{path} = {s}"))
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                hyphenated(x, &format!("{path}[{i}]"), found);
            }
        }
        Value::Object(o) => {
            for (k, x) in o {
                hyphenated(x, &format!("{path}.{k}"), found);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn ids_are_written_as_32_hex_digits() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("listener", true).await;
    let lib = app.create_library("Music", "music").await;
    let artist = app
        .create_media_item("Band", "MusicArtist", lib, None)
        .await;
    let album = app
        .create_media_item("Record", "MusicAlbum", lib, Some(artist))
        .await;
    app.create_media_item("Song", "Audio", lib, Some(album))
        .await;

    for path in [
        format!("/Users/{user}/Items?ParentId={album}&Fields=MediaSources"),
        format!("/Users/{}/Items?ParentId={}", user.simple(), album.simple()),
        format!("/Items/{album}"),
        format!("/Users/{user}/Views"),
        "/Users/Me".to_string(),
        "/Sessions".to_string(),
        "/System/Info".to_string(),
    ] {
        let (status, body) = app.get(&path, &token).await;
        assert!(status.is_success(), "{path}: {status} {body}");
        let mut found = Vec::new();
        hyphenated(&body, "$", &mut found);
        assert!(found.is_empty(), "{path}: {found:?}");
        assert!(body.to_string().len() > 2, "{path}");
    }
    let (_, items) = app
        .get(&format!("/Users/{user}/Items?ParentId={album}"), &token)
        .await;
    let id = items["Items"][0]["Id"].as_str().unwrap_or_default();
    assert_eq!(id.len(), 32, "{items}");
    assert!(
        id.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "{id}"
    );
    app.cleanup().await;
}
