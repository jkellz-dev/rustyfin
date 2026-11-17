//! A movie NFO as Radarr's Kodi metadata writes it (a real one, cast
//! trimmed): `<uniqueid>` ids, `<ratings>`, TMDB web
//! addresses for art and cast photos, cast out of order, empty elements, and
//! the film's TMDB and IMDb pages as plain lines after `</movie>`.

use rustyfin_common::nfo;
use std::path::Path;

fn path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/radarr_movie.nfo")
}

#[test]
fn parses_despite_the_links_after_the_document() {
    let meta = nfo::parse_movie_nfo(&path()).expect("a Radarr NFO parses");
    assert_eq!(meta.title.as_deref(), Some("8-Bit Christmas"));
    assert_eq!(meta.production_year, Some(2021));
    assert_eq!(meta.tmdb_id.as_deref(), Some("802217"));
    assert_eq!(meta.imdb_id.as_deref(), Some("tt11540284"));
    assert_eq!(meta.genres, ["Family", "Comedy"]);
    assert_eq!(meta.studios, ["Star Thrower Entertainment"]);
}

#[test]
fn keeps_cast_order_and_photo_addresses() {
    let meta = nfo::parse_movie_nfo(&path()).unwrap();
    let mut cast: Vec<_> = meta
        .people
        .iter()
        .filter(|p| p.person_type == "Actor")
        .map(|p| (p.sort_order, p.name.as_str(), p.image_url.is_some()))
        .collect();
    cast.sort();
    assert_eq!(
        cast,
        [
            (Some(0), "Winslow Fegley", true),
            (Some(10), "Braelyn Rankins", true),
            (Some(19), "Erica Levene", false),
        ]
    );
}

#[test]
fn a_film_without_a_series_has_no_collection() {
    assert_eq!(nfo::parse_movie_collection(&path()), None);
}
