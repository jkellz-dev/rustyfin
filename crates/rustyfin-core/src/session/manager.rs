use sqlx::PgPool;

/// Manages active sessions and playback state.
pub struct SessionManager {
    pool: PgPool,
}

impl SessionManager {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Report playback started.
    pub async fn report_playback_start(
        &self,
        _user_id: uuid::Uuid,
        _item_id: uuid::Uuid,
    ) -> anyhow::Result<()> {
        let _ = &self.pool;
        tracing::info!("Playback start reported");
        Ok(())
    }

    /// Report playback progress.
    pub async fn report_playback_progress(
        &self,
        _user_id: uuid::Uuid,
        _item_id: uuid::Uuid,
        _position_ticks: i64,
    ) -> anyhow::Result<()> {
        let _ = &self.pool;
        Ok(())
    }

    /// Report playback stopped.
    pub async fn report_playback_stopped(
        &self,
        _user_id: uuid::Uuid,
        _item_id: uuid::Uuid,
        _position_ticks: Option<i64>,
    ) -> anyhow::Result<()> {
        let _ = &self.pool;
        tracing::info!("Playback stopped reported");
        Ok(())
    }
}
