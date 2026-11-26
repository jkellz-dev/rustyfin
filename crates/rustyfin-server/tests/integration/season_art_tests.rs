//! Seasons have covers.
//!
//! Owner (2026-11-14): "seasons should have covers like the tv show itself.
//! Season covers should come from tvdb as well." The server never fetches from
//! providers (design boundary); it reads artwork beside the media. Season art is
//! read from the season folder (`Season 01/poster.jpg`) and from the Kodi /
//! Kodi names in the show folder (`season01-poster.jpg`,
//! `season-specials-poster.jpg`, `-banner`, `-fanart`, `-landscape`), which is
//! also where `rustyfin fetch` saves TVDB's season posters. A season with no
//! cover of its own shows the show's.
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::libraries;
use rustyfin_media::image_processing::Placeholder;
use rustyfin_services::LibraryScanner;
use std::path::PathBuf;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("rustyfin-season-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).expect("temp root");
        Tree(root)
    }

    fn file(&self, rel: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).expect("dirs");
        std::fs::write(&p, b"").expect("file");
    }

    /// A real PNG, so the image route can serve it.
    fn png(&self, rel: &str, shape: Placeholder) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).expect("dirs");
        let src = shape.ensure_file(&self.0.join(".cache")).expect("png");
        std::fs::copy(src, p).expect("copy");
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn scan_tv(app: &common::TestApp, tree: &Tree) {
    let root = tree.0.join("tv").to_string_lossy().to_string();
    let lib = libraries::create(&app.pool, "Shows", "tvshows", std::slice::from_ref(&root))
        .await
        .expect("library")
        .id;
    LibraryScanner::new(app.data.clone(), None)
        .scan(lib, &[root])
        .await
        .expect("scan");
}

async fn season_id(app: &common::TestApp, number: i32) -> Uuid {
    let (id,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM media_items WHERE item_type = 'Season' AND index_number = $1",
    )
    .bind(number)
    .fetch_one(&app.pool)
    .await
    .unwrap_or_else(|e| panic!("season {number}: {e}"));
    id
}

/// (image type, file name) of an item's images.
async fn images_of(app: &common::TestApp, id: Uuid) -> Vec<(String, String)> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT image_type, path FROM images WHERE entity_id = $1 ORDER BY image_type",
    )
    .bind(id)
    .fetch_all(&app.pool)
    .await
    .expect("images");
    rows.into_iter()
        .map(|(t, p)| {
            let name = PathBuf::from(p)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            (t, name)
        })
        .collect()
}

#[tokio::test]
async fn season_art_is_read_from_the_season_folder_and_the_show_folder() {
    let app = common::TestApp::new().await;
    let tree = Tree::new();
    tree.file("tv/Show (2021)/poster.jpg");
    tree.file("tv/Show (2021)/season01-poster.jpg");
    tree.file("tv/Show (2021)/season01-banner.jpg");
    tree.file("tv/Show (2021)/season01-fanart.jpg");
    tree.file("tv/Show (2021)/season01-landscape.jpg");
    tree.file("tv/Show (2021)/season-specials-poster.jpg");
    tree.file("tv/Show (2021)/season02-poster.jpg");
    tree.file("tv/Show (2021)/Season 00/Show - S00E01.mkv");
    tree.file("tv/Show (2021)/Season 01/Show - S01E01.mkv");
    tree.file("tv/Show (2021)/Season 02/poster.jpg");
    tree.file("tv/Show (2021)/Season 02/Show - S02E01.mkv");
    tree.file("tv/Show (2021)/Season 03/Show - S03E01.mkv");
    tree.file("tv/Show (2021)/Season 10/Show - S10E01.mkv");
    tree.file("tv/Show (2021)/season10-poster.png");
    scan_tv(&app, &tree).await;

    let s = |t: &str, n: &str| (t.to_string(), n.to_string());
    assert_eq!(
        images_of(&app, season_id(&app, 1).await).await,
        vec![
            s("Backdrop", "season01-fanart.jpg"),
            s("Banner", "season01-banner.jpg"),
            s("Primary", "season01-poster.jpg"),
            s("Thumb", "season01-landscape.jpg"),
        ]
    );
    assert_eq!(
        images_of(&app, season_id(&app, 0).await).await,
        vec![s("Primary", "season-specials-poster.jpg")]
    );
    assert_eq!(
        images_of(&app, season_id(&app, 2).await).await,
        vec![s("Primary", "poster.jpg")],
        "the season folder's own poster wins"
    );
    assert_eq!(
        images_of(&app, season_id(&app, 10).await).await,
        vec![s("Primary", "season10-poster.png")]
    );
    assert!(images_of(&app, season_id(&app, 3).await).await.is_empty());
    app.cleanup().await;
}

#[tokio::test]
async fn a_season_without_a_cover_shows_the_shows() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.png("tv/Show (2021)/poster.jpg", Placeholder::Poster);
    tree.png("tv/Show (2021)/season01-poster.jpg", Placeholder::Square);
    tree.file("tv/Show (2021)/Season 01/Show - S01E01.mkv");
    tree.file("tv/Show (2021)/Season 02/Show - S02E01.mkv");
    scan_tv(&app, &tree).await;

    let (_, shows) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Series", &token)
        .await;
    let show = &shows["Items"][0];
    let show_tag = show["ImageTags"]["Primary"]
        .as_str()
        .expect("show poster")
        .to_string();
    assert!(!show_tag.starts_with("placeholder"), "{show}");
    let show_id = show["Id"].as_str().unwrap().to_string();

    let (status, body) = app
        .get(
            &format!("/Items?ParentId={show_id}&SortBy=IndexNumber"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let seasons = body["Items"].as_array().expect("seasons");
    assert_eq!(seasons.len(), 2, "{body}");
    let (s1, s2) = (&seasons[0], &seasons[1]);
    assert_eq!(s1["IndexNumber"], 1);

    let own = s1["ImageTags"]["Primary"].as_str().expect("season 1 cover");
    assert_ne!(own, show_tag, "season 1 has its own cover");
    assert_eq!(
        s2["ImageTags"]["Primary"],
        show_tag.as_str(),
        "season 2 shows the show's: {s2}"
    );
    for season in [s1, s2] {
        assert_eq!(
            season["SeriesPrimaryImageTag"],
            show_tag.as_str(),
            "{season}"
        );
    }

    // The season's image route serves the show's poster (2:3), the season's own
    // is the square one.
    let png_size = |b: &[u8]| {
        (
            u32::from_be_bytes(b[16..20].try_into().unwrap()),
            u32::from_be_bytes(b[20..24].try_into().unwrap()),
        )
    };
    let s2_id = s2["Id"].as_str().unwrap();
    let (status, _, bytes) = app
        .get_bytes(&format!("/Items/{s2_id}/Images/Primary"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(png_size(&bytes), Placeholder::Poster.size());
    let s1_id = s1["Id"].as_str().unwrap();
    let (_, _, bytes) = app
        .get_bytes(&format!("/Items/{s1_id}/Images/Primary"), &token)
        .await;
    assert_eq!(png_size(&bytes), Placeholder::Square.size());

    // The single-item answer agrees.
    let (_, single) = app.get(&format!("/Items/{s2_id}"), &token).await;
    assert_eq!(
        single["ImageTags"]["Primary"],
        show_tag.as_str(),
        "{single}"
    );
    app.cleanup().await;
}

/// A season or episode without a logo, backdrop or thumb of its own names
/// its show's, as Jellyfin does (`ParentLogoItemId`, `ParentLogoImageTag`,
/// `ParentBackdropItemId`, `ParentBackdropImageTags`, `ParentThumb...`):
/// apps draw the show's logo over an episode's page (owner, 2026-11-26: most
/// titles had no logo).
#[tokio::test]
async fn seasons_and_episodes_name_their_shows_logo_backdrop_and_thumb() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let tree = Tree::new();
    tree.png("tv/Show (2021)/poster.jpg", Placeholder::Poster);
    tree.png("tv/Show (2021)/clearlogo.png", Placeholder::Square);
    tree.png("tv/Show (2021)/fanart.jpg", Placeholder::Square);
    tree.png("tv/Show (2021)/landscape.jpg", Placeholder::Square);
    tree.png("tv/Show (2021)/season01-fanart.jpg", Placeholder::Square);
    tree.file("tv/Show (2021)/Season 01/Show - S01E01.mkv");
    tree.file("tv/Show (2021)/Season 02/Show - S02E01.mkv");
    scan_tv(&app, &tree).await;

    let (_, shows) = app
        .get("/Items?Recursive=true&IncludeItemTypes=Series", &token)
        .await;
    let show = shows["Items"][0].clone();
    let show_id = show["Id"].as_str().unwrap().to_string();
    let logo = show["ImageTags"]["Logo"]
        .as_str()
        .expect("show logo")
        .to_string();
    let thumb = show["ImageTags"]["Thumb"]
        .as_str()
        .expect("show thumb")
        .to_string();
    let backdrops = show["BackdropImageTags"].clone();
    assert_eq!(backdrops.as_array().map(Vec::len), Some(1), "{show}");

    let (status, episodes) = app
        .get(&format!("/Shows/{show_id}/Episodes?UserId={user}"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{episodes}");
    let episodes = episodes["Items"].as_array().expect("episodes").clone();
    assert_eq!(episodes.len(), 2);
    for ep in &episodes {
        assert_eq!(ep["ParentLogoItemId"], show_id.as_str(), "{ep}");
        assert_eq!(ep["ParentLogoImageTag"], logo.as_str(), "{ep}");
        assert_eq!(ep["ParentBackdropItemId"], show_id.as_str(), "{ep}");
        assert_eq!(ep["ParentBackdropImageTags"], backdrops, "{ep}");
        assert_eq!(ep["ParentThumbItemId"], show_id.as_str(), "{ep}");
        assert_eq!(ep["ParentThumbImageTag"], thumb.as_str(), "{ep}");
    }

    // One item's page says the same.
    let ep_id = episodes[0]["Id"].as_str().unwrap();
    let (_, one) = app
        .get(&format!("/Users/{user}/Items/{ep_id}"), &token)
        .await;
    assert_eq!(one["ParentLogoImageTag"], logo.as_str(), "{one}");

    // A season with a backdrop of its own keeps it; the other borrows the show's.
    let (_, seasons) = app
        .get(&format!("/Shows/{show_id}/Seasons?UserId={user}"), &token)
        .await;
    let seasons = seasons["Items"].as_array().expect("seasons").clone();
    let (s1, s2) = (&seasons[0], &seasons[1]);
    assert_eq!(s1["IndexNumber"], 1, "{s1}");
    assert!(s1.get("ParentBackdropItemId").is_none(), "{s1}");
    assert_eq!(s2["ParentBackdropImageTags"], backdrops, "{s2}");
    for season in [s1, s2] {
        assert_eq!(season["ParentLogoItemId"], show_id.as_str(), "{season}");
    }

    // And the show's logo is served from the id named.
    let (status, _, bytes) = app
        .get_bytes(&format!("/Items/{show_id}/Images/Logo?tag={logo}"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!bytes.is_empty());
    app.cleanup().await;
}
