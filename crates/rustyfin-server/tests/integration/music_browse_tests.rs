//! Owner (Plezzy, music): an artist showed no albums, and "recently played"
//! listed tracks never played. Jellyfin clients ask for an artist's albums
//! with `ArtistIds` / `AlbumArtistIds` (the MusicArtist item's id, not a
//! person), and for played items with `Filters=IsPlayed`; both were ignored.
use crate::common;

use axum::http::StatusCode;
use rustyfin_data::repo::{libraries, user_data};
use rustyfin_models::data::{Data, MediaItemStore};
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

/// Owner, 2026-11-23: "Let's look at the artists list. If we can speed it up
/// we should." Finamp's artist list (`/Artists/AlbumArtists`, 0.33 s live)
/// over 3,000 artists, 6,000 albums and 60,000 tracks, timed whole and by
/// stage.
#[tokio::test]
async fn the_artist_list_stays_fast_in_a_large_library() {
    use rustyfin_data::repo::{images, media_items};
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Music", "music").await;
    sqlx::query(
        r#"
        WITH ar AS (
            INSERT INTO media_items (item_type, name, sort_name, library_id, is_folder)
            SELECT 'MusicArtist', 'Artist ' || g, 'artist ' || lpad(g::text, 5, '0'), $1, true
            FROM generate_series(1, 3000) g
            RETURNING id, name
        ), al AS (
            INSERT INTO media_items (item_type, name, sort_name, library_id, parent_id, is_folder)
            SELECT 'MusicAlbum', ar.name || ' LP ' || n, lower(ar.name) || ' lp ' || n, $1, ar.id, true
            FROM ar, generate_series(1, 2) n
            RETURNING id, name, parent_id
        )
        INSERT INTO media_items (item_type, name, sort_name, library_id, parent_id, series_id,
                                 artists, runtime_ticks)
        SELECT 'Audio', al.name || ' ' || g, lower(al.name) || ' ' || g, $1, al.id, al.parent_id,
               ARRAY[split_part(al.name, ' LP', 1)], 2000000000
        FROM al, generate_series(1, 10) g
        "#,
    )
    .bind(lib)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_data (user_id, item_id, play_count, played) \
         SELECT $1, id, 1, true FROM media_items WHERE item_type = 'Audio' AND random() < 0.3",
    )
    .bind(uid)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query("ANALYZE").execute(&app.pool).await.unwrap();

    let fields =
        "ChildCount,DateCreated,DateLastMediaAdded,Etag,Genres,ParentId,ProviderIds,Tags,SortName";
    let url = format!(
        "/Artists/AlbumArtists?ParentId={lib}&Recursive=true&SortBy=SortName&SortOrder=Ascending&StartIndex=0&Limit=100&UserId={uid}&Fields={fields}&EnableUserData=true"
    );
    app.get(&url, &token).await;
    let started = std::time::Instant::now();
    let (status, body) = app.get(&url, &token).await;
    let whole = started.elapsed();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["TotalRecordCount"], 3000, "{body}");
    assert_eq!(body["Items"].as_array().unwrap().len(), 100);
    assert_eq!(body["Items"][0]["Name"], "Artist 1");

    // The stages, as the route runs them.
    let filter = media_items::ArtistFilter {
        search: None,
        library_id: Some(lib),
        name_starts_with: None,
        name_less_than: None,
        name_starts_with_or_greater: None,
        album_artists_only: true,
        genres_any: None,
        ranked: None,
    };
    let t = std::time::Instant::now();
    let (rows, _) = media_items::list_artists(&app.pool, &filter, 0, 100)
        .await
        .unwrap();
    let list = t.elapsed();
    let ids: Vec<uuid::Uuid> = rows.iter().map(|r| r.id).collect();
    let t = std::time::Instant::now();
    images::list_by_items(&app.pool, &ids).await.unwrap();
    let imgs = t.elapsed();
    let t = std::time::Instant::now();
    media_items::count_albums_and_songs_by_artists(&app.pool, &ids)
        .await
        .unwrap();
    let counts = t.elapsed();
    let t = std::time::Instant::now();
    media_items::runtime_totals(&app.pool, &ids).await.unwrap();
    let runtimes = t.elapsed();
    let t = std::time::Instant::now();
    user_data::folder_progress(&app.pool, uid, &ids)
        .await
        .unwrap();
    let progress = t.elapsed();
    eprintln!(
        "ARTIST-LIST whole {whole:?}: list {list:?}, images {imgs:?}, counts {counts:?}, runtimes {runtimes:?}, progress {progress:?}"
    );
    assert!(
        whole < std::time::Duration::from_millis(150),
        "the artist list took {whole:?}: list {list:?}, images {imgs:?}, counts {counts:?}, runtimes {runtimes:?}, progress {progress:?}"
    );
    app.cleanup().await;
}

/// Owner, 2026-11-23: Finamp's genre tab said "Unsupported operation:
/// Unexpected BaseItemDto type: Genre". Finamp asks `/Genres` with the music
/// library as `ParentId`; Jellyfin answers that with music genres
/// (`GenresController`: a music or music-videos library gives
/// `GetMusicGenres`), and Finamp opens only a `MusicGenre`.
#[tokio::test]
async fn genres_of_a_music_library_are_music_genres() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let music = app.create_library("Music", "music").await;
    let films = app.create_library("Movies", "movies").await;
    let track = app
        .create_media_item("Purple Hat", "Audio", music, None)
        .await;
    let film = app.create_media_item("Heat", "Movie", films, None).await;
    for (id, genre) in [(track, "Electronic"), (film, "Crime")] {
        sqlx::query("UPDATE media_items SET genres = ARRAY[$2] WHERE id = $1")
            .bind(id)
            .bind(genre)
            .execute(&app.pool)
            .await
            .unwrap();
    }

    let (status, body) = app
        .get(
            &format!("/Genres?ParentId={music}&SortBy=SortName&SortOrder=Ascending&Recursive=true"),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["Items"].as_array().unwrap_or_else(|| panic!("{body}"));
    assert_eq!(items.len(), 1, "{body}");
    assert_eq!(items[0]["Name"], "Electronic");
    assert_eq!(items[0]["Type"], "MusicGenre", "{body}");
    assert_eq!(items[0]["SongCount"], 1, "{body}");
    assert_eq!(items[0]["ChildCount"], 1, "{body}");

    // A films library keeps its genres.
    let (_, body) = app.get(&format!("/Genres?ParentId={films}"), &token).await;
    assert_eq!(body["Items"][0]["Name"], "Crime", "{body}");
    assert_eq!(body["Items"][0]["Type"], "Genre", "{body}");
    app.cleanup().await;
}

/// Owner, 2026-11-24: "the api error". Opening a genre in Finamp fetches it by
/// id (`/Users/{userId}/Items/{genre id}`, a trace), and Rustyfin answered
/// 404: a genre is no stored item. Jellyfin answers the genre itself; a
/// music genre is a MusicGenre, a film genre a Genre.
#[tokio::test]
async fn a_genre_opens_by_id() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let music = app.create_library("Music", "music").await;
    let films = app.create_library("Movies", "movies").await;
    let track = app
        .create_media_item("Purple Hat", "Audio", music, None)
        .await;
    let film = app.create_media_item("Heat", "Movie", films, None).await;
    for (id, genre) in [(track, "Electronic"), (film, "Crime")] {
        sqlx::query("UPDATE media_items SET genres = ARRAY[$2] WHERE id = $1")
            .bind(id)
            .bind(genre)
            .execute(&app.pool)
            .await
            .unwrap();
    }
    for (genre, kind) in [("Electronic", "MusicGenre"), ("Crime", "Genre")] {
        let id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, genre.as_bytes()).simple();
        for path in [format!("/Users/{user}/Items/{id}"), format!("/Items/{id}")] {
            let (status, body) = app.get(&path, &token).await;
            assert_eq!(status, StatusCode::OK, "{path}: {body}");
            assert_eq!(body["Name"], genre, "{body}");
            assert_eq!(body["Type"], kind, "{body}");
        }
    }
    let nowhere = uuid::Uuid::new_v4().simple();
    let (status, _) = app
        .get(&format!("/Users/{user}/Items/{nowhere}"), &token)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    app.cleanup().await;
}

/// Owner, 2026-11-24: Finamp's recently played tracks took 1.7 s (a trace):
/// `SortBy=DatePlayed,SortName` descending sorted every track in the library.
/// Played tracks now come first by date, then the rest by name, page by page
/// across the boundary, with the same total; a favourite filter still works.
#[tokio::test]
async fn recently_played_pages_across_played_and_unplayed() {
    let app = common::TestApp::new().await;
    let (user, token) = app.create_user("viewer", true).await;
    let music = app.create_library("Music", "music").await;
    let mut ids = std::collections::HashMap::new();
    for name in ["Alpha", "Bravo", "Charlie", "Delta", "Echo"] {
        ids.insert(
            name,
            app.create_media_item(name, "Audio", music, None).await,
        );
    }
    // Played: Delta (newest), then Bravo; Echo has user data but no play date.
    for (name, minutes_ago) in [("Delta", 1), ("Bravo", 5)] {
        sqlx::query(
            "INSERT INTO user_data (user_id, item_id, played, play_count, last_played_date) \
             VALUES ($1, $2, true, 1, now() - make_interval(mins => $3))",
        )
        .bind(user)
        .bind(ids[name])
        .bind(minutes_ago)
        .execute(&app.pool)
        .await
        .unwrap();
    }
    sqlx::query("INSERT INTO user_data (user_id, item_id, is_favorite) VALUES ($1, $2, true)")
        .bind(user)
        .bind(ids["Echo"])
        .execute(&app.pool)
        .await
        .unwrap();

    let page = |start: i64, limit: i64, extra: &'static str| {
        let app = &app;
        let token = &token;
        async move {
            let (status, body) = app
                .get(
                    &format!(
                        "/Users/{user}/Items?ParentId={music}&IncludeItemTypes=Audio&Recursive=true\
                         &SortBy=DatePlayed,SortName&SortOrder=Descending&StartIndex={start}&Limit={limit}{extra}"
                    ),
                    token,
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            let names: Vec<String> = body["Items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["Name"].as_str().unwrap().to_string())
                .collect();
            (names, body["TotalRecordCount"].as_i64().unwrap())
        }
    };
    // Played by date (newest first), then the rest by name, descending.
    let all = ["Delta", "Bravo", "Echo", "Charlie", "Alpha"];
    let (names, total) = page(0, 10, "").await;
    assert_eq!(names, all);
    assert_eq!(total, 5);
    let (first, _) = page(0, 3, "").await;
    let (second, total) = page(3, 3, "").await;
    assert_eq!([first, second].concat(), all);
    assert_eq!(total, 5);
    let (one, _) = page(1, 2, "").await;
    assert_eq!(one, ["Bravo", "Echo"]);
    // A filter on user data keeps working.
    let (favs, total) = page(0, 10, "&IsFavorite=true").await;
    assert_eq!((favs, total), (vec!["Echo".to_string()], 1));
    app.cleanup().await;
}

/// Owner, 2026-11-24: Finamp's "Abstract Hip Hop" page counted every album,
/// track and artist in the library. Finamp opens a genre with `GenreIds`
/// (the genre's id) on `/Items`, `/Artists` and `/Artists/AlbumArtists`,
/// and Rustyfin ignored it. An album or artist is in a genre when it or one
/// of its tracks is (Jellyfin: any of the ids given).
#[tokio::test]
async fn a_genre_page_lists_only_that_genre() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let music = app.create_library("Music", "music").await;
    let mut albums = Vec::new();
    for (artist, album, track, genre) in [
        (
            "Madlib",
            "Shades of Blue",
            "Slim's Return",
            "Abstract Hip Hop",
        ),
        ("Pink Floyd", "p.u.l.s.e", "Shine On", "Progressive Rock"),
    ] {
        let a = app
            .create_media_item(artist, "MusicArtist", music, None)
            .await;
        let al = app
            .create_media_item(album, "MusicAlbum", music, Some(a))
            .await;
        let t = app.create_media_item(track, "Audio", music, Some(al)).await;
        sqlx::query("UPDATE media_items SET genres = ARRAY[$2], series_id = $3 WHERE id = $1")
            .bind(t)
            .bind(genre)
            .bind(a)
            .execute(&app.pool)
            .await
            .unwrap();
        albums.push(al);
    }
    let genre = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"Abstract Hip Hop");
    let g = genre.simple();
    let names = |body: &serde_json::Value| -> Vec<String> {
        body["Items"]
            .as_array()
            .unwrap_or_else(|| panic!("{body}"))
            .iter()
            .map(|i| i["Name"].as_str().unwrap().to_string())
            .collect()
    };

    for (kind, want) in [("Audio", "Slim's Return"), ("MusicAlbum", "Shades of Blue")] {
        let (status, body) = app
            .get(
                &format!(
                    "/Items?ParentId={music}&GenreIds={g}&IncludeItemTypes={kind}&Recursive=true"
                ),
                &token,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(names(&body), [want], "{kind}: {body}");
        assert_eq!(body["TotalRecordCount"], 1, "{kind}: {body}");
    }
    for route in ["/Artists", "/Artists/AlbumArtists"] {
        let (status, body) = app
            .get(&format!("{route}?ParentId={music}&GenreIds={g}"), &token)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(names(&body), ["Madlib"], "{route}: {body}");
    }
    // An id that names no genre finds nothing, not everything.
    let nowhere = uuid::Uuid::new_v4().simple();
    let (_, body) = app
        .get(
            &format!(
                "/Items?ParentId={music}&GenreIds={nowhere}&IncludeItemTypes=Audio&Recursive=true"
            ),
            &token,
        )
        .await;
    assert_eq!(body["TotalRecordCount"], 0, "{body}");
    app.cleanup().await;
}

/// Owner, 2026-11-23: "regarding genres for music, there are no images in
/// the genres, its just blank." A music genre's picture is the cover of one
/// of its albums (one with its own artwork first), as Finamp asks for it:
/// `ImageTags.Primary` on the genre, then `/Items/{genre id}/Images/Primary`.
#[tokio::test]
async fn music_genres_have_pictures() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let music = app.create_library("Music", "music").await;
    let artist = app
        .create_media_item("Sofi Tukker", "MusicArtist", music, None)
        .await;
    let album = app
        .create_media_item("Wet Tennis", "MusicAlbum", music, Some(artist))
        .await;
    let track = app
        .create_media_item("Purple Hat", "Audio", music, Some(album))
        .await;
    sqlx::query("UPDATE media_items SET genres = ARRAY['Electronic'] WHERE id = $1")
        .bind(track)
        .execute(&app.pool)
        .await
        .unwrap();
    // A real PNG (the poster placeholder), copied in as the album's cover.
    let dir = std::env::temp_dir().join(format!("rustyfin-cover-{}", album.simple()));
    let cover = dir.join("cover.png");
    let drawn = rustyfin_media::image_processing::Placeholder::Poster
        .ensure_file(&dir.join(".cache"))
        .unwrap();
    std::fs::copy(drawn, &cover).unwrap();
    sqlx::query("INSERT INTO images (entity_id, image_type, path, image_index) VALUES ($1, 'Primary', $2, 0)")
        .bind(album)
        .bind(cover.to_string_lossy().to_string())
        .execute(&app.pool)
        .await
        .unwrap();

    let (_, body) = app
        .get(&format!("/Genres?ParentId={music}&Recursive=true"), &token)
        .await;
    let genre = &body["Items"][0];
    assert_eq!(genre["Name"], "Electronic", "{body}");
    assert!(
        genre["ImageTags"]["Primary"].is_string(),
        "a picture: {body}"
    );
    let id = genre["Id"].as_str().unwrap();
    let (status, _) = app
        .get(&format!("/Items/{id}/Images/Primary?maxWidth=200"), &token)
        .await;
    assert_eq!(status, StatusCode::OK);
    std::fs::remove_dir_all(&dir).ok();
    app.cleanup().await;
}

/// The genres already stored as ID3 codes are named by a migration (scans
/// name new ones); run again here over a track stored the old way.
#[tokio::test]
async fn stored_id3_genre_codes_are_named() {
    let app = common::TestApp::new().await;
    let music = app.create_library("Music", "music").await;
    let track = app.create_media_item("Song", "Audio", music, None).await;
    sqlx::query(
        "UPDATE media_items SET genres = ARRAY['17', '(20)', '(9)Metal', '4000', 'Rock'] WHERE id = $1",
    )
    .bind(track)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../../migrations/20261124010000_id3_genre_names.sql"
    ))
    .execute(&app.pool)
    .await
    .unwrap();
    let genres: Vec<String> = sqlx::query_scalar("SELECT genres FROM media_items WHERE id = $1")
        .bind(track)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(genres, ["Rock", "Alternative", "Metal"]);
    app.cleanup().await;
}

/// A music genre list kept to answer fast is good until the next scan
/// finishes, not five minutes: a fresh server listed none right after its
/// first scan (the parity run of 2026-11-26).
#[tokio::test]
async fn a_scan_shows_its_new_music_genres_at_once() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let music = app.create_library("Music", "music").await;
    let names = |body: &serde_json::Value| -> Vec<String> {
        body["Items"]
            .as_array()
            .unwrap_or_else(|| panic!("{body}"))
            .iter()
            .map(|g| g["Name"].as_str().unwrap_or_default().to_string())
            .collect()
    };
    let (_, before) = app.get("/MusicGenres", &token).await;
    assert!(names(&before).is_empty(), "{before}");

    let track = app
        .create_media_item("Purple Hat", "Audio", music, None)
        .await;
    sqlx::query("UPDATE media_items SET genres = ARRAY['Electronic'] WHERE id = $1")
        .bind(track)
        .execute(&app.pool)
        .await
        .unwrap();
    libraries::mark_scanned(&app.pool, music, std::time::Duration::from_secs(1))
        .await
        .unwrap();
    let (_, after) = app.get("/MusicGenres", &token).await;
    assert_eq!(names(&after), ["Electronic"], "{after}");
    app.cleanup().await;
}

/// An album's genres are its tracks', as in Jellyfin (an album page lists
/// them; the parity run of 2026-11-26 found albums with none).
#[tokio::test]
async fn an_album_takes_its_tracks_genres() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let music = app.create_library("Music", "music").await;
    let album = app
        .create_media_item("Album", "MusicAlbum", music, None)
        .await;
    let disc = app
        .create_media_item("CD2", "Folder", music, Some(album))
        .await;
    let one = app
        .create_media_item("One", "Audio", music, Some(album))
        .await;
    let two = app
        .create_media_item("Two", "Audio", music, Some(disc))
        .await;
    for (id, genres) in [(one, vec!["Rock", "Pop"]), (two, vec!["Rock", "Jazz"])] {
        sqlx::query("UPDATE media_items SET genres = $2 WHERE id = $1")
            .bind(id)
            .bind(genres)
            .execute(&app.pool)
            .await
            .unwrap();
    }
    let changed = app
        .data
        .media_items()
        .album_genres_from_tracks(music)
        .await
        .unwrap();
    assert_eq!(changed, 1);
    let (_, body) = app.get(&format!("/Items/{album}"), &token).await;
    assert_eq!(
        body["Genres"],
        serde_json::json!(["Jazz", "Pop", "Rock"]),
        "{body}"
    );
    assert_eq!(body["GenreItems"][0]["Name"], "Jazz", "{body}");
    // Again: nothing to change.
    let again = app
        .data
        .media_items()
        .album_genres_from_tracks(music)
        .await
        .unwrap();
    assert_eq!(again, 0);
    app.cleanup().await;
}
