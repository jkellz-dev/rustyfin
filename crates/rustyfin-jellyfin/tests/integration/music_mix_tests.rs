//! Instant mixes and genres on music (parity catalog, 2026-11-22).
//!
//! Finamp's "instant mix" asks `/Items/{id}/InstantMix` (and the per-type
//! forms Jellyfin keeps: songs, albums, artists, playlists, music genres);
//! Rustyfin had none of them. Jellyfin builds a mix from the seed's genres,
//! so tracks keep the genre their tags name, and every item lists its genres
//! as `GenreItems` (name and id), as Jellyfin does.

use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::libraries;
use rustyfin_services::LibraryScanner;
use serde_json::Value;
use uuid::Uuid;

fn ids(body: &Value) -> Vec<String> {
    body["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("no Items: {body}"))
        .iter()
        .map(|i| i["Id"].as_str().unwrap().to_string())
        .collect()
}

async fn set_genres(app: &common::TestApp, id: Uuid, genres: &[&str]) {
    let genres: Vec<String> = genres.iter().map(|g| g.to_string()).collect();
    sqlx::query("UPDATE media_items SET genres = $2 WHERE id = $1")
        .bind(id)
        .bind(&genres)
        .execute(&app.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_mix_plays_the_tracks_that_share_the_seeds_genre() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("listener", true).await;
    let lib = app.create_library("Music", "music").await;
    let artist = app
        .create_media_item("Band", "MusicArtist", lib, None)
        .await;
    let album = app
        .create_media_item("Record", "MusicAlbum", lib, Some(artist))
        .await;
    let one = app
        .create_media_item("One", "Audio", lib, Some(album))
        .await;
    let two = app
        .create_media_item("Two", "Audio", lib, Some(album))
        .await;
    let other = app.create_media_item("Elsewhere", "Audio", lib, None).await;
    let rock = app.create_media_item("Loud", "Audio", lib, None).await;
    for id in [one, two] {
        sqlx::query("UPDATE media_items SET series_id = $2, season_id = $3 WHERE id = $1")
            .bind(id)
            .bind(artist)
            .bind(album)
            .execute(&app.pool)
            .await
            .unwrap();
    }
    set_genres(&app, one, &["Jazz"]).await;
    set_genres(&app, two, &["Jazz"]).await;
    set_genres(&app, other, &["Jazz", "Blues"]).await;
    set_genres(&app, rock, &["Rock"]).await;
    let jazz_id = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"Jazz");

    for path in [
        format!("/Songs/{one}/InstantMix?UserId={user}"),
        format!("/Items/{one}/InstantMix?UserId={user}"),
        format!("/Albums/{album}/InstantMix?UserId={user}"),
        format!("/Artists/{artist}/InstantMix?UserId={user}"),
        format!("/Artists/InstantMix?id={artist}&UserId={user}"),
        format!("/MusicGenres/Jazz/InstantMix?UserId={user}"),
        format!("/MusicGenres/InstantMix?id={jazz_id}&UserId={user}"),
        format!("/items/{one}/instantmix"),
    ] {
        let (status, body) = app.get(&path, &token).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        let got = ids(&body);
        for id in [one, two, other] {
            assert!(
                got.contains(&id.simple().to_string()),
                "{path}: {id} missing from {body}"
            );
        }
        assert!(
            !got.contains(&rock.simple().to_string()),
            "{path}: a rock track in a jazz mix"
        );
        assert_eq!(body["TotalRecordCount"], 3, "{path}");
        let track = &body["Items"][0];
        assert_eq!(track["Type"], "Audio");
        assert!(track["UserData"].is_object(), "{track}");
        assert!(track["GenreItems"].is_array(), "{track}");
    }

    let (status, body) = app
        .get(&format!("/Songs/{one}/InstantMix?Limit=1"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body).len(), 1, "Limit");

    let (status, _) = app
        .get(&format!("/Songs/{}/InstantMix", Uuid::new_v4()), &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn a_mix_leaves_out_libraries_the_user_may_not_see() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", false).await;
    let open = app.create_library("Open", "music").await;
    let closed = app.create_library("Closed", "music").await;
    let seed = app.create_media_item("Seed", "Audio", open, None).await;
    let hidden = app.create_media_item("Hidden", "Audio", closed, None).await;
    set_genres(&app, seed, &["Jazz"]).await;
    set_genres(&app, hidden, &["Jazz"]).await;
    rustyfin_data::repo::users::update_policy(&app.pool, user, false, false)
        .await
        .unwrap();
    rustyfin_data::repo::user_library_access::set_enabled_libraries(&app.pool, user, &[open])
        .await
        .unwrap();
    let (status, body) = app.get(&format!("/Items/{seed}/InstantMix"), &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), vec![seed.simple().to_string()], "{body}");
    let (status, _) = app
        .get(&format!("/Items/{hidden}/InstantMix"), &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

#[tokio::test]
async fn items_list_their_genres_with_ids() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    set_genres(&app, film, &["Horror", "Science Fiction"]).await;
    let (status, body) = app.get(&format!("/Items/{film}"), &token).await;
    assert_eq!(status, StatusCode::OK);
    let items = body["GenreItems"]
        .as_array()
        .unwrap_or_else(|| panic!("{body}"));
    assert_eq!(items.len(), 2, "{body}");
    assert_eq!(items[0]["Name"], "Horror");
    let horror = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"Horror");
    assert_eq!(items[0]["Id"], horror.simple().to_string());
    // The same id the genre list gives.
    let (_, genres) = app.get("/Genres", &token).await;
    let listed = genres["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["Name"] == "Horror")
        .unwrap_or_else(|| panic!("{genres}"));
    assert_eq!(listed["Id"], items[0]["Id"]);
    app.cleanup().await;
}

/// An ID3v2.3 text frame (Latin-1).
fn text_frame(id: &str, value: &str) -> Vec<u8> {
    let mut body = vec![0u8];
    body.extend_from_slice(value.as_bytes());
    let mut frame = id.as_bytes().to_vec();
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&body);
    frame
}

/// A tiny MP3 whose tag names a title and a genre.
fn mp3_with_genre(title: &str, genre: &str) -> Vec<u8> {
    let mut frames = text_frame("TIT2", title);
    frames.extend(text_frame("TCON", genre));
    let size = frames.len() as u32;
    let mut out = b"ID3\x03\x00\x00".to_vec();
    out.extend_from_slice(&[
        ((size >> 21) & 0x7f) as u8,
        ((size >> 14) & 0x7f) as u8,
        ((size >> 7) & 0x7f) as u8,
        (size & 0x7f) as u8,
    ]);
    out.extend(frames);
    for _ in 0..8 {
        let mut frame = vec![0u8; 417];
        frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x64]);
        out.extend(frame);
    }
    out
}

#[tokio::test]
async fn tracks_keep_the_genre_their_tags_name() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("listener", true).await;
    let root = std::env::temp_dir().join(format!("rf-genre-{}", Uuid::new_v4().simple()));
    let album_dir = root.join("Band").join("Record (2020)");
    std::fs::create_dir_all(&album_dir).unwrap();
    std::fs::write(
        album_dir.join("01 Song.mp3"),
        mp3_with_genre("Song", "Jazz"),
    )
    .unwrap();
    let root_s = root.to_string_lossy().to_string();
    let lib = libraries::create(&app.pool, "Music", "music", std::slice::from_ref(&root_s))
        .await
        .unwrap()
        .id;
    let scanner = LibraryScanner::new(app.data.clone(), None);
    scanner
        .scan(lib, std::slice::from_ref(&root_s))
        .await
        .expect("scan");

    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Audio", &token)
        .await;
    let track = &body["Items"][0];
    let id = track["Id"]
        .as_str()
        .unwrap_or_else(|| panic!("{body}"))
        .to_string();
    let (_, item) = app.get(&format!("/Items/{id}"), &token).await;
    assert_eq!(item["Genres"], serde_json::json!(["Jazz"]), "{item}");
    let (_, genres) = app.get("/MusicGenres", &token).await;
    assert_eq!(genres["Items"][0]["Name"], "Jazz", "{genres}");

    // A track scanned before genres were kept gets them on the next scan.
    sqlx::query("UPDATE media_items SET genres = NULL WHERE item_type = 'Audio'")
        .execute(&app.pool)
        .await
        .unwrap();
    scanner
        .scan(lib, std::slice::from_ref(&root_s))
        .await
        .expect("rescan");
    let (_, item) = app.get(&format!("/Items/{id}"), &token).await;
    assert_eq!(
        item["Genres"],
        serde_json::json!(["Jazz"]),
        "after rescan: {item}"
    );
    std::fs::remove_dir_all(&root).ok();
    app.cleanup().await;
}
