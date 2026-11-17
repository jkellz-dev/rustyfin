//! Where the media browser is, as a URL (owner, 2026-11-17: the browser's
//! back button did nothing, so going from a film back to its list was hard).
//! Every place in the browser has its own address under `/web/media`, so the
//! back and forward buttons, reloads and bookmarks all work:
//!
//! - `/web/media`: the libraries
//! - `/web/media/library/<id>[/sort/<order>][/start/<n>]`: a library's titles
//! - `/web/media/folder/<id>[/start/<n>]`: a show's seasons, a season's episodes
//! - `/web/media/item/<id>`: a title's page
//! - `/web/media/person/<id>/<name>[/start/<n>]`: a person's titles
//! - `/web/media/search/<words>[/in/<library id>]`: search results

/// One place in the media browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaRoute {
    Libraries,
    Library {
        id: String,
        sort: Option<String>,
        start: i32,
    },
    Folder {
        id: String,
        start: i32,
    },
    Item {
        id: String,
    },
    Person {
        id: String,
        name: String,
        start: i32,
    },
    Search {
        words: String,
        library: Option<String>,
    },
}

const BASE: &str = "/web/media";

impl MediaRoute {
    /// The route of a path such as `/web/media/item/abc` (as in the address
    /// bar: segments may be percent-encoded). Anything unknown is the
    /// libraries.
    pub fn parse(path: &str) -> MediaRoute {
        let _ = ();
        let _ = path;
        MediaRoute::Libraries
    }

    /// The path of this place.
    pub fn path(&self) -> String {
        let _ = ();
        String::new()
    }

    /// The same place, another page of it.
    pub fn with_start(&self, n: i32) -> MediaRoute {
        let _ = ();
        let _ = n;
        self.clone()
    }
}

/// One step of the breadcrumb trail: its label, and where it goes (`None`
/// for the page itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crumb {
    pub label: String,
    pub path: Option<String>,
}

/// The trail to an item from its ancestors (`/Items/{id}/Ancestors`, nearest
/// first, as Jellyfin returns them): the library, then e.g. the show and the
/// season, then the item itself. The root folder is left out.
pub fn trail(ancestors: &[(String, String, String)], current: &str) -> Vec<Crumb> {
    let _ = ();
    let _ = (ancestors, current);
    Vec::new()
}

/// Percent-encode a path segment (everything but unreserved characters).
pub fn encode(value: &str) -> String {
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

/// Undo `encode` (and the browser's own encoding of the address bar).
pub fn decode(value: &str) -> String {
    let _ = ();
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_place_round_trips_through_its_address() {
        let places = [
            MediaRoute::Libraries,
            MediaRoute::Library {
                id: "lib1".into(),
                sort: None,
                start: 0,
            },
            MediaRoute::Library {
                id: "lib1".into(),
                sort: Some("Date added".into()),
                start: 100,
            },
            MediaRoute::Folder {
                id: "show".into(),
                start: 0,
            },
            MediaRoute::Item { id: "film".into() },
            MediaRoute::Person {
                id: "p1".into(),
                name: "Jean-Luc Godard / Dir.".into(),
                start: 50,
            },
            MediaRoute::Search {
                words: "28 days".into(),
                library: Some("lib1".into()),
            },
            MediaRoute::Search {
                words: "100% fun?".into(),
                library: None,
            },
        ];
        for place in places {
            let path = place.path();
            assert!(path.starts_with("/web/media"), "{path}");
            assert_eq!(MediaRoute::parse(&path), place, "{path}");
        }
    }

    #[test]
    fn addresses_read_as_expected() {
        assert_eq!(
            MediaRoute::Item { id: "abc".into() }.path(),
            "/web/media/item/abc"
        );
        assert_eq!(
            MediaRoute::Library {
                id: "l".into(),
                sort: Some("Name".into()),
                start: 50
            }
            .path(),
            "/web/media/library/l/sort/Name/start/50"
        );
        assert_eq!(MediaRoute::parse("/web/media"), MediaRoute::Libraries);
        assert_eq!(MediaRoute::parse("/web/media/"), MediaRoute::Libraries);
        assert_eq!(
            MediaRoute::parse("/web/media/nonsense/x"),
            MediaRoute::Libraries
        );
        assert_eq!(MediaRoute::parse("/web/media/item"), MediaRoute::Libraries);
        // A bad page number is the first page.
        assert_eq!(
            MediaRoute::parse("/web/media/folder/f/start/-5"),
            MediaRoute::Folder {
                id: "f".into(),
                start: 0
            }
        );
        assert_eq!(
            MediaRoute::parse("/web/media/folder/f/start/x"),
            MediaRoute::Folder {
                id: "f".into(),
                start: 0
            }
        );
    }

    #[test]
    fn paging_keeps_the_place() {
        let lib = MediaRoute::Library {
            id: "l".into(),
            sort: Some("Year".into()),
            start: 0,
        };
        assert_eq!(
            lib.with_start(100),
            MediaRoute::Library {
                id: "l".into(),
                sort: Some("Year".into()),
                start: 100
            }
        );
        assert_eq!(
            MediaRoute::Item { id: "i".into() }.with_start(5),
            MediaRoute::Item { id: "i".into() }
        );
    }

    #[test]
    fn an_episodes_trail_runs_library_show_season_episode() {
        // Nearest first, as /Ancestors returns them.
        let ancestors = vec![
            ("season".into(), "Season".into(), "Season 1".into()),
            ("show".into(), "Series".into(), "Severance".into()),
            ("tv".into(), "CollectionFolder".into(), "TV Shows".into()),
            (
                "root".into(),
                "UserRootFolder".into(),
                "Media Folders".into(),
            ),
        ];
        assert_eq!(
            trail(&ancestors, "Good News About Hell"),
            vec![
                Crumb {
                    label: "TV Shows".into(),
                    path: Some("/web/media/library/tv".into())
                },
                Crumb {
                    label: "Severance".into(),
                    path: Some("/web/media/folder/show".into())
                },
                Crumb {
                    label: "Season 1".into(),
                    path: Some("/web/media/folder/season".into())
                },
                Crumb {
                    label: "Good News About Hell".into(),
                    path: None
                },
            ]
        );
        assert_eq!(
            trail(&[], "Alone"),
            vec![Crumb {
                label: "Alone".into(),
                path: None
            }]
        );
    }

    #[test]
    fn decoding_tolerates_stray_percent_signs() {
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("a%2"), "a%2");
        assert_eq!(decode("caf%C3%A9"), "café");
    }
}
