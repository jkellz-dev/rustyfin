//! Show and episode NFOs as Sonarr's Kodi metadata writes them (real ones
//! from the owner's library, cast trimmed): `<uniqueid>` ids (including
//! Sonarr's own and TVmaze, which are ignored), TheTVDB photo addresses for
//! the cast with no `<order>`, an `<episodeguide>` of JSON, and the TheTVDB
//! page as a plain line after `</tvshow>`.

use rustyfin_common::nfo;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn a_sonarr_show_parses_with_its_ids_and_cast() {
    let meta =
        nfo::parse_series_nfo(&fixture("sonarr_tvshow.nfo")).expect("a Sonarr tvshow.nfo parses");
    assert_eq!(meta.title.as_deref(), Some("Andor"));
    assert_eq!(meta.official_rating.as_deref(), Some("TV-14"));
    assert_eq!(meta.tvdb_id.as_deref(), Some("393189"));
    assert_eq!(meta.tmdb_id.as_deref(), Some("83867"));
    assert_eq!(meta.imdb_id.as_deref(), Some("tt9253284"));
    assert_eq!(meta.genres.len(), 4);
    assert_eq!(meta.studios, ["Disney+"]);
    let cast: Vec<_> = meta
        .people
        .iter()
        .map(|p| (p.sort_order, p.name.as_str(), p.image_url.is_some()))
        .collect();
    // No <order>: the cast keeps the file's order.
    assert_eq!(
        cast,
        [
            (Some(0), "Diego Luna", true),
            (Some(1), "Stellan Skarsgård", true),
            (Some(2), "Alastair Mackenzie", false),
        ]
    );
}

#[test]
fn a_sonarr_episode_parses_with_its_tvdb_id() {
    let meta = nfo::parse_episode_nfo(&fixture("sonarr_episode.nfo"))
        .expect("a Sonarr episode NFO parses");
    assert_eq!(meta.title.as_deref(), Some("Ever Been to Ghorman?"));
    assert_eq!(meta.tvdb_id.as_deref(), Some("10983008"));
    assert!(
        meta.overview
            .as_deref()
            .unwrap()
            .starts_with("Cassian and Bix")
    );
}
