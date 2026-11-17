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

/// What a library's titles are narrowed to (roadmap 12d).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryFilter {
    pub genre: Option<String>,
    pub year: Option<i32>,
    /// At least this community rating (out of 10).
    pub min_rating: Option<u8>,
}

impl LibraryFilter {
    pub fn is_empty(&self) -> bool {
        self.genre.is_none() && self.year.is_none() && self.min_rating.is_none()
    }

    /// The `/Items` query parameters for this filter (leading `&`).
    pub fn query(&self) -> String {
        let mut q = String::new();
        if let Some(g) = &self.genre {
            q.push_str(&format!("&Genres={}", encode(g)));
        }
        if let Some(y) = self.year {
            q.push_str(&format!("&Years={y}"));
        }
        if let Some(r) = self.min_rating {
            q.push_str(&format!("&MinCommunityRating={r}"));
        }
        q
    }

    /// The address segments for this filter.
    fn path(&self) -> String {
        let mut p = String::new();
        if let Some(g) = &self.genre {
            p.push_str(&format!("/genre/{}", encode(g)));
        }
        if let Some(y) = self.year {
            p.push_str(&format!("/year/{y}"));
        }
        if let Some(r) = self.min_rating {
            p.push_str(&format!("/rating/{r}"));
        }
        p
    }
}

/// One place in the media browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaRoute {
    Libraries,
    Library {
        id: String,
        sort: Option<String>,
        filter: LibraryFilter,
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
        /// Only this kind: Movie, Series, Episode, MusicAlbum, Audio, ...
        kind: Option<String>,
    },
}

const BASE: &str = "/web/media";

impl MediaRoute {
    /// The route of a path such as `/web/media/item/abc` (as in the address
    /// bar: segments may be percent-encoded). Anything unknown is the
    /// libraries.
    pub fn parse(path: &str) -> MediaRoute {
        let rest = path.strip_prefix(BASE).unwrap_or("").trim_matches('/');
        let segs: Vec<String> = rest
            .split('/')
            .filter(|s| !s.is_empty())
            .map(decode)
            .collect();
        // After the kind and id come key/value pairs.
        let opt = |key: &str| {
            segs.get(2..)
                .unwrap_or_default()
                .chunks(2)
                .find(|w| w.len() == 2 && w[0] == key)
                .map(|w| w[1].clone())
        };
        let start = || {
            opt("start")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
                .max(0)
        };
        match segs.first().map(String::as_str) {
            Some("library") if segs.len() >= 2 => MediaRoute::Library {
                id: segs[1].clone(),
                sort: opt("sort"),
                filter: LibraryFilter {
                    genre: opt("genre").filter(|g| !g.is_empty()),
                    year: opt("year")
                        .and_then(|y| y.parse().ok())
                        .filter(|y| (1800..=2200).contains(y)),
                    min_rating: opt("rating")
                        .and_then(|r| r.parse().ok())
                        .filter(|r| *r <= 10),
                },
                start: start(),
            },
            Some("folder") if segs.len() >= 2 => MediaRoute::Folder {
                id: segs[1].clone(),
                start: start(),
            },
            Some("item") if segs.len() >= 2 => MediaRoute::Item {
                id: segs[1].clone(),
            },
            Some("person") if segs.len() >= 3 => MediaRoute::Person {
                id: segs[1].clone(),
                name: segs[2].clone(),
                start: segs
                    .windows(2)
                    .skip(3)
                    .find(|w| w[0] == "start")
                    .and_then(|w| w[1].parse().ok())
                    .unwrap_or(0)
                    .max(0),
            },
            Some("search") if segs.len() >= 2 => MediaRoute::Search {
                words: segs[1].clone(),
                library: opt("in"),
                kind: opt("type").filter(|k| !k.is_empty()),
            },
            _ => MediaRoute::Libraries,
        }
    }

    /// The path of this place.
    pub fn path(&self) -> String {
        let start = |n: i32| {
            if n > 0 {
                format!("/start/{n}")
            } else {
                String::new()
            }
        };
        match self {
            MediaRoute::Libraries => BASE.to_string(),
            MediaRoute::Library {
                id,
                sort,
                filter,
                start: s,
            } => {
                let sort = sort
                    .as_deref()
                    .map(|o| format!("/sort/{}", encode(o)))
                    .unwrap_or_default();
                format!(
                    "{BASE}/library/{}{sort}{}{}",
                    encode(id),
                    filter.path(),
                    start(*s)
                )
            }
            MediaRoute::Folder { id, start: s } => {
                format!("{BASE}/folder/{}{}", encode(id), start(*s))
            }
            MediaRoute::Item { id } => format!("{BASE}/item/{}", encode(id)),
            MediaRoute::Person { id, name, start: s } => {
                format!("{BASE}/person/{}/{}{}", encode(id), encode(name), start(*s))
            }
            MediaRoute::Search {
                words,
                library,
                kind,
            } => {
                let scope = library
                    .as_deref()
                    .map(|l| format!("/in/{}", encode(l)))
                    .unwrap_or_default();
                let kind = kind
                    .as_deref()
                    .map(|k| format!("/type/{}", encode(k)))
                    .unwrap_or_default();
                format!("{BASE}/search/{}{scope}{kind}", encode(words))
            }
        }
    }

    /// The same place, another page of it.
    pub fn with_start(&self, n: i32) -> MediaRoute {
        let mut r = self.clone();
        match &mut r {
            MediaRoute::Library { start, .. }
            | MediaRoute::Folder { start, .. }
            | MediaRoute::Person { start, .. } => *start = n.max(0),
            _ => {}
        }
        r
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
    let mut crumbs: Vec<Crumb> = ancestors
        .iter()
        .rev()
        .filter(|(_, kind, _)| !matches!(kind.as_str(), "UserRootFolder" | "AggregateFolder"))
        .map(|(id, kind, name)| Crumb {
            label: name.clone(),
            path: Some(
                if kind == "CollectionFolder" {
                    MediaRoute::Library {
                        id: id.clone(),
                        sort: None,
                        filter: LibraryFilter::default(),
                        start: 0,
                    }
                } else {
                    MediaRoute::Folder {
                        id: id.clone(),
                        start: 0,
                    }
                }
                .path(),
            ),
        })
        .collect();
    crumbs.push(Crumb {
        label: current.to_string(),
        path: None,
    });
    crumbs
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
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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
                filter: LibraryFilter::default(),
                start: 0,
            },
            MediaRoute::Library {
                id: "lib1".into(),
                sort: Some("Date added".into()),
                filter: LibraryFilter::default(),
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
                kind: None,
            },
            MediaRoute::Search {
                words: "100% fun?".into(),
                library: None,
                kind: None,
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
                filter: LibraryFilter::default(),
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
            filter: LibraryFilter::default(),
            start: 0,
        };
        assert_eq!(
            lib.with_start(100),
            MediaRoute::Library {
                id: "l".into(),
                sort: Some("Year".into()),
                filter: LibraryFilter::default(),
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

    #[test]
    fn library_filters_and_search_kinds_live_in_the_address() {
        let place = MediaRoute::Library {
            id: "l".into(),
            sort: Some("Year".into()),
            filter: LibraryFilter {
                genre: Some("Science Fiction".into()),
                year: Some(1999),
                min_rating: Some(7),
            },
            start: 50,
        };
        assert_eq!(
            place.path(),
            "/web/media/library/l/sort/Year/genre/Science%20Fiction/year/1999/rating/7/start/50"
        );
        assert_eq!(MediaRoute::parse(&place.path()), place);
        // Paging keeps the filter.
        let MediaRoute::Library { filter, .. } = place.with_start(100) else {
            panic!()
        };
        assert_eq!(filter.year, Some(1999));
        let search = MediaRoute::Search {
            words: "alien".into(),
            library: None,
            kind: Some("Movie".into()),
        };
        assert_eq!(search.path(), "/web/media/search/alien/type/Movie");
        assert_eq!(MediaRoute::parse(&search.path()), search);
        // Nonsense values are dropped.
        assert_eq!(
            MediaRoute::parse("/web/media/library/l/year/abc/rating/99"),
            MediaRoute::Library {
                id: "l".into(),
                sort: None,
                filter: LibraryFilter::default(),
                start: 0
            }
        );
    }

    #[test]
    fn a_filter_becomes_query_parameters() {
        assert_eq!(LibraryFilter::default().query(), "");
        let f = LibraryFilter {
            genre: Some("Sci-Fi & Fantasy".into()),
            year: Some(2004),
            min_rating: Some(8),
        };
        assert_eq!(
            f.query(),
            "&Genres=Sci-Fi%20%26%20Fantasy&Years=2004&MinCommunityRating=8"
        );
    }
}
