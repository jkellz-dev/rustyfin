//! The search index: it ranks, the data layer's queries filter (owner,
//! 2026-11-19). Without an index (not configured, not built yet, or
//! failing) searches match names in the store.

use std::fmt;
use std::future::Future;
use uuid::Uuid;

use super::{DataError, MediaItem, Person};

/// Ids a ranked search asks the index for, at most.
pub const SEARCH_CANDIDATES: usize = 1_000;

/// A search hit with the fields needed to build a search hint.
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
    /// The index's ranking score (0 to 1); 0 when the store answered.
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
pub struct SearchHealth {
    /// The backend answering (`Meilisearch`, or `Postgres` as the fallback).
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

/// Why a search or a sync failed.
#[derive(Debug)]
pub enum SearchError {
    /// The store behind the index failed.
    Data(DataError),
    /// The index itself failed.
    Engine(String),
}

impl From<DataError> for SearchError {
    fn from(e: DataError) -> Self {
        SearchError::Data(e)
    }
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SearchError::Data(e) => write!(f, "{e}"),
            SearchError::Engine(e) => write!(f, "search engine error: {e}"),
        }
    }
}

impl std::error::Error for SearchError {}

/// The search index over the data layer's items and people.
pub trait SearchIndex: Send + Sync {
    /// Whether an index is configured.
    fn is_configured(&self) -> bool;

    /// Whether the index answers searches now (configured and built).
    fn is_ready(&self) -> bool;

    /// Whether the search backend answers.
    fn health(&self) -> impl Future<Output = SearchHealth> + Send;

    /// The items best matching `term`, best first, of `item_types` (any
    /// when empty), at most `limit`. `None` when the index cannot answer:
    /// the caller then matches names in the store.
    fn ranked_ids(
        &self,
        term: &str,
        item_types: &[String],
        limit: usize,
    ) -> impl Future<Output = Option<Vec<Uuid>>> + Send;

    /// As [`ranked_ids`](Self::ranked_ids), with each match's tier: matches
    /// scored alike share one, so an app's sort orders them.
    fn ranked_tiers(
        &self,
        term: &str,
        item_types: &[String],
        limit: usize,
    ) -> impl Future<Output = Option<(Vec<Uuid>, Vec<i32>)>> + Send;

    /// The people best matching `term`, best first, of `person_types` (any
    /// when empty). `None` as for [`ranked_ids`](Self::ranked_ids).
    fn ranked_people(
        &self,
        term: &str,
        person_types: &[String],
        limit: usize,
    ) -> impl Future<Output = Option<Vec<Uuid>>> + Send;

    /// Items by term, for search hints: the index's best matches with their
    /// scores, or names in the store.
    fn search(
        &self,
        term: &str,
        limit: i64,
        item_types: Option<&[String]>,
    ) -> impl Future<Output = Result<Vec<SearchHit>, SearchError>> + Send;

    /// People by term, for search hints: the index's best matches with their
    /// scores, or names in the store (score 0).
    fn search_people(
        &self,
        term: &str,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<(Person, f64)>, SearchError>> + Send;

    /// Bring the index up to date with the store; a no-op without one.
    fn sync(&self) -> impl Future<Output = Result<SyncReport, SearchError>> + Send;

    /// Rebuild the index whole, now.
    fn reindex_all(&self) -> impl Future<Output = Result<(), SearchError>> + Send;

    /// Drop the index (tests clean up with it).
    fn drop_indexes(&self) -> impl Future<Output = ()> + Send;
}
