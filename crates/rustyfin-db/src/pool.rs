use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Wrapper around sqlx's PgPool for convenience.
pub type DbPool = PgPool;

/// Create a new database connection pool.
pub async fn create_pool(database_url: &str) -> Result<DbPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(20)
        .connect(database_url)
        .await
}

/// Run pending migrations against the database at runtime.
pub async fn run_migrations(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    let migrator = sqlx::migrate::Migrator::new(std::path::Path::new("./migrations")).await?;
    migrator.run(pool).await
}
