//! Owner (2026-11-15): use the MusicBrainz ids the files carry. Picard-tagged
//! music names its release, release group, artists and track; the scan stores
//! them as Jellyfin's provider ids (no network lookup) and items link to
//! MusicBrainz in `ExternalUrls`, as Jellyfin does.
use crate::common;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::libraries;
use serde_json::Value;
use uuid::Uuid;

const RELEASE: &str = "11111111-1111-1111-1111-111111111111";
const GROUP: &str = "22222222-2222-2222-2222-222222222222";
const ARTIST: &str = "33333333-3333-3333-3333-333333333333";
const ALBUM_ARTIST: &str = "44444444-4444-4444-4444-444444444444";
const TRACK: &str = "55555555-5555-5555-5555-555555555555";

/// An ID3v2.3 TXXX frame (Latin-1 description and value).
fn txxx(description: &str, value: &str) -> Vec<u8> {
    let mut body = vec![0u8];
    body.extend_from_slice(description.as_bytes());
    body.push(0);
    body.extend_from_slice(value.as_bytes());
    let mut frame = b"TXXX".to_vec();
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&body);
    frame
}

/// A tiny MP3: an ID3v2.3 tag with Picard's MusicBrainz frames, then a few
/// silent MPEG-1 Layer III frames (128 kb/s, 44.1 kHz, 417 bytes each).
fn tagged_mp3() -> Vec<u8> {
    let mut frames = Vec::new();
    for (d, v) in [
        ("MusicBrainz Album Id", RELEASE),
        ("MusicBrainz Release Group Id", GROUP),
        ("MusicBrainz Artist Id", ARTIST),
        ("MusicBrainz Album Artist Id", ALBUM_ARTIST),
        ("MusicBrainz Release Track Id", TRACK),
    ] {
        frames.extend(txxx(d, v));
    }
    let size = frames.len() as u32;
    let syncsafe = [
        ((size >> 21) & 0x7f) as u8,
        ((size >> 14) & 0x7f) as u8,
        ((size >> 7) & 0x7f) as u8,
        (size & 0x7f) as u8,
    ];
    let mut out = b"ID3\x03\x00\x00".to_vec();
    out.extend_from_slice(&syncsafe);
    out.extend(frames);
    for _ in 0..8 {
        let mut frame = vec![0u8; 417];
        frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x64]);
        out.extend(frame);
    }
    out
}

fn ids(item: &Value) -> &Value {
    &item["ProviderIds"]
}

fn has_link(item: &Value, url: &str) -> bool {
    item["ExternalUrls"].as_array().is_some_and(|l| {
        l.iter()
            .any(|u| u["Name"] == "MusicBrainz" && u["Url"] == url)
    })
}

#[tokio::test]
async fn tracks_albums_and_artists_carry_the_files_musicbrainz_ids() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let root = std::env::temp_dir().join(format!("rf-mb-{}", Uuid::new_v4().simple()));
    let album_dir = root.join("Band").join("Record (2020)");
    std::fs::create_dir_all(&album_dir).unwrap();
    let file = album_dir.join("01 Song.mp3");
    std::fs::write(&file, tagged_mp3()).unwrap();
    let tags = rustyfin_core::music_tags::read_tags(&file).expect("the fixture reads");
    assert_eq!(
        tags.musicbrainz_album_id.as_deref(),
        Some(RELEASE),
        "fixture check"
    );

    let root_s = root.to_string_lossy().to_string();
    let lib = libraries::create(&app.pool, "Music", "music", std::slice::from_ref(&root_s))
        .await
        .unwrap()
        .id;
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[root_s])
        .await
        .expect("scan");

    let get_one = |kind: &'static str| {
        let app = &app;
        let token = &token;
        async move {
            let (status, body) = app
                .get(
                    &format!("/Items?Recursive=true&IncludeItemTypes={kind}"),
                    token,
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            let id = body["Items"][0]["Id"]
                .as_str()
                .unwrap_or_else(|| panic!("no {kind}: {body}"))
                .to_string();
            let (_, item) = app.get(&format!("/Items/{id}"), token).await;
            item
        }
    };

    let track = get_one("Audio").await;
    assert_eq!(ids(&track)["MusicBrainzAlbum"], RELEASE, "{track}");
    assert_eq!(ids(&track)["MusicBrainzReleaseGroup"], GROUP, "{track}");
    assert_eq!(ids(&track)["MusicBrainzArtist"], ARTIST, "{track}");
    assert_eq!(
        ids(&track)["MusicBrainzAlbumArtist"],
        ALBUM_ARTIST,
        "{track}"
    );
    assert_eq!(ids(&track)["MusicBrainzTrack"], TRACK, "{track}");

    let album = get_one("MusicAlbum").await;
    assert_eq!(ids(&album)["MusicBrainzAlbum"], RELEASE, "{album}");
    assert_eq!(ids(&album)["MusicBrainzReleaseGroup"], GROUP, "{album}");
    assert_eq!(
        ids(&album)["MusicBrainzAlbumArtist"],
        ALBUM_ARTIST,
        "{album}"
    );
    assert!(
        has_link(
            &album,
            &format!("https://musicbrainz.org/release/{RELEASE}")
        ),
        "{album}"
    );
    assert!(
        has_link(
            &album,
            &format!("https://musicbrainz.org/release-group/{GROUP}")
        ),
        "{album}"
    );

    let artist = get_one("MusicArtist").await;
    assert_eq!(ids(&artist)["MusicBrainzArtist"], ALBUM_ARTIST, "{artist}");
    assert!(
        has_link(
            &artist,
            &format!("https://musicbrainz.org/artist/{ALBUM_ARTIST}")
        ),
        "{artist}"
    );

    let _ = std::fs::remove_dir_all(&root);
    app.cleanup().await;
}
