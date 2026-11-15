use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Wrapper around sqlx's PgPool for convenience.
pub type DbPool = PgPool;

/// How long a request waits for a free connection before giving up with a 503
/// (sqlx's default, 30 s, holds the client that long and then fails anyway).
pub const ACQUIRE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Create a new database connection pool.
pub async fn create_pool(database_url: &str) -> Result<DbPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect(database_url)
        .await
}

/// Whether `e` means the database cannot be reached right now (every
/// connection busy, the pool shut, the server unreachable) rather than a bad
/// query: worth a 503 and a retry, not a 500.
pub fn is_unavailable(e: &sqlx::Error) -> bool {
    matches!(
        e,
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_)
    )
}

/// Run pending migrations against the database at runtime.
pub async fn run_migrations(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    let migrator = sqlx::migrate::Migrator::new(std::path::Path::new("./migrations")).await?;
    migrator.run(pool).await
}
