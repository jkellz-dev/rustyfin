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
    /// One direction per `sort_by` key (the server reuses the first for any
    /// key without one).
    pub sort_order: &'static str,
}

/// An order the library grid offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortOption {
    pub label: &'static str,
    pub sort_by: &'static str,
    pub sort_order: &'static str,
}

/// The orders a library can be listed in; newest, highest first except name.
pub const SORT_OPTIONS: &[SortOption] = &[
    SortOption {
        label: "Name",
        sort_by: "SortName",
        sort_order: "Ascending",
    },
    SortOption {
        label: "Date added",
        sort_by: "DateCreated,SortName",
        sort_order: "Descending,Ascending",
    },
    SortOption {
        label: "Year",
        sort_by: "ProductionYear,SortName",
        sort_order: "Descending,Ascending",
    },
    SortOption {
        label: "Rating",
        sort_by: "CommunityRating,SortName",
        sort_order: "Descending,Ascending",
    },
];

/// `query` listed in the order labelled `label` (unchanged for an unknown one).
pub fn with_sort(query: ChildQuery, label: &str) -> ChildQuery {
    SORT_OPTIONS
        .iter()
        .find(|o| o.label == label)
        .map_or(query, |o| ChildQuery {
            sort_by: o.sort_by,
            sort_order: o.sort_order,
            ..query
        })
}

/// The label of the order `query` uses ("Name" when it is none of them).
pub fn sort_label(query: ChildQuery) -> &'static str {
    SORT_OPTIONS
        .iter()
        .find(|o| o.sort_by == query.sort_by)
        .map_or("Name", |o| o.label)
}

/// What a person's page lists: the films, shows and episodes they are in,
/// newest first.
pub fn person_query() -> ChildQuery {
    ChildQuery {
        recursive: true,
        item_types: Some("Movie,Series,Episode"),
        sort_by: "ProductionYear,SortName",
        sort_order: "Descending,Ascending",
    }
}

/// The corner badge of a grid card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardBadge {
    /// Watched (a folder: everything in it watched).
    Played,
    /// A folder's unwatched episodes or films.
    Unplayed(i32),
}

impl CardBadge {
    pub fn text(self) -> String {
        match self {
            CardBadge::Played => "✓".to_string(),
            CardBadge::Unplayed(n) if n > 99 => "99+".to_string(),
            CardBadge::Unplayed(n) => n.to_string(),
        }
    }

    pub fn class(self) -> &'static str {
        match self {
            CardBadge::Played => "card-badge card-badge-played",
            CardBadge::Unplayed(_) => "card-badge",
        }
    }
}

/// A card's badge from its user data: a tick when watched, else a folder's
/// unwatched count; nothing otherwise.
pub fn card_badge(is_folder: bool, played: bool, unplayed: Option<i32>) -> Option<CardBadge> {
    if played {
        return Some(CardBadge::Played);
    }
    match unplayed {
        Some(n) if is_folder && n > 0 => Some(CardBadge::Unplayed(n)),
        _ => None,
    }
}

/// How far through an item the user is, in percent, when part way: from the
/// server's percentage, else position over runtime. None when not started or
/// watched (the badge says so).
pub fn progress_percent(
    played_percentage: Option<f64>,
    position_ticks: Option<i64>,
    runtime_ticks: Option<i64>,
    played: bool,
) -> Option<f64> {
    if played {
        return None;
    }
    let pct = played_percentage.or_else(|| match (position_ticks, runtime_ticks) {
        (Some(pos), Some(run)) if run > 0 => Some(pos as f64 * 100.0 / run as f64),
        _ => None,
    })?;
    (pct > 0.0).then_some(pct.min(100.0))
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
        sort_order: "Ascending",
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
        sort_order: "Ascending",
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

/// What an extra is, in words: "Trailer", "Behind the scenes", ...
pub fn extra_label(extra_type: Option<&str>) -> &'static str {
    match extra_type.unwrap_or("Unknown") {
        "Trailer" => "Trailer",
        "BehindTheScenes" => "Behind the scenes",
        "DeletedScene" => "Deleted scene",
        "Interview" => "Interview",
        "Scene" => "Scene",
        "Sample" => "Sample",
        "Short" => "Short",
        "Featurette" => "Featurette",
        "Clip" => "Clip",
        "ThemeVideo" => "Theme video",
        _ => "Extra",
    }
}

/// How a library's titles are laid out (roadmap 12d); remembered in the
/// browser under `VIEW_MODE_KEY`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Grid,
    List,
}

pub const VIEW_MODE_KEY: &str = "rustyfin_view_mode";

impl ViewMode {
    /// From the stored value; anything else is the grid.
    pub fn from_stored(value: Option<&str>) -> ViewMode {
        match value {
            Some("list") => ViewMode::List,
            _ => ViewMode::Grid,
        }
    }

    pub fn stored(self) -> &'static str {
        match self {
            ViewMode::Grid => "grid",
            ViewMode::List => "list",
        }
    }
}

/// Add `id` to the selection, or take it out when it is there.
pub fn toggle_selected(selected: &mut Vec<String>, id: &str) {
    if let Some(i) = selected.iter().position(|s| s == id) {
        selected.remove(i);
    } else {
        selected.push(id.to_string());
    }
}

/// The kinds a search can be narrowed to: (IncludeItemTypes value, label).
pub const SEARCH_KINDS: [(&str, &str); 8] = [
    ("", "Everything"),
    ("Movie", "Films"),
    ("BoxSet", "Collections"),
    ("Series", "Shows"),
    ("Episode", "Episodes"),
    ("MusicAlbum", "Albums"),
    ("MusicArtist", "Artists"),
    ("Audio", "Tracks"),
];

#[cfg(test)]
mod tests {

    #[test]
    fn a_search_can_be_narrowed_to_collections() {
        assert!(SEARCH_KINDS.contains(&("BoxSet", "Collections")));
    }
    use super::*;

    #[test]
    fn extras_are_named_in_words() {
        assert_eq!(extra_label(Some("Trailer")), "Trailer");
        assert_eq!(extra_label(Some("BehindTheScenes")), "Behind the scenes");
        assert_eq!(extra_label(Some("DeletedScene")), "Deleted scene");
        assert_eq!(extra_label(Some("Unknown")), "Extra");
        assert_eq!(extra_label(None), "Extra");
    }

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
    fn a_search_is_named_by_its_words() {
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
                sort_order: "Ascending",
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

    #[test]
    fn the_view_mode_is_remembered() {
        assert_eq!(ViewMode::from_stored(Some("list")), ViewMode::List);
        assert_eq!(ViewMode::from_stored(Some("grid")), ViewMode::Grid);
        assert_eq!(ViewMode::from_stored(Some("??")), ViewMode::Grid);
        assert_eq!(ViewMode::from_stored(None), ViewMode::Grid);
        assert_eq!(
            ViewMode::from_stored(Some(ViewMode::List.stored())),
            ViewMode::List
        );
    }

    #[test]
    fn selecting_twice_unselects() {
        let mut s = Vec::new();
        toggle_selected(&mut s, "a");
        toggle_selected(&mut s, "b");
        toggle_selected(&mut s, "a");
        assert_eq!(s, vec!["b".to_string()]);
    }

    #[test]
    fn a_track_shows_minutes_and_seconds() {
        // "Your Worst Nightmare" runs 0:51; it showed as "0m".
        assert_eq!(runtime_label(510_000_000, "Audio").as_deref(), Some("0:51"));
        assert_eq!(
            runtime_label(1_850_000_000, "Audio").as_deref(),
            Some("3:05")
        );
        assert_eq!(
            runtime_label(37_230_000_000, "Audio").as_deref(),
            Some("1:02:03")
        );
    }

    #[test]
    fn a_video_under_a_minute_shows_seconds() {
        assert_eq!(runtime_label(450_000_000, "Video").as_deref(), Some("45s"));
        assert_eq!(
            runtime_label(25_000_000_000, "Episode").as_deref(),
            Some("41m")
        );
        assert_eq!(
            runtime_label(55_980_000_000, "Movie").as_deref(),
            Some("1h 33m")
        );
        assert_eq!(
            runtime_label(72_000_000_000, "Movie").as_deref(),
            Some("2h")
        );
    }

    #[test]
    fn an_unknown_runtime_shows_nothing() {
        assert_eq!(runtime_label(0, "Audio"), None);
        assert_eq!(runtime_label(-5, "Movie"), None);
        assert_eq!(runtime_label(9_000_000, "Movie"), None);
    }
}
