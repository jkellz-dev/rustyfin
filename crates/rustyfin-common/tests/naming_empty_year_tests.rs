//! Radarr names a film with no known year "Title ()"; the empty parentheses
//! are not part of the title.
use rustyfin_common::naming::{clean_title, split_folder_year};

#[test]
fn an_empty_year_is_dropped() {
    assert_eq!(clean_title("Baby Driver 2 ()"), "Baby Driver 2");
    assert_eq!(
        split_folder_year("Baby Driver 2 ()"),
        ("Baby Driver 2".to_string(), None)
    );
    assert_eq!(
        split_folder_year("Hadestown West End Cast 2024 () [tmdbid-1760634]"),
        ("Hadestown West End Cast 2024".to_string(), None)
    );
    assert_eq!(
        split_folder_year("Bluey (2018)"),
        ("Bluey".to_string(), Some(2018))
    );
}
