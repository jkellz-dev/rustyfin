//! Images are cheap to serve and cache well.
//!
//! Owner (2026-11-14): the admin loading every image at once jammed the server.
//! Tagged image URLs are cached for a year (a new image gets a new tag), every
//! image answers with an ETag and `If-None-Match` gets a 304, a browser's
//! `Accept: image/webp` no longer re-encodes every image (the encoder only does
//! lossless WebP, often larger than the JPEG), tags stay the same across scans
//! unless the file changes, and a request no longer writes the session row each
//! time.
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::libraries;
use rustyfin_media::image_processing::Placeholder;
use rustyfin_services::LibraryScanner;
use std::path::{Path, PathBuf};
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("rustyfin-imgsrv-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).expect("temp root");
        Tree(root)
    }

    fn png(&self, rel: &str) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).expect("dirs");
        let src = Placeholder::Poster
            .ensure_file(&self.0.join(".cache"))
            .expect("png");
        std::fs::copy(src, &p).expect("copy");
        p
    }

    fn file(&self, rel: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).expect("dirs");
        std::fs::write(&p, b"").expect("file");
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

async fn film_with_poster(app: &common::TestApp, poster: &Path) -> Uuid {
    let lib = app.create_library("Movies", "movies").await;
    let film = app.create_media_item("Film", "Movie", lib, None).await;
    sqlx::query(
        "INSERT INTO images (entity_id, image_type, path, image_index) VALUES ($1, 'Primary', $2, 0)",
    )
    .bind(film)
    .bind(poster.to_string_lossy().to_string())
    .execute(&app.pool)
    .await
    .expect("image row");
    film
}

#[tokio::test]
async fn a_tagged_image_is_cached_for_a_year_and_revalidates_with_304() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    let film = film_with_poster(&app, &tree.png("Film/poster.jpg")).await;

    let url = format!("/Items/{film}/Images/Primary?tag=abc&maxWidth=100");
    let (status, headers, body) = app.get_bytes_with(&url, "", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.is_empty());
    let cc = header(&headers, "cache-control").unwrap_or_default();
    assert!(
        cc.contains("max-age=31536000") && cc.contains("immutable"),
        "{cc}"
    );
    let etag = header(&headers, "etag").expect("etag").to_string();

    let (status, headers, body) = app
        .get_bytes_with(&url, "", &[("If-None-Match", &etag)])
        .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);
    assert!(body.is_empty());
    assert_eq!(header(&headers, "etag"), Some(etag.as_str()));

    // Another size is another image.
    let (_, headers, _) = app
        .get_bytes_with(
            &format!("/Items/{film}/Images/Primary?tag=abc&maxWidth=50"),
            "",
            &[],
        )
        .await;
    assert_ne!(header(&headers, "etag"), Some(etag.as_str()));
    app.cleanup().await;
}

#[tokio::test]
async fn an_untagged_image_is_cached_briefly_and_revalidates() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    let film = film_with_poster(&app, &tree.png("Film/poster.jpg")).await;

    let url = format!("/Items/{film}/Images/Primary");
    let (status, headers, _) = app.get_bytes_with(&url, "", &[]).await;
    assert_eq!(status, StatusCode::OK);
    let cc = header(&headers, "cache-control").unwrap_or_default();
    assert!(!cc.contains("immutable"), "{cc}");
    assert!(cc.contains("max-age=3600"), "{cc}");
    let etag = header(&headers, "etag").expect("etag").to_string();
    let (status, _, _) = app
        .get_bytes_with(&url, "", &[("If-None-Match", &etag)])
        .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);
    app.cleanup().await;
}

#[tokio::test]
async fn a_browser_accepting_webp_gets_the_file_as_it_is() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    let poster = tree.png("Film/poster.png");
    let film = film_with_poster(&app, &poster).await;

    let (status, headers, body) = app
        .get_bytes_with(
            &format!("/Items/{film}/Images/Primary"),
            "",
            &[("Accept", "image/avif,image/webp,*/*")],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(header(&headers, "content-type"), Some("image/png"));
    assert_eq!(body, std::fs::read(&poster).unwrap(), "served untouched");
    assert_eq!(
        header(&headers, "content-length"),
        Some(body.len().to_string().as_str())
    );
    app.cleanup().await;
}

async fn series_tag(app: &common::TestApp, token: &str) -> String {
    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Series", token)
        .await;
    body["Items"][0]["ImageTags"]["Primary"]
        .as_str()
        .unwrap_or_else(|| panic!("{body}"))
        .to_string()
}

#[tokio::test]
async fn image_tags_survive_rescans_and_change_when_the_file_does() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    let poster = tree.png("tv/Show (2021)/poster.jpg");
    tree.file("tv/Show (2021)/Season 01/Show - S01E01.mkv");
    let root = tree.0.join("tv").to_string_lossy().to_string();
    let lib = libraries::create(&app.pool, "Shows", "tvshows", std::slice::from_ref(&root))
        .await
        .unwrap()
        .id;
    let scan = || async {
        LibraryScanner::new(app.data.clone(), None)
            .scan(lib, std::slice::from_ref(&root))
            .await
            .expect("scan");
    };
    scan().await;
    let first = series_tag(&app, &token).await;
    scan().await;
    assert_eq!(
        series_tag(&app, &token).await,
        first,
        "a rescan keeps the tag"
    );

    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&poster)
        .unwrap()
        .set_modified(later)
        .unwrap();
    scan().await;
    assert_ne!(
        series_tag(&app, &token).await,
        first,
        "a changed poster gets a new tag"
    );
    app.cleanup().await;
}

/// A logo resized without a named format used to come back as a JPEG on
/// black; it keeps its transparency now. Apps keep a tagged URL's answer for
/// a year, so a logo's tag is not its old one (the image id) and they fetch
/// it again; a poster's tag, whose answer did not change, stays.
#[tokio::test]
async fn a_logos_tag_changed_with_its_transparency() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    let film = film_with_poster(&app, &tree.png("Film/poster.jpg")).await;
    let logo: Uuid = sqlx::query_scalar(
        "INSERT INTO images (entity_id, image_type, path, image_index) \
         VALUES ($1, 'Logo', $2, 0) RETURNING id",
    )
    .bind(film)
    .bind(tree.png("Film/clearlogo.png").to_string_lossy().to_string())
    .fetch_one(&app.pool)
    .await
    .expect("logo row");
    let poster: Uuid =
        sqlx::query_scalar("SELECT id FROM images WHERE entity_id = $1 AND image_type = 'Primary'")
            .bind(film)
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let (status, item) = app
        .get(&format!("/Users/{user}/Items/{film}"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{item}");
    let tag = |kind: &str| {
        item["ImageTags"][kind]
            .as_str()
            .unwrap_or_default()
            .replace('-', "")
    };
    assert_eq!(tag("Primary"), poster.simple().to_string(), "{item}");
    let logo_tag = tag("Logo");
    assert!(!logo_tag.is_empty(), "{item}");
    assert_ne!(logo_tag, logo.simple().to_string(), "{item}");
    app.cleanup().await;
}

#[tokio::test]
async fn requests_write_the_session_at_most_once_a_minute() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let set = |age: &'static str| {
        let pool = app.pool.clone();
        let token = token.clone();
        async move {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE sessions SET last_activity = date_trunc('second', NOW()) - INTERVAL '{age}' \
                 WHERE access_token = $1"
            )))
            .bind(&token)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    let read = || {
        let pool = app.pool.clone();
        let token = token.clone();
        async move {
            let (t,): (chrono::DateTime<chrono::Utc>,) =
                sqlx::query_as("SELECT last_activity FROM sessions WHERE access_token = $1")
                    .bind(&token)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            t
        }
    };

    set("5 seconds").await;
    let before = read().await;
    let (status, _) = app.get("/Users/Me", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read().await, before, "touched 5 s ago: not written again");

    set("10 minutes").await;
    let stale = read().await;
    app.get("/Users/Me", &token).await;
    assert!(read().await > stale, "touched 10 min ago: written");
    app.cleanup().await;
}
