use sqlx::PgPool;

/// Library scanner that discovers media files and populates the database.
pub struct LibraryScanner {
    pool: PgPool,
}

impl LibraryScanner {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Scan a library path and ingest media items.
    pub async fn scan(&self, _library_id: uuid::Uuid, _paths: &[String]) -> anyhow::Result<()> {
        tracing::info!("Library scanning not yet implemented");
        let _ = &self.pool;
        Ok(())
    }
}
