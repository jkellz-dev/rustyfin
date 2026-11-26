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
//! The sync keeps both indexes in step from the database's own
//! change stamps (`change_xid`, `media_removals`, `people_removals`), so a
//! rename shows up within one sync instead of at the next full rebuild.
//! Without Meilisearch, or while it is down or not yet built, searches match
//! names in Postgres.

pub(crate) mod indexer;
pub(crate) mod query;

use rustyfin_models::data::{
    Data, MediaItem, MediaItemStore, PeopleStore, Person, SearchError, SearchFeedStore,
    SearchHealth as BackendHealth, SearchHit, SearchIndex, SyncReport,
};
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use uuid::Uuid;

/// Documents per Meilisearch batch when indexing.
const PAGE: i64 = 5_000;

/// How long an index task may take before the sync gives up for this round.
const TASK_TIMEOUT: Duration = Duration::from_secs(600);

struct Meili {
    client: meilisearch_sdk::client::Client,
    /// The items index's uid (`media_items`, or prefixed in tests).
    index: String,
    /// The people index's uid.
    people: String,
}

/// Meilisearch, when configured, and what the process knows of it; shared
/// by every copy of the data layer.
pub(crate) struct Engine {
    meili: Option<Meili>,
    /// The indexes have been brought up to date at least once by this
    /// process, so their answers can be trusted.
    ready: AtomicBool,
    /// One sync at a time.
    syncing: tokio::sync::Mutex<()>,
}

impl Engine {
    /// No Meilisearch: searches match names in Postgres. One for the
    /// process, shared by every data layer built without one.
    pub(crate) fn none() -> Arc<Self> {
        static NONE: std::sync::LazyLock<Arc<Engine>> =
            std::sync::LazyLock::new(|| Arc::new(Engine::new(None, None, indexer::INDEX_NAME)));
        NONE.clone()
    }

    /// Meilisearch at `url` (none when empty), with the items index called
    /// `index` (tests give each their own); the people index is beside it.
    pub(crate) fn new(url: Option<&str>, key: Option<&str>, index: &str) -> Self {
        let meili = url
            .filter(|u| !u.is_empty())
            .and_then(|url| meilisearch_sdk::client::Client::new(url, key).ok())
            .map(|client| Meili {
                client,
                index: index.to_string(),
                people: indexer::people_uid(index),
            });
        Self {
            meili,
            ready: AtomicBool::new(false),
            syncing: tokio::sync::Mutex::new(()),
        }
    }
}

/// The search index of [`crate::PgData`]: Meilisearch when configured and
/// built, else Postgres.
#[derive(Clone)]
pub struct PgSearch {
    pub(crate) pool: PgPool,
    pub(crate) engine: Arc<Engine>,
}

/// One search, over the data it reads.
struct SearchService<D> {
    data: D,
    engine: Arc<Engine>,
}

/// (indexed, removed, rebuilt) for one index.
type Counts = (usize, usize, bool);

impl<D: Data> SearchService<D> {
    /// Whether Meilisearch is configured.
    fn is_meilisearch_available(&self) -> bool {
        self.engine.meili.is_some()
    }

    /// Whether Meilisearch answers searches now (configured and built).
    fn is_ready(&self) -> bool {
        self.engine.meili.is_some() && self.engine.ready.load(Ordering::Acquire)
    }

    /// Whether the search backend answers. Without Meilisearch, search is
    /// Postgres and as healthy as the database.
    async fn health(&self) -> BackendHealth {
        if let Some(ref m) = self.engine.meili {
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
        let m = self.engine.meili.as_ref().filter(|_| self.is_ready())?;
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
    async fn ranked_ids(
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
    async fn ranked_tiers(
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
    async fn ranked_people(
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
    async fn search(
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
    async fn search_people(
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
    async fn sync(&self) -> Result<SyncReport, SearchError> {
        let Some(ref m) = self.engine.meili else {
            return Ok(SyncReport::default());
        };
        let _one = self.engine.syncing.lock().await;
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
        self.engine.ready.store(true, Ordering::Release);
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
    async fn reindex_all(&self) -> Result<(), SearchError> {
        if let Some(ref m) = self.engine.meili {
            let _one = self.engine.syncing.lock().await;
            self.data.search_feed().clear_cursor(&m.index).await?;
            self.data.search_feed().clear_cursor(&m.people).await?;
            self.sync_locked(m).await?;
        }
        Ok(())
    }

    /// Drop this service's indexes (tests clean up with it).
    async fn drop_indexes(&self) {
        if let Some(ref m) = self.engine.meili {
            for uid in [&m.index, &m.people] {
                indexer::drop_index(&m.client, uid).await;
                indexer::drop_index(&m.client, &format!("{uid}_building")).await;
            }
        }
    }
}

impl PgSearch {
    fn service(&self) -> SearchService<crate::PgData> {
        SearchService {
            data: crate::PgData::from_pool(&self.pool),
            engine: self.engine.clone(),
        }
    }
}

impl SearchIndex for PgSearch {
    fn is_configured(&self) -> bool {
        self.engine.meili.is_some()
    }

    fn is_ready(&self) -> bool {
        self.service().is_ready()
    }

    async fn health(&self) -> BackendHealth {
        self.service().health().await
    }

    async fn ranked_ids(
        &self,
        term: &str,
        item_types: &[String],
        limit: usize,
    ) -> Option<Vec<Uuid>> {
        self.service().ranked_ids(term, item_types, limit).await
    }

    async fn ranked_tiers(
        &self,
        term: &str,
        item_types: &[String],
        limit: usize,
    ) -> Option<(Vec<Uuid>, Vec<i32>)> {
        self.service().ranked_tiers(term, item_types, limit).await
    }

    async fn ranked_people(
        &self,
        term: &str,
        person_types: &[String],
        limit: usize,
    ) -> Option<Vec<Uuid>> {
        self.service()
            .ranked_people(term, person_types, limit)
            .await
    }

    async fn search(
        &self,
        term: &str,
        limit: i64,
        item_types: Option<&[String]>,
    ) -> Result<Vec<SearchHit>, SearchError> {
        self.service().search(term, limit, item_types).await
    }

    async fn search_people(
        &self,
        term: &str,
        limit: i64,
    ) -> Result<Vec<(Person, f64)>, SearchError> {
        self.service().search_people(term, limit).await
    }

    async fn sync(&self) -> Result<SyncReport, SearchError> {
        self.service().sync().await
    }

    async fn reindex_all(&self) -> Result<(), SearchError> {
        self.service().reindex_all().await
    }

    async fn drop_indexes(&self) {
        self.service().drop_indexes().await
    }
}

/// Hits, best first, with a tier each: one per distinct score (to four
/// places), counting from 0.
fn tiers(hits: Vec<(Uuid, f64)>) -> (Vec<Uuid>, Vec<i32>) {
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
