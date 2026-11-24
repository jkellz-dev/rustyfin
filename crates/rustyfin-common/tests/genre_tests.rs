//! Owner, 2026-11-23: "there are a few genres that make no sense.
//! Specifically all the ones that are just numbers." ID3 tags may give a genre
//! as its ID3v1 code ("17", "(17)", "(17)Rock"); those are named, and a bare
//! number that names no genre is dropped.
use rustyfin_common::music_tags::split_genres;

#[test]
fn id3_genre_codes_are_named() {
    assert_eq!(split_genres("17"), ["Rock"]);
    assert_eq!(split_genres("(20)"), ["Alternative"]);
    assert_eq!(split_genres("(9)Metal"), ["Metal"]);
    assert_eq!(
        split_genres("(17)(20)"),
        ["Rock"],
        "the first code; the rest is not a name"
    );
    assert_eq!(split_genres("(RX)"), ["Remix"]);
    assert_eq!(split_genres("(CR)"), ["Cover"]);
}

#[test]
fn numbers_that_name_no_genre_are_dropped() {
    assert_eq!(split_genres("4000"), Vec::<String>::new());
    assert_eq!(split_genres("Rock; 2019; 17"), ["Rock"], "named once");
}

#[test]
fn names_are_kept_as_written() {
    assert_eq!(
        split_genres("Electronic; Indie Pop"),
        ["Electronic", "Indie Pop"]
    );
    assert_eq!(split_genres("80s"), ["80s"]);
}
