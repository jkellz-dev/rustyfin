use std::future::Future;

use super::DataResult;

/// How many connections to the store are open and how many of those are
/// idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connections {
    pub open: u32,
    pub idle: u32,
}

/// The store itself: whether it answers and which one this is.
pub trait StatusStore {
    /// A round trip to the store.
    fn ping(&self) -> impl Future<Output = DataResult<()>> + Send;

    /// Names the database and schema this layer reads (`database.schema`):
    /// what caches of query results are keyed by.
    fn scope(&self) -> impl Future<Output = DataResult<String>> + Send;

    /// The connections open now.
    fn connections(&self) -> Connections;

    /// Refresh the query planner's statistics; returns what it refreshed.
    fn refresh_statistics(&self) -> impl Future<Output = DataResult<Vec<String>>> + Send;
}
