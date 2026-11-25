use std::future::Future;

use super::DataResult;

/// What the data layer keeps for `user_library_access`.
pub trait LibraryAccessStore {
    /// Get the list of library IDs a user has been explicitly granted access to.
    fn get_enabled_libraries(
        &self,
        user_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<uuid::Uuid>>> + Send;

    /// Replace the set of libraries a user can access.
    /// Deletes all existing entries and inserts the new set.
    fn set_enabled_libraries(
        &self,
        user_id: uuid::Uuid,
        library_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Returns `None` if the user has access to all libraries (no filtering needed).
    /// Returns `Some(vec)` with the allowed library IDs if the user is restricted.
    fn get_accessible_library_ids(
        &self,
        user_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Option<Vec<uuid::Uuid>>>> + Send;
}
