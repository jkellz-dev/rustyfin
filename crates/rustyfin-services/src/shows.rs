//! TV shows: seasons and episodes of a show, what to watch next, and what
//! airs soon, all within the libraries the user may see.

use chrono::{DateTime, Utc};
use rustyfin_models::data::{Data, MediaItem, MediaItemStore};
use uuid::Uuid;

use crate::access::Access;
use crate::error::ServiceResult;

/// A page of items and how many there are in all.
#[derive(Debug, Default)]
pub struct Page {
    pub items: Vec<MediaItem>,
    pub total: i64,
}

impl Page {
    /// `start..start + limit` of `all`, with the full count.
    fn of(all: Vec<MediaItem>, start: i64, limit: i64) -> Self {
        let (items, total) = slice(all, start, limit);
        Self { items, total }
    }
}

/// `start..start + limit` of `all`, and how many `all` held.
fn slice<T>(all: Vec<T>, start: i64, limit: i64) -> (Vec<T>, i64) {
    let total = all.len() as i64;
    let page = all
        .into_iter()
        .skip(start.max(0) as usize)
        .take(limit.max(0) as usize)
        .collect();
    (page, total)
}

/// What to put in next up.
#[derive(Debug, Default, Clone)]
pub struct NextUp {
    /// Only this show's next episode.
    pub series: Option<Uuid>,
    /// Only shows in this library.
    pub library: Option<Uuid>,
    pub start: i64,
    pub limit: i64,
    /// Only shows watched since then (unstarted shows are kept, as in
    /// Jellyfin 10.10).
    pub watched_since: Option<DateTime<Utc>>,
    /// Offer the first episode of shows the user never started, after the
    /// ones in progress, newest first.
    pub unstarted: bool,
}

/// Which episodes of a show.
#[derive(Debug, Default, Clone, Copy)]
pub struct Episodes {
    /// This season (by id) ...
    pub season: Option<Uuid>,
    /// ... or this season (by number); neither: every season's episodes.
    pub season_number: Option<i32>,
    pub start: i64,
    pub limit: i64,
}

/// Most seasons a show lists, and most episodes when all seasons are asked.
const MAX_SEASONS: i64 = 100;
const MAX_ALL_EPISODES: usize = 500;
/// Upcoming episodes looked at for a user limited to some libraries.
const MAX_UPCOMING: i64 = 1000;

/// Show rules over a data layer.
pub struct Shows<'a, D> {
    pub(crate) data: &'a D,
}

impl<D: Data> Shows<'_, D> {
    fn access(&self) -> Access<'_, D> {
        Access { data: self.data }
    }

    /// The next episode to watch of each show `user` is in, then (when
    /// asked) the first of shows they never started.
    pub async fn next_up(&self, user: Uuid, want: &NextUp) -> ServiceResult<Page> {
        let items = self.data.media_items();
        if let Some(series) = want.series {
            if self.access().item(user, series).await.is_err() {
                return Ok(Page::default());
            }
            let last = items.get_last_watched_episode(user, series).await?;
            let (season, episode) = last
                .map(|e| (e.parent_index_number, e.index_number))
                .unwrap_or_default();
            let next = items
                .get_next_episode(user, series, season, episode)
                .await?;
            return Ok(Page::of(next.into_iter().collect(), 0, 1));
        }

        let fetch = want.start + want.limit + 10;
        let active: Vec<Uuid> = items
            .get_active_series(user, want.library, fetch)
            .await?
            .into_iter()
            .filter(|(_, last)| want.watched_since.is_none_or(|since| *last >= since))
            .map(|(id, _)| id)
            .collect();
        let mut next = items.get_next_episodes(user, &active).await?;
        if want.unstarted {
            let fresh = items
                .series_never_started(user, want.library, fetch)
                .await?;
            next.extend(items.get_next_episodes(user, &fresh).await?);
        }
        let next = self.access().retain(user, next).await?;
        Ok(Page::of(next, want.start, want.limit))
    }

    /// A show's seasons, with the show; `NotFound` when `user` may not see it.
    pub async fn seasons(
        &self,
        user: Uuid,
        series: Uuid,
    ) -> ServiceResult<(MediaItem, Vec<MediaItem>)> {
        let show = self.access().item(user, series).await?;
        let seasons = self
            .data
            .media_items()
            .list_by_parent(Some(series), &["Season".to_string()], 0, MAX_SEASONS)
            .await?;
        Ok((show, seasons))
    }

    /// A show's episodes; `NotFound` when `user` may not see the show or the
    /// season named. A season number the show lacks gives an empty page.
    pub async fn episodes(&self, user: Uuid, series: Uuid, want: Episodes) -> ServiceResult<Page> {
        self.access().item(user, series).await?;
        let items = self.data.media_items();
        let episode = ["Episode".to_string()];
        let season = match (want.season, want.season_number) {
            (Some(season), _) => {
                self.access().item(user, season).await?;
                season
            }
            (None, number) => {
                let (_, seasons) = self.seasons(user, series).await?;
                let Some(number) = number else {
                    // Every season's episodes in one query, season by season.
                    let ids: Vec<Uuid> = seasons.iter().map(|s| s.id).collect();
                    let all = items
                        .list_by_parents(&ids, &episode, MAX_ALL_EPISODES)
                        .await?;
                    let total = all.len() as i64;
                    return Ok(Page { items: all, total });
                };
                match seasons.iter().find(|s| s.index_number == Some(number)) {
                    Some(s) => s.id,
                    None => return Ok(Page::default()),
                }
            }
        };
        let page = items
            .list_by_parent(Some(season), &episode, want.start, want.limit)
            .await?;
        let total = items.count_by_parent(Some(season), &episode).await?;
        Ok(Page { items: page, total })
    }

    /// Episodes airing from today, soonest first, in libraries `user` sees.
    pub async fn upcoming(
        &self,
        user: Uuid,
        library: Option<Uuid>,
        start: i64,
        limit: i64,
    ) -> ServiceResult<Page> {
        let items = self.data.media_items();
        if self.access().limited(user).await? {
            let all = items.upcoming_episodes(library, MAX_UPCOMING, 0).await?;
            let all = self.access().retain(user, all).await?;
            return Ok(Page::of(all, start, limit));
        }
        let page = items.upcoming_episodes(library, limit, start).await?;
        let total = items.count_upcoming_episodes(library).await?;
        Ok(Page { items: page, total })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_counts_everything_and_keeps_its_slice() {
        assert_eq!(slice(vec![1, 2, 3, 4, 5], 1, 2), (vec![2, 3], 5));
        assert_eq!(slice(vec![1, 2], 5, 2), (vec![], 2));
    }
}
