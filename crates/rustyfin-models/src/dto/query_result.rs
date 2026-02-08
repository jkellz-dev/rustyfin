use serde::{Deserialize, Serialize};

/// Generic query result wrapper matching Jellyfin's `QueryResult<T>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct QueryResult<T> {
    pub items: Vec<T>,
    pub total_record_count: i32,
    pub start_index: i32,
}

impl<T> QueryResult<T> {
    pub fn new(items: Vec<T>, total_record_count: i32) -> Self {
        Self {
            items,
            total_record_count,
            start_index: 0,
        }
    }

    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            total_record_count: 0,
            start_index: 0,
        }
    }
}
