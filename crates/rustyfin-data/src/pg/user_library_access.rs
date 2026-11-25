use rustyfin_models::data::*;

use super::{IntoData, PgUserLibraryAccess};
use crate::repo::user_library_access as repo;

impl LibraryAccessStore for PgUserLibraryAccess {
    async fn get_enabled_libraries(&self, user_id: uuid::Uuid) -> DataResult<Vec<uuid::Uuid>> {
        repo::get_enabled_libraries(&self.pool, user_id)
            .await
            .data()
    }

    async fn set_enabled_libraries(
        &self,
        user_id: uuid::Uuid,
        library_ids: &[uuid::Uuid],
    ) -> DataResult<()> {
        repo::set_enabled_libraries(&self.pool, user_id, library_ids)
            .await
            .data()
    }

    async fn get_accessible_library_ids(
        &self,
        user_id: uuid::Uuid,
    ) -> DataResult<Option<Vec<uuid::Uuid>>> {
        repo::get_accessible_library_ids(&self.pool, user_id)
            .await
            .data()
    }
}
