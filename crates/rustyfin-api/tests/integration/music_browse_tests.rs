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

/// Owner, 2026-11-23: "Loading artists in Finamp seems really slow" (Sofi
/// Tukker; the item lists of the artist page took 1 to 5 s in a library of
/// about 67,000 tracks). The artist filter was checked track by track; it is
/// found once, as a set, with the track artists indexed.
#[tokio::test]
async fn an_artist_page_stays_fast_in_a_large_library() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Music", "music").await;
    let sofi = app
        .create_media_item("Sofi Tukker", "MusicArtist", lib, None)
        .await;
    let album = app
        .create_media_item("Wet Tennis", "MusicAlbum", lib, Some(sofi))
        .await;
    for t in ["Purple Hat", "Summer In New York"] {
        app.create_media_item(t, "Audio", lib, Some(album)).await;
    }
    // 600 other artists of one album of 100 tracks each, one track of which
    // names Sofi Tukker as a guest.
    sqlx::query(
        r#"
        WITH ar AS (
            INSERT INTO media_items (item_type, name, sort_name, library_id)
            SELECT 'MusicArtist', 'Artist ' || g, 'artist ' || g, $1 FROM generate_series(1, 600) g
            RETURNING id, name
        ), al AS (
            INSERT INTO media_items (item_type, name, sort_name, library_id, parent_id)
            SELECT 'MusicAlbum', ar.name || ' LP', lower(ar.name) || ' lp', $1, ar.id FROM ar
            RETURNING id, name
        )
        INSERT INTO media_items (item_type, name, sort_name, library_id, parent_id, artists)
        SELECT 'Audio', al.name || ' ' || g, lower(al.name) || ' ' || g, $1, al.id,
               CASE WHEN al.name = 'Artist 7 LP' AND g = 3 THEN ARRAY['Artist 7', 'Sofi Tukker']
                    ELSE ARRAY[split_part(al.name, ' LP', 1)] END
        FROM al, generate_series(1, 100) g
        "#,
    )
    .bind(lib)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_data (user_id, item_id, play_count, played) \
         SELECT $1, id, 1 + (random() * 20)::int, true FROM media_items \
         WHERE item_type = 'Audio' AND random() < 0.3",
    )
    .bind(uid)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query("ANALYZE media_items")
        .execute(&app.pool)
        .await
        .unwrap();

    // What Finamp's artist page asks for (finamp artist_content_provider.dart,
    // 2026-11-20): within the music library, with its default fields, the
    // albums, the albums it appears on, the five most played tracks of each
    // kind of credit, and the track count.
    let fields =
        "ChildCount,DateCreated,DateLastMediaAdded,Etag,Genres,ParentId,ProviderIds,Tags,SortName";
    let base = format!("/Users/{uid}/Items?ParentId={lib}&Recursive=true");
    let albums = vec!["Wet Tennis"];
    let tracks = vec!["Artist 7 LP 3", "Purple Hat", "Summer In New York"];
    let none: Vec<&str> = Vec::new();
    let asks = [
        (
            format!(
                "{base}&AlbumArtistIds={sofi}&IncludeItemTypes=MusicAlbum&SortBy=PremiereDate,SortName&SortOrder=Ascending&Fields={fields}"
            ),
            albums.clone(),
        ),
        (
            format!(
                "{base}&ArtistIds={sofi}&IncludeItemTypes=MusicAlbum&SortBy=PremiereDate,SortName&SortOrder=Ascending&Fields={fields}"
            ),
            albums.clone(),
        ),
        (
            format!(
                "{base}&AlbumArtistIds={sofi}&IncludeItemTypes=Audio&SortBy=PlayCount&SortOrder=Descending&Limit=5&Fields={fields}"
            ),
            tracks.clone(),
        ),
        (
            format!(
                "{base}&ArtistIds={sofi}&IncludeItemTypes=Audio&SortBy=PlayCount&SortOrder=Descending&Limit=5&Fields={fields}"
            ),
            tracks.clone(),
        ),
        (
            format!("{base}&ArtistIds={sofi}&IncludeItemTypes=Audio&Limit=1&Fields="),
            none.clone(),
        ),
    ];
    let mut slow = Vec::new();
    for (url, want) in &asks {
        // Once to warm the caches, then timed.
        app.get(url, &token).await;
        let started = std::time::Instant::now();
        let (status, body) = app.get(url, &token).await;
        let took = started.elapsed();
        eprintln!("ARTIST-PAGE {took:?} {url}");
        assert_eq!(status, StatusCode::OK, "{body}");
        if want.is_empty() {
            assert_eq!(body["TotalRecordCount"], 3, "{url}: {body}");
        } else {
            let mut got = names(&body);
            got.sort();
            assert_eq!(&got, want, "{url}");
        }
        if took > std::time::Duration::from_millis(250) {
            slow.push(format!("{took:?} {url}"));
        }
    }
    assert!(slow.is_empty(), "over 60,000 tracks: {slow:#?}");
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
