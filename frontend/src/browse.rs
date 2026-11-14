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

/// Width of a grid card's poster request (cards are ~150 px; 2x for sharp screens).
pub const CARD_IMAGE_WIDTH: u32 = 300;
/// Width of the detail view's poster request.
pub const DETAIL_IMAGE_WIDTH: u32 = 600;
/// Width of a search result's thumbnail request.
pub const THUMB_IMAGE_WIDTH: u32 = 120;

/// A primary-image URL at a display size, carrying the image's tag so the
/// browser can keep it for a year (a new image has a new tag).
pub fn image_url(
    base: &str,
    item_id: &str,
    tag: Option<&str>,
    max_width: u32,
    token: &str,
) -> String {
    todo!("{base}{item_id}{tag:?}{max_width}{token}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_urls_ask_for_a_display_size_and_carry_the_tag() {
        assert_eq!(
            image_url("", "abc", Some("t1"), CARD_IMAGE_WIDTH, "tok"),
            "/Items/abc/Images/Primary?maxWidth=300&quality=90&tag=t1&api_key=tok"
        );
        assert_eq!(
            image_url("http://h", "abc", None, 600, ""),
            "http://h/Items/abc/Images/Primary?maxWidth=600&quality=90"
        );
        assert_eq!(
            image_url("", "abc", Some("a b&c"), 120, ""),
            "/Items/abc/Images/Primary?maxWidth=120&quality=90&tag=a%20b%26c"
        );
    }

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
