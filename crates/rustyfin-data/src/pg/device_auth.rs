use rustyfin_models::data::*;

use super::{IntoData, PgDeviceAuth};
use crate::repo::device_auth as repo;

impl DeviceAuthStore for PgDeviceAuth {
    async fn create(
        &self,
        device_code: &str,
        user_code: &str,
        client_info: &serde_json::Value,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> DataResult<DeviceAuth> {
        repo::create(&self.pool, device_code, user_code, client_info, expires_at)
            .await
            .data()
    }

    async fn find_by_device_code(&self, device_code: &str) -> DataResult<Option<DeviceAuth>> {
        repo::find_by_device_code(&self.pool, device_code)
            .await
            .data()
    }

    async fn find_by_user_code(&self, user_code: &str) -> DataResult<Option<DeviceAuth>> {
        repo::find_by_user_code(&self.pool, user_code).await.data()
    }

    async fn authorize(&self, id: uuid::Uuid, user_id: uuid::Uuid) -> DataResult<()> {
        repo::authorize(&self.pool, id, user_id).await.data()
    }

    async fn expire_old(&self) -> DataResult<u64> {
        repo::expire_old(&self.pool).await.data()
    }
}
