//! Search through Meilisearch: it ranks, Postgres filters (owner,
//! 2026-11-19).
//!
//! Meilisearch holds every shared item's searchable text (name, original
//! title, show name, genres, overview) and answers a search with the
//! best-matching ids, typo-tolerant and ranked. The routes then read those
//! ids from Postgres with all their other filters (access, folder, played,
//! linked servers) and keep Meilisearch's order unless the app sorts.
//!
//! [`SearchService::sync`] keeps the index in step from the database's own
//! change stamps (`change_xid`, `media_removals`, as the link feed does), so
//! a rename shows up within one sync instead of at the next full rebuild.
//! Without Meilisearch, or while it is down or not yet built, searches match
//! names in Postgres.

pub mod indexer;
pub mod query;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rustyfin_db::repo::{media_items, search_feed};
use sqlx::PgPool;
use uuid::Uuid;

/// Items per Meilisearch batch when indexing.
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
}

impl From<media_items::MediaItemRow> for SearchHit {
    fn from(item: media_items::MediaItemRow) -> Self {
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
    /// The index was built whole (first sync, or after a reset).
    pub rebuilt: bool,
    pub indexed: usize,
    pub removed: usize,
}

/// Search error type.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Search engine error: {0}")]
    Engine(String),
}

struct Meili {
    client: meilisearch_sdk::client::Client,
    /// The items index's uid (`media_items`, or prefixed in tests).
    index: String,
}

/// Search backend: Meilisearch when configured and built, else Postgres.
pub struct SearchService {
    pool: PgPool,
    meili: Option<Meili>,
    /// The index has been brought up to date at least once by this process,
    /// so its answers can be trusted.
    ready: AtomicBool,
    /// One sync at a time.
    syncing: tokio::sync::Mutex<()>,
}

impl SearchService {
    pub fn new(pool: PgPool, meili_url: Option<&str>, meili_key: Option<&str>) -> Self {
        Self::with_index(pool, meili_url, meili_key, indexer::INDEX_NAME)
    }

    /// With the items index called `index` (tests give each their own).
    pub fn with_index(
        pool: PgPool,
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
            });
        Self {
            pool,
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

    /// The ids best matching `term`, best first, of `item_types` (any when
    /// empty), at most `limit`. `None` when Meilisearch cannot answer (not
    /// configured, not built yet, or failing): the caller then matches names
    /// in Postgres.
    pub async fn ranked_ids(
        &self,
        term: &str,
        item_types: &[String],
        limit: usize,
    ) -> Option<Vec<Uuid>> {
        let m = self.meili.as_ref().filter(|_| self.is_ready())?;
        match query::ranked_ids(&m.client, &m.index, term, item_types, limit).await {
            Ok(ids) => Some(ids),
            Err(e) => {
                tracing::warn!("Meilisearch search failed, matching names in Postgres: {e}");
                None
            }
        }
    }

    /// Search for items by term, for search hints: Meilisearch's best
    /// matches, or names in Postgres.
    pub async fn search(
        &self,
        term: &str,
        limit: i64,
        item_types: Option<&[String]>,
    ) -> Result<Vec<SearchHit>, SearchError> {
        let types = item_types.unwrap_or_default();
        if let Some(ids) = self.ranked_ids(term, types, limit.max(0) as usize).await {
            let rows = media_items::find_by_ids(&self.pool, &ids).await?;
            let mut by_id: std::collections::HashMap<Uuid, media_items::MediaItemRow> =
                rows.into_iter().map(|r| (r.id, r)).collect();
            return Ok(ids
                .iter()
                .filter_map(|id| by_id.remove(id))
                .map(SearchHit::from)
                .collect());
        }
        let items = media_items::search_by_name(&self.pool, term, limit).await?;
        Ok(items
            .into_iter()
            .filter(|i| types.is_empty() || types.iter().any(|t| t == &i.item_type))
            .map(SearchHit::from)
            .collect())
    }

    /// Bring the index up to date: everything changed or removed since the
    /// last sync, or the whole library the first time. A no-op without
    /// Meilisearch.
    pub async fn sync(&self) -> Result<SyncReport, SearchError> {
        let Some(ref m) = self.meili else {
            return Ok(SyncReport::default());
        };
        let _one = self.syncing.lock().await;
        let report = match search_feed::cursor(&self.pool, &m.index).await? {
            None => self.rebuild(m).await?,
            Some(since) => {
                // Taken before reading, so what changes meanwhile comes next time.
                let snapshot = search_feed::snapshot(&self.pool).await?;
                let mut report = SyncReport::default();
                let mut after = None;
                loop {
                    let page =
                        search_feed::changes(&self.pool, &since, after.as_ref(), PAGE).await?;
                    if !page.removed.is_empty() {
                        indexer::delete(&m.client, &m.index, &page.removed, TASK_TIMEOUT)
                            .await
                            .map_err(SearchError::Engine)?;
                        report.removed += page.removed.len();
                    }
                    if !page.items.is_empty() {
                        indexer::upsert(&m.client, &m.index, &page.items, TASK_TIMEOUT)
                            .await
                            .map_err(SearchError::Engine)?;
                        report.indexed += page.items.len();
                    }
                    match page.next {
                        Some(next) => after = Some(next),
                        None => break,
                    }
                }
                search_feed::set_cursor(&self.pool, &m.index, &snapshot).await?;
                report
            }
        };
        self.ready.store(true, Ordering::Release);
        if report.rebuilt || report.indexed > 0 || report.removed > 0 {
            tracing::info!(
                "Search index: {} item(s) indexed, {} removed{}",
                report.indexed,
                report.removed,
                if report.rebuilt { " (rebuilt)" } else { "" }
            );
        }
        Ok(report)
    }

    /// Build the index whole and swap it in, so searches keep answering from
    /// the old one meanwhile; what changes during the build comes with the
    /// next sync.
    async fn rebuild(&self, m: &Meili) -> Result<SyncReport, SearchError> {
        let snapshot = search_feed::snapshot(&self.pool).await?;
        let building = format!("{}_building", m.index);
        indexer::drop_index(&m.client, &building).await;
        indexer::create(&m.client, &building, TASK_TIMEOUT)
            .await
            .map_err(SearchError::Engine)?;
        let mut indexed = 0;
        let mut after = None;
        loop {
            let items = search_feed::all_page(&self.pool, after, PAGE).await?;
            let Some(last) = items.last() else { break };
            after = Some(last.id);
            indexer::upsert(&m.client, &building, &items, TASK_TIMEOUT)
                .await
                .map_err(SearchError::Engine)?;
            indexed += items.len();
        }
        indexer::swap_in(&m.client, &m.index, &building, TASK_TIMEOUT)
            .await
            .map_err(SearchError::Engine)?;
        indexer::drop_index(&m.client, &building).await;
        search_feed::set_cursor(&self.pool, &m.index, &snapshot).await?;
        Ok(SyncReport {
            rebuilt: true,
            indexed,
            removed: 0,
        })
    }

    /// Rebuild the index whole at the next [`sync`](Self::sync), and run it.
    pub async fn reindex_all(&self) -> Result<(), SearchError> {
        if let Some(ref m) = self.meili {
            search_feed::clear_cursor(&self.pool, &m.index).await?;
            self.sync().await?;
        }
        Ok(())
    }

    /// Drop this service's indexes (tests clean up with it).
    pub async fn drop_indexes(&self) {
        if let Some(ref m) = self.meili {
            indexer::drop_index(&m.client, &m.index).await;
            indexer::drop_index(&m.client, &format!("{}_building", m.index)).await;
        }
    }
}
