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
        Some("boxsets") => Some("BoxSet"),
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

/// Whether clicking a card opens the item's details page (anything that is
/// not a folder, and albums and collections, whose page lists what they hold)
/// rather than browsing into it.
pub fn opens_detail(item_type: &str, is_folder: bool) -> bool {
    matches!(item_type, "MusicAlbum" | "BoxSet") || !is_folder
}

/// How a details page lists what the item holds: an album its tracks by
/// number, a collection its films by release. None: nothing to list.
pub fn detail_children_sort(item_type: &str) -> Option<&'static str> {
    match item_type {
        "MusicAlbum" => Some("IndexNumber"),
        "BoxSet" => Some("ProductionYear,SortName"),
        _ => None,
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

/// The CSS shape class of an item's image: square for music, landscape for
/// episodes (a frame of the video), else a portrait poster.
pub fn card_shape(item_type: &str) -> &'static str {
    match item_type {
        "MusicArtist" | "MusicAlbum" | "Audio" => "square",
        "Episode" => "landscape",
        _ => "poster",
    }
}

/// The grid card's image and placeholder classes.
pub fn grid_classes(item_type: &str) -> (&'static str, &'static str) {
    match card_shape(item_type) {
        "square" => ("media-poster square", "media-poster-placeholder square"),
        "landscape" => (
            "media-poster landscape",
            "media-poster-placeholder landscape",
        ),
        _ => ("media-poster", "media-poster-placeholder"),
    }
}

/// The library a search is scoped to: the one the breadcrumbs start in
/// (none from the Libraries page, which searches everything).
pub fn search_scope(first_crumb_parent: Option<&str>) -> Option<String> {
    first_crumb_parent
        .filter(|p| !p.is_empty())
        .map(str::to_string)
}

/// The breadcrumb naming a search.
pub fn search_crumb_label(term: &str) -> String {
    format!("Search: {}", term.trim())
}

/// A search result thumbnail's class: episodes landscape, music square, the
/// rest posters.
pub fn search_thumb_class(item_type: &str) -> &'static str {
    match card_shape(item_type) {
        "landscape" => "search-result-poster landscape",
        "square" => "search-result-poster square",
        _ => "search-result-poster",
    }
}

/// The detail view's image and placeholder classes.
pub fn detail_classes(item_type: &str) -> (&'static str, &'static str) {
    match card_shape(item_type) {
        "square" => ("detail-poster square", "detail-poster-placeholder square"),
        "landscape" => (
            "detail-poster landscape",
            "detail-poster-placeholder landscape",
        ),
        _ => ("detail-poster", "detail-poster-placeholder"),
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
    let mut url = format!("{base}/Items/{item_id}/Images/Primary?maxWidth={max_width}&quality=90");
    if let Some(tag) = tag {
        url.push_str("&tag=");
        url.push_str(&encode(tag));
    }
    if !token.is_empty() {
        url.push_str("&api_key=");
        url.push_str(&encode(token));
    }
    url
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Breadcrumb entries for a keyed list: (index, crumb, is it the last one).
/// The flag is part of the entry so that when a crumb stops being the last
/// (going deeper), the entry changes and the list redraws it as a link.
pub fn crumb_trail<T: Clone>(crumbs: &[T]) -> Vec<(usize, T, bool)> {
    let last = crumbs.len().saturating_sub(1);
    crumbs
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, c)| (i, c, i == last))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_library_can_be_sorted_by_name_date_added_year_or_rating() {
        let lib = library_query(Some("movies"));
        assert_eq!(lib.sort_order, "Ascending");
        let labels: Vec<&str> = SORT_OPTIONS.iter().map(|o| o.label).collect();
        assert_eq!(labels, ["Name", "Date added", "Year", "Rating"]);
        let by_year = with_sort(lib, "Year");
        assert_eq!(by_year.sort_by, "ProductionYear,SortName");
        assert_eq!(by_year.sort_order, "Descending,Ascending");
        assert_eq!(by_year.item_types, lib.item_types, "only the order changes");
        assert_eq!(sort_label(by_year), "Year");
        assert_eq!(sort_label(lib), "Name");
        assert_eq!(with_sort(lib, "nonsense"), lib);
    }

    #[test]
    fn a_person_lists_their_films_shows_and_episodes_newest_first() {
        let q = person_query();
        assert!(q.recursive);
        assert_eq!(q.item_types, Some("Movie,Series,Episode"));
        assert_eq!(q.sort_by, "ProductionYear,SortName");
        assert_eq!(q.sort_order, "Descending,Ascending");
    }

    #[test]
    fn cards_badge_watched_items_and_unwatched_counts() {
        assert_eq!(card_badge(false, true, None), Some(CardBadge::Played));
        assert_eq!(
            card_badge(true, false, Some(3)),
            Some(CardBadge::Unplayed(3))
        );
        assert_eq!(card_badge(true, true, Some(0)), Some(CardBadge::Played));
        assert_eq!(card_badge(true, false, Some(0)), None);
        assert_eq!(card_badge(false, false, None), None);
        assert_eq!(CardBadge::Unplayed(120).text(), "99+");
        assert_eq!(CardBadge::Unplayed(7).text(), "7");
        assert_eq!(CardBadge::Played.text(), "✓");
    }

    #[test]
    fn progress_shows_only_for_items_part_way_through() {
        assert_eq!(progress_percent(Some(42.4), None, None, false), Some(42.4));
        assert_eq!(
            progress_percent(None, Some(25), Some(100), false),
            Some(25.0)
        );
        assert_eq!(progress_percent(None, Some(0), Some(100), false), None);
        assert_eq!(
            progress_percent(Some(50.0), None, None, true),
            None,
            "watched: badge, not bar"
        );
        assert_eq!(progress_percent(None, Some(10), None, false), None);
        assert_eq!(
            progress_percent(Some(140.0), None, None, false),
            Some(100.0)
        );
    }
    #[test]
    fn search_is_scoped_to_the_library_the_crumbs_start_in() {
        assert_eq!(search_scope(Some("lib-1")).as_deref(), Some("lib-1"));
        assert_eq!(search_scope(Some("")), None);
        assert_eq!(search_scope(None), None);
        assert_eq!(search_crumb_label(" buddy "), "Search: buddy");
    }
    #[test]
    fn search_thumbnails_keep_each_item_shape() {
        assert_eq!(
            search_thumb_class("Episode"),
            "search-result-poster landscape"
        );
        assert_eq!(
            search_thumb_class("MusicAlbum"),
            "search-result-poster square"
        );
        assert_eq!(search_thumb_class("Movie"), "search-result-poster");
        assert_eq!(search_thumb_class("Series"), "search-result-poster");
    }

    #[test]
    fn the_collections_library_lists_collections() {
        assert_eq!(
            library_query(Some("boxsets")),
            ChildQuery {
                recursive: true,
                item_types: Some("BoxSet"),
                sort_by: "SortName",
            }
        );
    }

    #[test]
    fn collections_and_albums_open_their_details_with_what_they_hold() {
        assert!(opens_detail("BoxSet", true));
        assert!(opens_detail("MusicAlbum", true));
        assert!(opens_detail("Movie", false));
        assert!(!opens_detail("Series", true));
        assert!(!opens_detail("Season", true));
        // A collection lists its films in release order, an album its tracks.
        assert_eq!(
            detail_children_sort("BoxSet"),
            Some("ProductionYear,SortName")
        );
        assert_eq!(detail_children_sort("MusicAlbum"), Some("IndexNumber"));
        assert_eq!(detail_children_sort("Movie"), None);
    }

    #[test]
    fn every_crumb_but_the_last_is_a_link_and_the_flag_moves_when_one_is_added() {
        let trail = crumb_trail(&["TV Shows"]);
        assert_eq!(trail, [(0, "TV Shows", true)]);
        // Going into a show: "TV Shows" is no longer the last crumb. The flag is
        // part of each entry, so the keyed list re-renders it as a link.
        let trail = crumb_trail(&["TV Shows", "Agatha All Along", "Season 1"]);
        assert_eq!(
            trail,
            [
                (0, "TV Shows", false),
                (1, "Agatha All Along", false),
                (2, "Season 1", true),
            ]
        );
        assert!(crumb_trail::<&str>(&[]).is_empty());
    }

    #[test]
    fn episodes_are_landscape_music_square_the_rest_posters() {
        assert_eq!(card_shape("Episode"), "landscape");
        assert_eq!(card_shape("MusicAlbum"), "square");
        assert_eq!(card_shape("Movie"), "poster");
        assert_eq!(card_shape("Season"), "poster");
    }

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
