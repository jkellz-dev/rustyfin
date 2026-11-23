//! Items without artwork get a placeholder primary image.
//!
//! Owner decision (2026-11-14): a placeholder for now, not frames grabbed from
//! the video. Jellyfin clients draw a broken tile, or skip the item in rows, when
//! `ImageTags.Primary` is missing, and they fetch `/Items/{id}/Images/Primary`
//! with that tag. The placeholder is a plain tile in the item's usual shape:
//! a 2:3 poster for films, series and seasons, 16:9 for episodes and videos, a
//! square for music.
use crate::common;

use axum::http::StatusCode;
use serde_json::Value;

const BLURHASH: &str = "005hZ5";

fn first(body: &Value) -> &Value {
    let items = body["Items"].as_array().unwrap_or_else(|| panic!("{body}"));
    assert!(!items.is_empty(), "{body}");
    &items[0]
}

/// The PNG width and height from its IHDR chunk.
fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "not a PNG");
    let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (w, h)
}

#[tokio::test]
async fn an_item_without_art_names_a_placeholder_in_its_shape() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let movies = app.create_library("Movies", "movies").await;
    app.create_media_item("Film", "Movie", movies, None).await;
    let shows = app.create_library("Shows", "tvshows").await;
    let series = app.create_media_item("Show", "Series", shows, None).await;
    let season = app
        .create_media_item("Season 1", "Season", shows, Some(series))
        .await;
    app.create_media_item("Ep", "Episode", shows, Some(season))
        .await;
    let music = app.create_library("Music", "music").await;
    app.create_media_item("Record", "MusicAlbum", music, None)
        .await;

    for (kind, tag, ratio) in [
        ("Movie", "placeholder-poster-1", 2.0 / 3.0),
        ("Series", "placeholder-poster-1", 2.0 / 3.0),
        ("Season", "placeholder-poster-1", 2.0 / 3.0),
        // Episodes name a frame of the video instead (episode_thumb_tests).
        ("Episode", "frame-", 16.0 / 9.0),
        // Music names its tracks' embedded cover instead, which falls back
        // to the square placeholder (embedded_album_art_tests).
        ("MusicAlbum", "cover-", 1.0),
    ] {
        let (status, body) = app
            .get(
                &format!("/Items?Recursive=true&IncludeItemTypes={kind}"),
                &token,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let item = first(&body);
        let got_tag = item["ImageTags"]["Primary"].as_str().unwrap_or_default();
        if tag.ends_with('-') {
            assert!(got_tag.starts_with(tag), "{kind}: {item}");
        } else {
            assert_eq!(got_tag, tag, "{kind}: {item}");
        }
        assert_eq!(
            item["ImageBlurHashes"]["Primary"][got_tag], BLURHASH,
            "{kind}: {item}"
        );
        let got = item["PrimaryImageAspectRatio"].as_f64().unwrap_or(-1.0);
        assert!((got - ratio).abs() < 1e-6, "{kind}: ratio {got}");
    }
    app.cleanup().await;
}

#[tokio::test]
async fn the_placeholder_is_served_at_the_primary_image_route() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    let shows = app.create_library("Shows", "tvshows").await;
    let ep = app.create_media_item("Ep", "Episode", shows, None).await;

    let (status, headers, bytes) = app
        .get_bytes(
            &format!("/Items/{film}/Images/Primary?tag=placeholder-poster-1"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v == "image/png"),
        "{headers:?}"
    );
    let (w, h) = png_size(&bytes);
    assert_eq!(w * 3, h * 2, "poster is 2:3, got {w}x{h}");

    let (status, _, bytes) = app
        .get_bytes(&format!("/Items/{ep}/Images/Primary"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (w, h) = png_size(&bytes);
    assert_eq!(w * 9, h * 16, "episode is 16:9, got {w}x{h}");

    // Resizing goes through the usual pipeline.
    let (status, _, bytes) = app
        .get_bytes(
            &format!("/Items/{film}/Images/Primary?maxWidth=100&format=png"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (w, _) = png_size(&bytes);
    assert!(w <= 100, "maxWidth ignored: {w}");
    app.cleanup().await;
}

#[tokio::test]
async fn real_art_wins_over_the_placeholder() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    let (real_tag,): (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO images (entity_id, image_type, path, image_index) \
         VALUES ($1, 'Primary', '/media/movies/Film/poster.jpg', 0) RETURNING id",
    )
    .bind(film)
    .fetch_one(&app.pool)
    .await
    .expect("image row");

    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Movie", &token)
        .await;
    assert_eq!(
        first(&body)["ImageTags"]["Primary"],
        real_tag.simple().to_string()
    );
    let (_, single) = app.get(&format!("/Items/{film}"), &token).await;
    assert_eq!(
        single["ImageTags"]["Primary"],
        real_tag.simple().to_string()
    );
    app.cleanup().await;
}

#[tokio::test]
async fn an_item_with_only_a_backdrop_still_gets_a_primary_placeholder() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    sqlx::query(
        "INSERT INTO images (entity_id, image_type, path, image_index) \
         VALUES ($1, 'Backdrop', '/media/movies/Film/fanart.jpg', 0)",
    )
    .bind(film)
    .execute(&app.pool)
    .await
    .expect("backdrop row");

    let (_, single) = app.get(&format!("/Items/{film}"), &token).await;
    assert_eq!(
        single["ImageTags"]["Primary"], "placeholder-poster-1",
        "{single}"
    );
    assert_eq!(
        single["BackdropImageTags"].as_array().map(Vec::len),
        Some(1)
    );
    app.cleanup().await;
}

#[tokio::test]
async fn libraries_and_search_hints_carry_the_placeholder_too() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Movies", "movies").await;
    app.create_media_item("Findable", "Movie", lib, None).await;

    let (_, views) = app.get("/UserViews", &token).await;
    let view = first(&views);
    assert_eq!(
        view["ImageTags"]["Primary"], "placeholder-landscape-1",
        "{view}"
    );
    let (status, _, bytes) = app
        .get_bytes(&format!("/Items/{lib}/Images/Primary"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (w, h) = png_size(&bytes);
    assert_eq!(w * 9, h * 16);

    let (_, hints) = app.get("/Search/Hints?searchTerm=Findable", &token).await;
    let hint = &hints["SearchHints"][0];
    assert_eq!(hint["PrimaryImageTag"], "placeholder-poster-1", "{hint}");
    app.cleanup().await;
}

#[tokio::test]
async fn an_unknown_item_still_has_no_image() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let (status, _, _) = app
        .get_bytes(
            &format!("/Items/{}/Images/Primary", uuid::Uuid::new_v4()),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}
