//! Browsing: the lists of items a user reads (a library, a folder, a
//! collection, search results), what was added lately and what to carry
//! on with, all within the libraries the user may see.

use std::collections::{HashMap, HashSet};

use rustyfin_models::data::{
    CollectionStore, Data, ItemQuery, LibraryStore, MediaItem, MediaItemStore, SEARCH_CANDIDATES,
    SearchIndex, UserData, UserDataStore,
};
use uuid::Uuid;

use crate::access::Access;
use crate::error::{ServiceError, ServiceResult};
use crate::shows::Page;

/// What a list asks for. `query` carries the filters, sort and page; its
/// scope (library, parent, ids), user and access fields are set here.
#[derive(Debug, Default, Clone)]
pub struct Listing {
    pub query: ItemQuery,
    /// The library, folder or collection listed (`None`: everything).
    pub parent: Option<Uuid>,
    /// Count every match, not only the page.
    pub count: bool,
}

/// What Latest asks for.
#[derive(Debug, Default, Clone)]
pub struct Latest {
    /// One library's (`None`: across them all).
    pub library: Option<Uuid>,
    /// The kinds wanted (`None`/empty: films, episodes, tracks, videos).
    pub types: Vec<String>,
    /// Tracks become their album and a show's several new episodes the show.
    pub group: bool,
    pub limit: i64,
    /// Libraries the user keeps out of Latest across libraries.
    pub left_out: Vec<Uuid>,
    /// Leave out what the user has played.
    pub hide_played: bool,
}

/// What Continue Watching asks for.
#[derive(Debug, Default, Clone)]
pub struct Resume {
    /// Within this library, folder, show or season.
    pub within: Option<Uuid>,
    pub include_types: Vec<String>,
    pub exclude_types: Vec<String>,
    pub limit: i64,
}

/// How many in-progress items Continue Watching reads before filtering.
const RESUME_SCAN: i64 = 500;

/// Browsing rules over a data layer and its search index.
pub struct Browse<'a, D> {
    pub(crate) data: &'a D,
}

impl<D: Data> Browse<'_, D> {
    fn access(&self) -> Access<'_, D> {
        Access { data: self.data }
    }

    /// A page of items. A library the user may not see is not found; a
    /// collection lists its films (by release unless another order is
    /// asked); a search term is ranked by the search index when it is ready.
    pub async fn items(&self, user: Uuid, want: Listing) -> ServiceResult<Page> {
        let Listing {
            mut query,
            parent,
            count,
        } = want;
        let accessible = self.access().limited_to(user).await?;
        let is_library = match parent {
            Some(id) => self.data.libraries().find_by_id(id).await?.is_some(),
            None => false,
        };
        if is_library
            && let Some(allowed) = &accessible
            && !allowed.contains(&parent.unwrap_or_default())
        {
            return Err(ServiceError::NotFound);
        }

        // A collection (BoxSet) lists its films, which are not its children by
        // parent: they live in their library and may be in several collections.
        let mut members: Option<Vec<Uuid>> = None;
        if !is_library
            && let Some(id) = parent
            && let Some(found) = self.data.media_items().find_by_id(id).await?
            && found.item_type == "BoxSet"
        {
            self.access().item(user, id).await?;
            let ids = self.data.collections().list_item_ids(id).await?;
            // An empty id list means "no filter" to the query: match nothing.
            members = Some(if ids.is_empty() {
                vec![Uuid::nil()]
            } else {
                ids
            });
        }
        // A collection's films go by release (its DisplayOrder), as in
        // Jellyfin, when no order or name order is asked.
        if members.is_some()
            && query
                .sort_by
                .iter()
                .all(|s| matches!(s.as_str(), "SortName" | "Name" | "Default"))
        {
            query.sort_by = ["PremiereDate", "ProductionYear", "SortName"]
                .map(str::to_string)
                .to_vec();
        }

        // The search index ranks the matches (owner, 2026-11-19); without
        // it (or while it is not ready) the name is matched.
        if let Some(term) = query.search_term.as_deref().map(str::trim)
            && !term.is_empty()
            && let Some((ids, tiers)) = self
                .data
                .search()
                .ranked_tiers(term, &query.include_item_types, SEARCH_CANDIDATES)
                .await
        {
            query.ranked_ids = Some(ids);
            query.ranked_tiers = tiers;
        }

        query.library_id = if is_library { parent } else { None };
        query.parent_id = if is_library || members.is_some() {
            None
        } else {
            parent
        };
        query.recursive = query.recursive && members.is_none();
        if let Some(ids) = members {
            query.ids = ids;
        }
        query.accessible_libraries = if is_library { None } else { accessible };
        query.user_id = Some(user);

        let items = self.data.media_items().query(&query).await?;
        let total = if count {
            self.data.media_items().count_query(&query).await?
        } else {
            0
        };
        Ok(Page { items, total })
    }

    /// Items of each kind in the libraries the user may see.
    pub async fn counts(&self, user: Uuid) -> ServiceResult<Vec<(String, i64)>> {
        let accessible = self.access().limited_to(user).await?;
        Ok(self
            .data
            .media_items()
            .count_by_type(accessible.as_deref())
            .await?)
    }

    /// The media added lately, newest first, in libraries the user may see;
    /// grouped (tracks into albums, a show's several episodes into the show)
    /// when asked.
    pub async fn latest(&self, user: Uuid, want: &Latest) -> ServiceResult<Vec<MediaItem>> {
        let fetch_types = latest_fetch_types(&want.types);
        if !want.types.is_empty() && fetch_types.is_empty() {
            return Ok(Vec::new());
        }
        let accessible = self.access().limited_to(user).await?;
        let thinned =
            want.group || want.hide_played || !want.left_out.is_empty() || accessible.is_some();
        let fetch = if thinned {
            (want.limit * 20).min(1000)
        } else {
            want.limit
        };
        let mut added = self
            .data
            .media_items()
            .latest_by_library(want.library, &fetch_types, fetch)
            .await?;
        // Before grouping, so a user limited to some libraries still gets a
        // full list.
        added.retain(|i| {
            i.library_id.is_none_or(|l| {
                !want.left_out.contains(&l) && accessible.as_ref().is_none_or(|ok| ok.contains(&l))
            })
        });
        if want.hide_played {
            let ids: Vec<Uuid> = added.iter().map(|i| i.id).collect();
            // Played state only thins the list: without it, nothing is left out.
            let played = self
                .data
                .user_data()
                .get_many(user, &ids)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("user_data::get_many for Latest: {e}");
                    HashMap::new()
                });
            added.retain(|i| !played.get(&i.id).is_some_and(|d| d.played));
        }
        if !want.group {
            added.truncate(want.limit.max(0) as usize);
            return Ok(added);
        }
        group_latest(self.data, added, &want.types, want.limit).await
    }

    /// What the user is part way through, most recent first, in libraries
    /// they may see; `keep` is the caller's own filter (media types).
    pub async fn resume(
        &self,
        user: Uuid,
        want: &Resume,
        keep: impl Fn(&MediaItem) -> bool,
    ) -> ServiceResult<Vec<(UserData, MediaItem)>> {
        let accessible = self.access().limited_to(user).await?;
        let started = self
            .data
            .user_data()
            .get_resume_items(user, RESUME_SCAN)
            .await?;
        let ids: Vec<Uuid> = started.iter().map(|(_, id)| *id).collect();
        let mut items: HashMap<Uuid, MediaItem> = self
            .data
            .media_items()
            .find_by_ids(&ids)
            .await?
            .into_iter()
            .map(|item| (item.id, item))
            .collect();
        let mut out = Vec::new();
        for (state, id) in started {
            let Some(item) = items.remove(&id) else {
                continue;
            };
            let visible = item
                .library_id
                .is_none_or(|l| accessible.as_ref().is_none_or(|ok| ok.contains(&l)));
            // Within one library (or show, or season): films are not in a TV
            // library's list.
            let inside = want.within.is_none_or(|w| {
                [
                    item.library_id,
                    item.parent_id,
                    item.series_id,
                    item.season_id,
                ]
                .contains(&Some(w))
            });
            let kind = (want.include_types.is_empty()
                || want.include_types.contains(&item.item_type))
                && !want.exclude_types.contains(&item.item_type);
            if visible && inside && kind && keep(&item) {
                out.push((state, item));
                if out.len() as i64 >= want.limit {
                    break;
                }
            }
        }
        Ok(out)
    }
}

/// What Latest reads: media, never containers. Asking for shows or albums
/// reads their episodes or tracks, which are then grouped.
fn latest_fetch_types(wanted: &[String]) -> Vec<String> {
    const MEDIA: [&str; 5] = ["Movie", "Episode", "Audio", "Video", "MusicVideo"];
    if wanted.is_empty() {
        return MEDIA.map(str::to_string).to_vec();
    }
    let mut types: Vec<String> = Vec::new();
    for t in wanted {
        let read = match t.as_str() {
            "Series" | "Season" => "Episode",
            "MusicAlbum" | "MusicArtist" => "Audio",
            other => other,
        };
        if MEDIA.contains(&read) && !types.iter().any(|x| x == read) {
            types.push(read.to_string());
        }
    }
    types
}

/// One slot of the grouped Latest list.
enum LatestSlot {
    Item(MediaItem),
    Album(Uuid),
    Series(Uuid, MediaItem, usize),
}

/// Jellyfin's grouping of Latest: tracks become their album (also from disc
/// folders), a show with several new episodes becomes the show, one new
/// episode stays an episode. Newest first, `limit` entries.
async fn group_latest<D: Data>(
    data: &D,
    added: Vec<MediaItem>,
    wanted: &[String],
    limit: i64,
) -> ServiceResult<Vec<MediaItem>> {
    // Each track's album: its parent, or its parent's parent for disc folders.
    let parent_ids: Vec<Uuid> = added
        .iter()
        .filter(|r| r.item_type == "Audio")
        .filter_map(|r| r.parent_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let parents: HashMap<Uuid, MediaItem> = data
        .media_items()
        .find_by_ids(&parent_ids)
        .await?
        .into_iter()
        .map(|r| (r.id, r))
        .collect();
    let album_of = |track: &MediaItem| -> Option<Uuid> {
        let parent = parents.get(&track.parent_id?)?;
        if parent.item_type == "MusicAlbum" {
            Some(parent.id)
        } else {
            parent.parent_id
        }
    };

    let mut slots: Vec<LatestSlot> = Vec::new();
    let mut seen_album: HashSet<Uuid> = HashSet::new();
    let mut series_slot: HashMap<Uuid, usize> = HashMap::new();
    for row in added {
        if row.item_type == "Audio"
            && let Some(album) = album_of(&row)
        {
            if seen_album.insert(album) {
                slots.push(LatestSlot::Album(album));
            }
            continue;
        }
        if row.item_type == "Episode"
            && let Some(series) = row.series_id
        {
            if let Some(&i) = series_slot.get(&series) {
                if let LatestSlot::Series(_, _, n) = &mut slots[i] {
                    *n += 1;
                }
            } else {
                series_slot.insert(series, slots.len());
                slots.push(LatestSlot::Series(series, row, 1));
            }
            continue;
        }
        slots.push(LatestSlot::Item(row));
    }

    let folder_ids: Vec<Uuid> = slots
        .iter()
        .filter_map(|s| match s {
            LatestSlot::Album(id) => Some(*id),
            LatestSlot::Series(id, _, n) if *n > 1 => Some(*id),
            _ => None,
        })
        .collect();
    let mut folders: HashMap<Uuid, MediaItem> = data
        .media_items()
        .find_by_ids(&folder_ids)
        .await?
        .into_iter()
        .map(|r| (r.id, r))
        .collect();
    let wants = |t: &str| {
        wanted.is_empty()
            || wanted.iter().any(|w| {
                w == t
                    || (t == "Series" && w == "Episode")
                    || (t == "Episode" && w == "Series")
                    || (t == "MusicAlbum" && w == "Audio")
            })
    };
    let mut out = Vec::new();
    for slot in slots {
        let row = match slot {
            LatestSlot::Item(row) => Some(row),
            LatestSlot::Album(id) => folders.remove(&id),
            LatestSlot::Series(id, episode, n) => {
                if n > 1 {
                    folders.remove(&id)
                } else {
                    Some(episode)
                }
            }
        };
        if let Some(row) = row
            && wants(&row.item_type)
        {
            out.push(row);
            if out.len() as i64 >= limit {
                break;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_reads_media_for_the_kinds_asked() {
        let types =
            |w: &[&str]| latest_fetch_types(&w.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(types(&[]).len(), 5);
        assert_eq!(types(&["Series", "Episode"]), ["Episode"]);
        assert_eq!(types(&["MusicAlbum"]), ["Audio"]);
        assert!(types(&["BoxSet"]).is_empty());
    }
}
