//! What the media browser asks for at each level of the drill-down
//! (Media > TV Shows > show > season > episode).
//!
//! A library lists its top-level items (series, films, artists), found anywhere
//! below it. Below that, each level lists only its direct children: a show its
//! seasons, a season its episodes, in their numbered order. Asking recursively
//! there would mix a show's seasons with every one of its episodes.

/// The `/Items` query for one level of the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildQuery {
    pub recursive: bool,
    pub item_types: Option<&'static str>,
    pub sort_by: &'static str,
}

/// A library's top level: its kind of top-level item, wherever it sits. A mixed
/// library (no kind) lists its direct children instead of everything below it.
pub fn library_query(collection_type: Option<&str>) -> ChildQuery {
    let item_types = match collection_type {
        Some("movies") => Some("Movie"),
        Some("tvshows") => Some("Series"),
        Some("music") => Some("MusicArtist"),
        _ => None,
    };
    ChildQuery {
        recursive: item_types.is_some(),
        item_types,
        sort_by: "SortName",
    }
}

/// The direct children of a folder item: a show's seasons and a season's
/// episodes (and an album's tracks) by number, anything else by name.
pub fn children_query(parent_type: &str) -> ChildQuery {
    ChildQuery {
        recursive: false,
        item_types: None,
        sort_by: match parent_type {
            "Series" | "Season" | "MusicAlbum" => "IndexNumber",
            _ => "SortName",
        },
    }
}

/// A grid card's label: episodes and tracks lead with their number.
pub fn card_label(item_type: &str, name: &str, index_number: Option<i32>) -> String {
    match (item_type, index_number) {
        ("Episode", Some(n)) => format!("E{n:02} · {name}"),
        ("Audio", Some(n)) => format!("{n}. {name}"),
        _ => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tv_library_lists_its_shows_from_anywhere_below_it() {
        let q = library_query(Some("tvshows"));
        assert!(q.recursive);
        assert_eq!(q.item_types, Some("Series"));
        assert_eq!(q.sort_by, "SortName");
    }

    #[test]
    fn a_mixed_library_lists_only_its_direct_children() {
        let q = library_query(None);
        assert!(
            !q.recursive,
            "recursive with no type filter lists every file"
        );
        assert_eq!(q.item_types, None);
    }

    #[test]
    fn a_show_lists_its_seasons_and_a_season_its_episodes_in_order() {
        for parent in ["Series", "Season"] {
            let q = children_query(parent);
            assert!(!q.recursive, "{parent}: only direct children");
            assert_eq!(q.item_types, None);
            assert_eq!(q.sort_by, "IndexNumber", "{parent}");
        }
    }

    #[test]
    fn other_folders_list_direct_children_by_name() {
        let q = children_query("MusicArtist");
        assert!(!q.recursive);
        assert_eq!(q.sort_by, "SortName");
    }

    #[test]
    fn episodes_are_labelled_with_their_number() {
        assert_eq!(card_label("Episode", "Pilot", Some(1)), "E01 · Pilot");
        assert_eq!(card_label("Episode", "Pilot", None), "Pilot");
        assert_eq!(card_label("Audio", "Song", Some(3)), "3. Song");
        assert_eq!(card_label("Season", "Season 2", Some(2)), "Season 2");
    }
}
