//! Live, 2026-11-20: after a start, the search sync sent about 1000 changed
//! items every 10 seconds for many minutes, while the scans reported nothing
//! added or updated. A scan of files that did not change must change no item
//! (each change is sent to the search index and to linked servers).
#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;

use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::{libraries, media_streams};
use uuid::Uuid;

/// A tiny MP3: an ID3v2.3 tag with a title and an artist (or none), then a few
/// silent MPEG-1 Layer III frames.
fn mp3(tags: &[(&str, &str)]) -> Vec<u8> {
    let mut frames = Vec::new();
    for (id, value) in tags {
        let mut body = vec![0u8];
        body.extend_from_slice(value.as_bytes());
        frames.extend_from_slice(id.as_bytes());
        frames.extend_from_slice(&(body.len() as u32).to_be_bytes());
        frames.extend_from_slice(&[0, 0]);
        frames.extend(body);
    }
    let mut out = Vec::new();
    if !frames.is_empty() {
        let size = frames.len() as u32;
        out.extend_from_slice(b"ID3\x03\x00\x00");
        out.extend_from_slice(&[
            ((size >> 21) & 0x7f) as u8,
            ((size >> 14) & 0x7f) as u8,
            ((size >> 7) & 0x7f) as u8,
            (size & 0x7f) as u8,
        ]);
        out.extend(frames);
    }
    for _ in 0..8 {
        let mut frame = vec![0u8; 417];
        frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x64]);
        out.extend(frame);
    }
    out
}

async fn changes(app: &common::TestApp) -> BTreeMap<Uuid, String> {
    sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, change_xid::text || ' ' || (to_jsonb(m) - 'change_xid')::text \
         || ' streams=' || (SELECT COUNT(*) FROM media_streams s WHERE s.item_id = m.id) \
         || ' images=' || (SELECT COUNT(*) FROM images i WHERE i.entity_id = m.id) \
         FROM media_items m",
    )
    .fetch_all(&app.pool)
    .await
    .unwrap()
    .into_iter()
    .collect()
}

#[tokio::test]
async fn a_rescan_of_unchanged_music_changes_no_item() {
    let app = common::TestApp::new().await;
    let root = std::env::temp_dir().join(format!("rf-rescan-{}", Uuid::new_v4().simple()));
    let album = root.join("Band").join("Record (2020)");
    let loose = root.join("Band");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(
        album.join("01 Tagged.mp3"),
        mp3(&[("TIT2", "Tagged"), ("TPE1", "Band")]),
    )
    .unwrap();
    std::fs::write(album.join("02 Bare.mp3"), mp3(&[])).unwrap();
    std::fs::write(
        album.join("cover.jpg"),
        b"\xFF\xD8\xFF\xE0not-really-a-jpeg",
    )
    .unwrap();
    std::fs::write(loose.join("Single.mp3"), mp3(&[("TIT2", "Single")])).unwrap();
    std::fs::write(album.join("01 Tagged.lrc"), b"[00:01.00]la").unwrap();

    let root_s = root.to_string_lossy().to_string();
    let lib = libraries::create(&app.pool, "Music", "music", std::slice::from_ref(&root_s))
        .await
        .unwrap()
        .id;
    let scanner = LibraryScanner::new(app.pool.clone(), None);
    scanner
        .scan(lib, std::slice::from_ref(&root_s))
        .await
        .expect("scan");
    // Twice: a first rescan may fill in what the first scan could not.
    scanner
        .scan(lib, std::slice::from_ref(&root_s))
        .await
        .expect("rescan");
    let before = changes(&app).await;
    assert!(before.len() >= 5, "artist, album, three tracks: {before:?}");
    scanner
        .scan(lib, std::slice::from_ref(&root_s))
        .await
        .expect("rescan");
    let after = changes(&app).await;
    let changed: Vec<String> = sqlx::query_scalar::<_, String>(
        "SELECT item_type || ' ' || name FROM media_items WHERE id = ANY($1) ORDER BY name",
    )
    .bind(
        before
            .iter()
            .filter(|(id, xid)| after.get(id) != Some(xid))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
    )
    .fetch_all(&app.pool)
    .await
    .unwrap();
    for (id, row) in &before {
        if after.get(id) != Some(row) {
            eprintln!(
                "BEFORE {row}\nAFTER  {}",
                after.get(id).map_or("gone", |r| r.as_str())
            );
        }
    }
    assert!(changed.is_empty(), "changed by a rescan: {changed:?}");
    std::fs::remove_dir_all(&root).ok();
    app.cleanup().await;
}

#[tokio::test]
async fn a_rescan_of_unchanged_films_and_shows_changes_no_item() {
    let app = common::TestApp::new().await;
    let root = std::env::temp_dir().join(format!("rf-rescan-v-{}", Uuid::new_v4().simple()));
    let film = root.join("films").join("Film (2020)");
    let show = root.join("shows").join("Show").join("Season 01");
    std::fs::create_dir_all(&film).unwrap();
    std::fs::create_dir_all(&show).unwrap();
    std::fs::write(film.join("Film (2020).mkv"), b"").unwrap();
    std::fs::write(
        film.join("Film (2020).en.srt"),
        b"1\n00:00:01,000 --> 00:00:02,000\nhi\n",
    )
    .unwrap();
    std::fs::write(film.join("poster.jpg"), b"\xFF\xD8\xFF\xE0x").unwrap();
    std::fs::write(film.join("Film (2020)-trailer.mkv"), b"").unwrap();
    std::fs::write(show.join("Show S01E01.mkv"), b"").unwrap();
    std::fs::write(
        root.join("shows").join("Show").join("poster.jpg"),
        b"\xFF\xD8\xFF\xE0x",
    )
    .unwrap();

    let mut libs = Vec::new();
    for (name, kind, dir) in [("Movies", "movies", "films"), ("TV", "tvshows", "shows")] {
        let path = root.join(dir).to_string_lossy().to_string();
        let id = libraries::create(&app.pool, name, kind, std::slice::from_ref(&path))
            .await
            .unwrap()
            .id;
        libs.push((id, path));
    }
    let scanner = LibraryScanner::new(app.pool.clone(), None);
    for _ in 0..2 {
        for (id, path) in &libs {
            scanner
                .scan(*id, std::slice::from_ref(path))
                .await
                .expect("scan");
        }
    }
    let before = changes(&app).await;
    for (id, path) in &libs {
        scanner
            .scan(*id, std::slice::from_ref(path))
            .await
            .expect("rescan");
    }
    let after = changes(&app).await;
    let changed: Vec<Uuid> = before
        .iter()
        .filter(|(id, xid)| after.get(id) != Some(xid))
        .map(|(id, _)| *id)
        .collect();
    assert!(changed.is_empty(), "changed by a rescan: {changed:?}");
    std::fs::remove_dir_all(&root).ok();
    app.cleanup().await;
}

/// A subtitle file beside a video is a stream ffprobe never reads, so it has
/// no time base; it does not make the video look scanned before the details
/// existed (which re-probed it, and changed it, on every scan).
#[tokio::test]
async fn a_subtitle_file_does_not_make_a_video_look_stale() {
    let app = common::TestApp::new().await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, time_base) \
         VALUES ($1, 0, 'Video', 'h264', '1/24000')",
    )
    .bind(film)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, is_external, path) \
         VALUES ($1, 1, 'Subtitle', 'srt', true, '/m/Film.en.srt')",
    )
    .bind(film)
    .execute(&app.pool)
    .await
    .unwrap();
    assert!(!media_streams::lacks_details(&app.pool, film).await.unwrap());
    app.cleanup().await;
}
