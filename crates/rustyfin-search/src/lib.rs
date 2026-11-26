//! Search through Meilisearch: it ranks, Postgres filters (owner,
//! 2026-11-19).
//!
//! Meilisearch holds every shared item's searchable text (name, original
//! title, show name, genres, overview) in one index and every person's name
//! in another, and answers a search with the best-matching ids,
//! typo-tolerant and ranked. The routes then read those ids from Postgres
//! with all their other filters (access, folder, played, linked servers) and
//! keep Meilisearch's order unless the app sorts.
//!
//! [`SearchService::sync`] keeps both indexes in step from the database's own
//! change stamps (`change_xid`, `media_removals`, `people_removals`), so a
//! rename shows up within one sync instead of at the next full rebuild.
//! Without Meilisearch, or while it is down or not yet built, searches match
//! names in Postgres.

pub mod indexer;
pub mod query;

use rustyfin_models::data::{
    Data, DataError, MediaItem, MediaItemStore, PeopleStore, Person, SearchFeedStore,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use uuid::Uuid;

/// Documents per Meilisearch batch when indexing.
const PAGE: i64 = 5_000;

/// Ids a search asks Meilisearch for, at most (its own default cap).
pub const CANDIDATES: usize = 1_000;

/// How long an index task may take before the sync gives up for this round.
const TASK_TIMEOUT: Duration = Duration::from_secs(600);

/// A search hit with the fields needed to build a `SearchHint`.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub id: Uuid,
    pub name: String,
    pub item_type: String,
    pub production_year: Option<i32>,
    pub series_name: Option<String>,
    pub index_number: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub runtime_ticks: Option<i64>,
    pub library_id: Option<Uuid>,
    /// Meilisearch's ranking score (0 to 1); 0 when Postgres answered.
    pub score: f64,
}

impl From<MediaItem> for SearchHit {
    fn from(item: MediaItem) -> Self {
        Self {
            id: item.id,
            name: item.name,
            item_type: item.item_type,
            production_year: item.production_year,
            series_name: item.series_name,
            index_number: item.index_number,
            parent_index_number: item.parent_index_number,
            runtime_ticks: item.runtime_ticks,
            library_id: item.library_id,
            score: 0.0,
        }
    }
}

/// Whether search works, for the dashboard.
#[derive(Debug, Clone)]
pub struct BackendHealth {
    /// `Meilisearch` or `Postgres` (the fallback, always there).
    pub backend: &'static str,
    pub ok: bool,
    pub detail: Option<String>,
}

/// What a sync did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// The items index was built whole (first sync, or after a reset).
    pub rebuilt: bool,
    pub indexed: usize,
    pub removed: usize,
    pub people_indexed: usize,
    pub people_removed: usize,
    /// The people index was built whole.
    pub people_rebuilt: bool,
}

/// Search error type.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Database error: {0}")]
    Database(#[from] DataError),
    #[error("Search engine error: {0}")]
    Engine(String),
}

struct Meili {
    client: meilisearch_sdk::client::Client,
    /// The items index's uid (`media_items`, or prefixed in tests).
    index: String,
    /// The people index's uid.
    people: String,
}

/// Search backend: Meilisearch when configured and built, else Postgres.
pub struct SearchService<D> {
    data: D,
    meili: Option<Meili>,
    /// The indexes have been brought up to date at least once by this
    /// process, so their answers can be trusted.
    ready: AtomicBool,
    /// One sync at a time.
    syncing: tokio::sync::Mutex<()>,
}

/// (indexed, removed, rebuilt) for one index.
type Counts = (usize, usize, bool);

impl<D: Data> SearchService<D> {
    pub fn new(data: D, meili_url: Option<&str>, meili_key: Option<&str>) -> Self {
        Self::with_index(data, meili_url, meili_key, indexer::INDEX_NAME)
    }

    /// With the items index called `index` (tests give each their own); the
    /// people index is beside it.
    pub fn with_index(
        data: D,
        meili_url: Option<&str>,
        meili_key: Option<&str>,
        index: &str,
    ) -> Self {
        let meili = meili_url
            .filter(|u| !u.is_empty())
            .and_then(|url| meilisearch_sdk::client::Client::new(url, meili_key).ok())
            .map(|client| Meili {
                client,
                index: index.to_string(),
                people: indexer::people_uid(index),
            });
        Self {
            data,
            meili,
            ready: AtomicBool::new(false),
            syncing: tokio::sync::Mutex::new(()),
        }
    }

    /// Whether Meilisearch is configured.
    pub fn is_meilisearch_available(&self) -> bool {
        self.meili.is_some()
    }

    /// Whether Meilisearch answers searches now (configured and built).
    pub fn is_ready(&self) -> bool {
        self.meili.is_some() && self.ready.load(Ordering::Acquire)
    }

    /// Whether the search backend answers. Without Meilisearch, search is
    /// Postgres and as healthy as the database.
    pub async fn health(&self) -> BackendHealth {
        if let Some(ref m) = self.meili {
            let answer = tokio::time::timeout(Duration::from_secs(2), m.client.health()).await;
            return match answer {
                Ok(Ok(h)) => BackendHealth {
                    backend: "Meilisearch",
                    ok: h.status == "available",
                    detail: Some(if self.is_ready() {
                        h.status
                    } else {
                        format!(
                            "{}; the index is being built, searches use Postgres",
                            h.status
                        )
                    }),
                },
                Ok(Err(e)) => BackendHealth {
                    backend: "Meilisearch",
                    ok: false,
                    detail: Some(format!("{e}; searches fall back to Postgres")),
                },
                Err(_) => BackendHealth {
                    backend: "Meilisearch",
                    ok: false,
                    detail: Some(
                        "no answer within 2 s; searches fall back to Postgres".to_string(),
                    ),
                },
            };
        }
        BackendHealth {
            backend: "Postgres",
            ok: true,
            detail: None,
        }
    }

    async fn ranked(
        &self,
        people: bool,
        term: &str,
        filter: Option<String>,
        limit: usize,
    ) -> Option<Vec<(Uuid, f64)>> {
        let m = self.meili.as_ref().filter(|_| self.is_ready())?;
        let uid = if people { &m.people } else { &m.index };
        match query::ranked(&m.client, uid, term, filter.as_deref(), limit).await {
            Ok(hits) => Some(hits),
            Err(e) => {
                tracing::warn!("Meilisearch search failed, matching names in Postgres: {e}");
                None
            }
        }
    }

    /// The items best matching `term`, best first, of `item_types` (any when
    /// empty), at most `limit`. `None` when Meilisearch cannot answer (not
    /// configured, not built yet, or failing): the caller then matches names
    /// in Postgres.
    pub async fn ranked_ids(
        &self,
        term: &str,
        item_types: &[String],
        limit: usize,
    ) -> Option<Vec<Uuid>> {
        let hits = self
            .ranked(false, term, query::type_filter(item_types), limit)
            .await?;
        Some(hits.into_iter().map(|(id, _)| id).collect())
    }

    /// As [`ranked_ids`](Self::ranked_ids), with each match's tier: matches
    /// Meilisearch scores alike share one, so an app's sort orders them.
    pub async fn ranked_tiers(
        &self,
        term: &str,
        item_types: &[String],
        limit: usize,
    ) -> Option<(Vec<Uuid>, Vec<i32>)> {
        let hits = self
            .ranked(false, term, query::type_filter(item_types), limit)
            .await?;
        Some(tiers(hits))
    }

    /// The people best matching `term`, best first, of `person_types` (any
    /// when empty). `None` as for [`ranked_ids`](Self::ranked_ids).
    pub async fn ranked_people(
        &self,
        term: &str,
        person_types: &[String],
        limit: usize,
    ) -> Option<Vec<Uuid>> {
        let hits = self
            .ranked(
                true,
                term,
                query::any_of("person_type", person_types),
                limit,
            )
            .await?;
        Some(hits.into_iter().map(|(id, _)| id).collect())
    }

    /// Search for items by term, for search hints: Meilisearch's best
    /// matches with their scores, or names in Postgres.
    pub async fn search(
        &self,
        term: &str,
        limit: i64,
        item_types: Option<&[String]>,
    ) -> Result<Vec<SearchHit>, SearchError> {
        let types = item_types.unwrap_or_default();
        let filter = query::type_filter(types);
        if let Some(hits) = self
            .ranked(false, term, filter, limit.max(0) as usize)
            .await
        {
            let ids: Vec<Uuid> = hits.iter().map(|(id, _)| *id).collect();
            let rows = self.data.media_items().find_by_ids(&ids).await?;
            let mut by_id: std::collections::HashMap<Uuid, MediaItem> =
                rows.into_iter().map(|r| (r.id, r)).collect();
            return Ok(hits
                .into_iter()
                .filter_map(|(id, score)| {
                    by_id.remove(&id).map(|row| SearchHit {
                        score,
                        ..SearchHit::from(row)
                    })
                })
                .collect());
        }
        let items = self.data.media_items().search_by_name(term, limit).await?;
        Ok(items
            .into_iter()
            .filter(|i| types.is_empty() || types.iter().any(|t| t == &i.item_type))
            .map(SearchHit::from)
            .collect())
    }

    /// Search for people by term, for search hints: Meilisearch's best
    /// matches with their scores, or names in Postgres (score 0).
    pub async fn search_people(
        &self,
        term: &str,
        limit: i64,
    ) -> Result<Vec<(Person, f64)>, SearchError> {
        if let Some(hits) = self.ranked(true, term, None, limit.max(0) as usize).await {
            let ids: Vec<Uuid> = hits.iter().map(|(id, _)| *id).collect();
            let rows = self
                .data
                .people()
                .list_people_by_ids(&ids, &[], &[], limit, 0)
                .await?;
            let mut by_id: std::collections::HashMap<Uuid, Person> =
                rows.into_iter().map(|r| (r.id, r)).collect();
            return Ok(hits
                .into_iter()
                .filter_map(|(id, score)| by_id.remove(&id).map(|p| (p, score)))
                .collect());
        }
        let rows = self
            .data
            .people()
            .list_people(&[], &[], Some(term), limit, 0)
            .await?;
        Ok(rows.into_iter().map(|p| (p, 0.0)).collect())
    }

    /// Bring both indexes up to date: everything changed or removed since the
    /// last sync, or everything the first time. A no-op without Meilisearch.
    pub async fn sync(&self) -> Result<SyncReport, SearchError> {
        let Some(ref m) = self.meili else {
            return Ok(SyncReport::default());
        };
        let _one = self.syncing.lock().await;
        self.sync_locked(m).await
    }

    /// [`Self::sync`] with the lock held.
    async fn sync_locked(&self, m: &Meili) -> Result<SyncReport, SearchError> {
        let (indexed, removed, rebuilt) = self.sync_items(m).await?;
        let (people_indexed, people_removed, people_rebuilt) = self.sync_people(m).await?;
        let report = SyncReport {
            rebuilt,
            indexed,
            removed,
            people_indexed,
            people_removed,
            people_rebuilt,
        };
        self.ready.store(true, Ordering::Release);
        if report != SyncReport::default() {
            tracing::info!(
                "Search index: {} item(s){} and {} people{} indexed, {} and {} removed",
                report.indexed,
                if report.rebuilt { " (rebuilt)" } else { "" },
                report.people_indexed,
                if report.people_rebuilt {
                    " (rebuilt)"
                } else {
                    ""
                },
                report.removed,
                report.people_removed,
            );
        }
        Ok(report)
    }

    async fn sync_items(&self, m: &Meili) -> Result<Counts, SearchError> {
        let engine = SearchError::Engine;
        let Some(since) = self
            .cursor(m, &m.index, indexer::ITEM_FILTERS, indexer::ITEM_SEARCHABLE)
            .await?
        else {
            // Built whole and swapped in, so searches keep answering from the
            // old one meanwhile; what changes during the build comes next time.
            let snapshot = self.data.search_feed().snapshot().await?;
            let building = format!("{}_building", m.index);
            let settings = indexer::item_settings();
            indexer::drop_index(&m.client, &building).await;
            indexer::create(&m.client, &building, &settings, TASK_TIMEOUT)
                .await
                .map_err(engine)?;
            let (mut indexed, mut after) = (0, None);
            loop {
                let items = self.data.search_feed().all_page(after, PAGE).await?;
                let Some(last) = items.last() else { break };
                after = Some(last.id);
                let docs: Vec<indexer::ItemDoc> = items.iter().map(Into::into).collect();
                indexer::upsert(&m.client, &building, &docs, TASK_TIMEOUT)
                    .await
                    .map_err(engine)?;
                indexed += items.len();
            }
            indexer::swap_in(&m.client, &m.index, &building, TASK_TIMEOUT)
                .await
                .map_err(engine)?;
            indexer::drop_index(&m.client, &building).await;
            self.data
                .search_feed()
                .set_cursor(&m.index, &snapshot)
                .await?;
            return Ok((indexed, 0, true));
        };
        // Taken before reading, so what changes meanwhile comes next time.
        let snapshot = self.data.search_feed().snapshot().await?;
        let (mut indexed, mut removed, mut after) = (0, 0, None);
        loop {
            let page = self
                .data
                .search_feed()
                .changes(&since, after.as_ref(), PAGE)
                .await?;
            if !page.removed.is_empty() {
                indexer::delete(&m.client, &m.index, &page.removed, TASK_TIMEOUT)
                    .await
                    .map_err(engine)?;
                removed += page.removed.len();
            }
            if !page.items.is_empty() {
                let docs: Vec<indexer::ItemDoc> = page.items.iter().map(Into::into).collect();
                indexer::upsert(&m.client, &m.index, &docs, TASK_TIMEOUT)
                    .await
                    .map_err(engine)?;
                indexed += page.items.len();
            }
            match page.next {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        self.data
            .search_feed()
            .set_cursor(&m.index, &snapshot)
            .await?;
        Ok((indexed, removed, false))
    }

    async fn sync_people(&self, m: &Meili) -> Result<Counts, SearchError> {
        let engine = SearchError::Engine;
        let Some(since) = self
            .cursor(
                m,
                &m.people,
                indexer::PEOPLE_FILTERS,
                indexer::PEOPLE_SEARCHABLE,
            )
            .await?
        else {
            let snapshot = self.data.search_feed().snapshot().await?;
            let building = format!("{}_building", m.people);
            let settings = indexer::people_settings();
            indexer::drop_index(&m.client, &building).await;
            indexer::create(&m.client, &building, &settings, TASK_TIMEOUT)
                .await
                .map_err(engine)?;
            let (mut indexed, mut after) = (0, None);
            loop {
                let rows = self.data.search_feed().people_page(after, PAGE).await?;
                let Some(last) = rows.last() else { break };
                after = Some(last.id);
                let docs: Vec<indexer::PersonDoc> = rows.iter().map(Into::into).collect();
                indexer::upsert(&m.client, &building, &docs, TASK_TIMEOUT)
                    .await
                    .map_err(engine)?;
                indexed += rows.len();
            }
            indexer::swap_in(&m.client, &m.people, &building, TASK_TIMEOUT)
                .await
                .map_err(engine)?;
            indexer::drop_index(&m.client, &building).await;
            self.data
                .search_feed()
                .set_cursor(&m.people, &snapshot)
                .await?;
            return Ok((indexed, 0, true));
        };
        let snapshot = self.data.search_feed().snapshot().await?;
        let (mut indexed, mut removed, mut after) = (0, 0, None);
        loop {
            let page = self
                .data
                .search_feed()
                .people_changes(&since, after.as_ref(), PAGE)
                .await?;
            if !page.removed.is_empty() {
                indexer::delete(&m.client, &m.people, &page.removed, TASK_TIMEOUT)
                    .await
                    .map_err(engine)?;
                removed += page.removed.len();
            }
            if !page.people.is_empty() {
                let docs: Vec<indexer::PersonDoc> = page.people.iter().map(Into::into).collect();
                indexer::upsert(&m.client, &m.people, &docs, TASK_TIMEOUT)
                    .await
                    .map_err(engine)?;
                indexed += page.people.len();
            }
            match page.next {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        self.data
            .search_feed()
            .set_cursor(&m.people, &snapshot)
            .await?;
        Ok((indexed, removed, false))
    }

    /// Where the index `uid` was last brought up to; `None` (build it whole)
    /// when it never was, or when Meilisearch no longer has it as built: a
    /// Meilisearch without stored data loses its indexes on restart, and an
    /// update would then make a bare index that filters on nothing.
    async fn cursor(
        &self,
        m: &Meili,
        uid: &str,
        filters: &[&str],
        searchable: &[&str],
    ) -> Result<Option<String>, SearchError> {
        let Some(cursor) = self.data.search_feed().cursor(uid).await? else {
            return Ok(None);
        };
        if indexer::is_built(&m.client, uid, filters, searchable).await {
            return Ok(Some(cursor));
        }
        tracing::warn!("Search index {uid} is missing or incomplete in Meilisearch; rebuilding it");
        Ok(None)
    }

    /// Rebuild both indexes whole, now. The lock is held from clearing the
    /// cursors on, so a sync already running cannot write its cursor back.
    pub async fn reindex_all(&self) -> Result<(), SearchError> {
        if let Some(ref m) = self.meili {
            let _one = self.syncing.lock().await;
            self.data.search_feed().clear_cursor(&m.index).await?;
            self.data.search_feed().clear_cursor(&m.people).await?;
            self.sync_locked(m).await?;
        }
        Ok(())
    }

    /// Drop this service's indexes (tests clean up with it).
    pub async fn drop_indexes(&self) {
        if let Some(ref m) = self.meili {
            for uid in [&m.index, &m.people] {
                indexer::drop_index(&m.client, uid).await;
                indexer::drop_index(&m.client, &format!("{uid}_building")).await;
            }
        }
    }
}

/// Hits, best first, with a tier each: one per distinct score (to four
/// places), counting from 0.
pub fn tiers(hits: Vec<(Uuid, f64)>) -> (Vec<Uuid>, Vec<i32>) {
    let mut ids = Vec::with_capacity(hits.len());
    let mut tiers = Vec::with_capacity(hits.len());
    let mut tier = 0;
    let mut last = None;
    for (id, score) in hits {
        let score = (score * 10_000.0).round() as i64;
        if last.is_some_and(|l| l != score) {
            tier += 1;
        }
        last = Some(score);
        ids.push(id);
        tiers.push(tier);
    }
    (ids, tiers)
}

#[cfg(test)]
mod tier_tests {
    use super::*;

    #[test]
    fn equal_scores_share_a_tier() {
        let (a, b, c, d) = (
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let (ids, tiers) = tiers(vec![(a, 0.99), (b, 0.8), (c, 0.80001), (d, 0.5)]);
        assert_eq!(ids, vec![a, b, c, d]);
        assert_eq!(tiers, vec![0, 1, 1, 2]);
    }
}
