//! Owner (Plezzy, music): an artist showed no albums, and "recently played"
//! listed tracks never played. Jellyfin clients ask for an artist's albums
//! with `ArtistIds` / `AlbumArtistIds` (the MusicArtist item's id, not a
//! person), and for played items with `Filters=IsPlayed`; both were ignored.
use crate::common;

use axum::http::StatusCode;
use rustyfin_db::repo::user_data;
use serde_json::Value;

fn names(body: &Value) -> Vec<String> {
    let mut n: Vec<String> = body["Items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["Name"].as_str().unwrap().to_string())
        .collect();
    n.sort();
    n
}

#[tokio::test]
async fn an_artists_albums_and_tracks_are_found_by_artist_id() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Music", "music").await;
    let abba = app
        .create_media_item("ABBA", "MusicArtist", lib, None)
        .await;
    let arrival = app
        .create_media_item("Arrival", "MusicAlbum", lib, Some(abba))
        .await;
    app.create_media_item("Dancing Queen", "Audio", lib, Some(arrival))
        .await;
    let other = app
        .create_media_item("Queen", "MusicArtist", lib, None)
        .await;
    let opera = app
        .create_media_item("A Night at the Opera", "MusicAlbum", lib, Some(other))
        .await;
    app.create_media_item("Bohemian Rhapsody", "Audio", lib, Some(opera))
        .await;
    // A track on someone else's album that names ABBA as an artist.
    let guest = app
        .create_media_item("Guest Spot", "Audio", lib, Some(opera))
        .await;
    sqlx::query("UPDATE media_items SET artists = ARRAY['Queen', 'ABBA'] WHERE id = $1")
        .bind(guest)
        .execute(&app.pool)
        .await
        .unwrap();

    for key in ["ArtistIds", "AlbumArtistIds", "artistIds"] {
        let (status, body) = app
            .get(
                &format!("/Items?{key}={abba}&IncludeItemTypes=MusicAlbum&Recursive=true&SortBy=ProductionYear,SortName"),
                &token,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(names(&body), ["Arrival"], "{key}");
    }
    let (_, tracks) = app
        .get(
            &format!("/Items?ArtistIds={abba}&IncludeItemTypes=Audio&Recursive=true"),
            &token,
        )
        .await;
    assert_eq!(names(&tracks), ["Dancing Queen", "Guest Spot"]);
    app.cleanup().await;
}

#[tokio::test]
async fn filters_keep_only_played_favourite_or_resumable_items() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Music", "music").await;
    let played = app.create_media_item("Played", "Audio", lib, None).await;
    let fav = app.create_media_item("Favourite", "Audio", lib, None).await;
    let half = app.create_media_item("Half", "Audio", lib, None).await;
    app.create_media_item("Never", "Audio", lib, None).await;
    user_data::upsert(&app.pool, user, played, 0, true, false, 1)
        .await
        .unwrap();
    user_data::upsert(&app.pool, user, fav, 0, false, true, 0)
        .await
        .unwrap();
    user_data::upsert(&app.pool, user, half, 5_000_000, false, false, 0)
        .await
        .unwrap();

    for (filters, want) in [
        ("IsPlayed", vec!["Played"]),
        ("IsFavorite", vec!["Favourite"]),
        ("IsResumable", vec!["Half"]),
        ("IsUnplayed", vec!["Favourite", "Half", "Never"]),
        ("IsPlayed,IsFavorite", vec![]),
    ] {
        let (status, body) = app
            .get(
                &format!(
                    "/Items?ParentId={lib}&IncludeItemTypes=Audio&Recursive=true\
                     &SortBy=DatePlayed&SortOrder=Descending&Filters={filters}"
                ),
                &token,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(names(&body), want, "Filters={filters}");
    }
    app.cleanup().await;
}

/// Plezy lists an album's tracks with `AlbumIds` (Jellyfin's filter): only
/// that album's tracks, in disc/track order, also when they sit in disc folders.
#[tokio::test]
async fn album_ids_list_only_that_albums_tracks() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Music", "music").await;
    let artist = app
        .create_media_item("QOTSA", "MusicArtist", lib, None)
        .await;
    let deaf = app
        .create_media_item("Songs for the Deaf", "MusicAlbum", lib, Some(artist))
        .await;
    let other = app
        .create_media_item("Rated R", "MusicAlbum", lib, Some(artist))
        .await;
    let disc2 = app
        .create_media_item("Disc 2", "Folder", lib, Some(deaf))
        .await;
    for (name, parent, disc, track) in [
        ("The Real Song for the Deaf", deaf, 1, 0),
        ("Millionaire", deaf, 1, 2),
        ("You Think I Ain't Worth a Dollar", deaf, 1, 1),
        ("Bonus", disc2, 2, 1),
        ("Feel Good Hit", other, 1, 1),
    ] {
        let id = app
            .create_media_item(name, "Audio", lib, Some(parent))
            .await;
        sqlx::query(
            "UPDATE media_items SET parent_index_number = $2, index_number = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(disc)
        .bind(track)
        .execute(&app.pool)
        .await
        .unwrap();
    }
    let (status, body) = app
        .get(
            &format!(
                "/Items?AlbumIds={deaf}&IncludeItemTypes=Audio&Recursive=true\
                 &SortBy=ParentIndexNumber,IndexNumber,SortName&SortOrder=Ascending"
            ),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let in_order: Vec<&str> = body["Items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["Name"].as_str().unwrap())
        .collect();
    assert_eq!(
        in_order,
        [
            "The Real Song for the Deaf",
            "You Think I Ain't Worth a Dollar",
            "Millionaire",
            "Bonus"
        ]
    );
    app.cleanup().await;
}

/// Parity suite (music.hurl): Jellyfin's album artists carry ChildCount (their
/// albums; Plezy's badge falls back to it), and an album carries
/// CumulativeRunTimeTicks, its tracks' total, also from disc folders.
#[tokio::test]
async fn artists_count_albums_and_albums_add_up_their_tracks() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Music", "music").await;
    let artist = app
        .create_media_item("Band", "MusicArtist", lib, None)
        .await;
    let one = app
        .create_media_item("One", "MusicAlbum", lib, Some(artist))
        .await;
    app.create_media_item("Two", "MusicAlbum", lib, Some(artist))
        .await;
    let disc = app
        .create_media_item("Disc 2", "Folder", lib, Some(one))
        .await;
    for (name, parent, ticks) in [("a", one, 1_000i64), ("b", one, 2_000), ("c", disc, 3_000)] {
        let id = app
            .create_media_item(name, "Audio", lib, Some(parent))
            .await;
        sqlx::query("UPDATE media_items SET runtime_ticks = $2 WHERE id = $1")
            .bind(id)
            .bind(ticks)
            .execute(&app.pool)
            .await
            .unwrap();
    }

    let (status, body) = app.get("/Artists/AlbumArtists", &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let band = body["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["Name"] == "Band")
        .unwrap_or_else(|| panic!("Band missing: {body}"));
    assert_eq!(band["ChildCount"], 2, "{band}");

    let (status, album) = app.get(&format!("/Items/{one}"), &token).await;
    assert_eq!(status, StatusCode::OK, "{album}");
    assert_eq!(album["CumulativeRunTimeTicks"], 6_000, "{album}");
    app.cleanup().await;
}

/// Plezy's alphabet bar filters the artist list with NameStartsWith (and
/// NameLessThan=A for "#"), on the sort name, within the library it shows.
/// Ignoring them answered every artist, so the jump only redrew the list.
#[tokio::test]
async fn the_alphabet_bar_filters_artists_by_sort_name() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Music", "music").await;
    let other = app.create_library("Other Music", "music").await;
    for (name, sort, library) in [
        ("ABBA", "abba", lib),
        ("The Beatles", "beatles", lib),
        ("Blur", "blur", lib),
        ("Coldplay", "coldplay", lib),
        ("!!!", "!!!", lib),
        ("Bjork", "bjork", other),
    ] {
        let artist = app
            .create_media_item(name, "MusicArtist", library, None)
            .await;
        app.create_media_item(
            &format!("{name} album"),
            "MusicAlbum",
            library,
            Some(artist),
        )
        .await;
        sqlx::query("UPDATE media_items SET sort_name = $2 WHERE id = $1")
            .bind(artist)
            .bind(sort)
            .execute(&app.pool)
            .await
            .unwrap();
    }
    let names = |body: &Value| -> Vec<String> {
        let mut n: Vec<String> = body["Items"]
            .as_array()
            .unwrap_or_else(|| panic!("{body}"))
            .iter()
            .map(|i| i["Name"].as_str().unwrap().to_string())
            .collect();
        n.sort();
        n
    };
    for path in ["/Artists/AlbumArtists", "/Artists"] {
        let (status, b) = app
            .get(&format!("{path}?ParentId={lib}&NameStartsWith=b"), &token)
            .await;
        assert_eq!(status, StatusCode::OK, "{b}");
        assert_eq!(names(&b), ["Blur", "The Beatles"], "{path}: {b}");
        assert_eq!(b["TotalRecordCount"], 2, "{path}: {b}");
        let (_, b) = app
            .get(&format!("{path}?ParentId={lib}&NameLessThan=A"), &token)
            .await;
        assert_eq!(names(&b), ["!!!"], "{path}: {b}");
        let (_, b) = app
            .get(
                &format!("{path}?ParentId={lib}&NameStartsWithOrGreater=C"),
                &token,
            )
            .await;
        assert_eq!(names(&b), ["Coldplay"], "{path}: {b}");
    }
    // The same filters on /Items (Plezy's other libraries).
    let (_, b) = app
        .get(
            &format!(
                "/Items?ParentId={lib}&Recursive=true&IncludeItemTypes=MusicArtist&NameLessThan=A"
            ),
            &token,
        )
        .await;
    assert_eq!(names(&b), ["!!!"], "{b}");
    app.cleanup().await;
}
