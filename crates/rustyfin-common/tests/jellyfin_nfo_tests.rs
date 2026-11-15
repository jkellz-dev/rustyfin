//! A movie NFO as Jellyfin's NFO saver writes it (a real one from the owner's
//! library, cast trimmed): a UTF-8 byte-order mark, ids as top-level
//! `<tmdbid>`/`<imdbid>`/`<id>` instead of Kodi's `<uniqueid>`, cast ordered by
//! `<sortorder>`, and actor thumbs that are paths inside Jellyfin's own config
//! folder, which no other program can read.

use rustyfin_common::nfo;
use std::path::Path;

fn fixture() -> nfo::NfoMetadata {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jellyfin_movie.nfo");
    nfo::parse_movie_nfo(&path).expect("a Jellyfin NFO parses, byte-order mark and all")
}

#[test]
fn reads_the_film_and_skips_empty_elements() {
    let meta = fixture();
    assert_eq!(meta.title.as_deref(), Some("8-Bit Christmas"));
    assert_eq!(meta.production_year, Some(2021));
    assert_eq!(meta.official_rating.as_deref(), Some("PG"));
    assert_eq!(meta.genres, ["Comedy", "Family"]);
    assert_eq!(
        meta.studios,
        ["New Line Cinema", "Star Thrower Entertainment"]
    );
}

#[test]
fn reads_top_level_ids() {
    let meta = fixture();
    assert_eq!(meta.tmdb_id.as_deref(), Some("802217"));
    assert_eq!(meta.imdb_id.as_deref(), Some("tt11540284"));
}

#[test]
fn orders_cast_by_sortorder_and_ignores_photos_only_jellyfin_can_read() {
    let meta = fixture();
    let cast: Vec<_> = meta
        .people
        .iter()
        .filter(|p| p.person_type == "Actor")
        .map(|p| (p.name.as_str(), p.sort_order, p.image_url.as_deref()))
        .collect();
    // /config/metadata/People/... is inside Jellyfin's config volume: not a
    // photo anyone else can fetch, so it must not stop a TMDB lookup.
    assert_eq!(
        cast,
        [
            ("Neil Patrick Harris", Some(0), None),
            ("Winslow Fegley", Some(1), None),
            ("Tom Rooney", Some(15), None),
        ]
    );
}

#[test]
fn a_kodi_actor_photo_address_is_kept() {
    let dir = std::env::temp_dir().join(format!("rf-nfo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("film.nfo");
    std::fs::write(
        &path,
        "<movie><title>T</title><actor><name>A</name><order>2</order>\
         <thumb>https://image.tmdb.org/t/p/original/a.jpg</thumb></actor></movie>",
    )
    .unwrap();
    let meta = nfo::parse_movie_nfo(&path).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(meta.people[0].sort_order, Some(2));
    assert_eq!(
        meta.people[0].image_url.as_deref(),
        Some("https://image.tmdb.org/t/p/original/a.jpg")
    );
}
