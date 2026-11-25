//! The Postgres data layer: [`PgData`] implements every trait in
//! `rustyfin_models::data`, one module per area, by running the queries in
//! [`crate::repo`]. Database errors become [`DataError`] here, so nothing
//! outside this crate sees sqlx.

mod display_preferences;

use rustyfin_models::data::{DataError, DataResult};
use sqlx::PgPool;

/// Postgres behind the data-layer traits. Cloning is cheap (the pool is
/// shared).
#[derive(Clone)]
pub struct PgData {
    pool: PgPool,
}

impl PgData {
    /// The data layer over an open pool.
    ///
    /// Transitional: while route handlers still hold the pool themselves,
    /// they wrap it here. Once every caller goes through the traits, the
    /// server opens the data layer directly and this goes.
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Turns a sqlx result into a data-layer result.
pub(crate) trait IntoData<T> {
    fn data(self) -> DataResult<T>;
}

impl<T> IntoData<T> for Result<T, sqlx::Error> {
    fn data(self) -> DataResult<T> {
        self.map_err(data_error)
    }
}

/// Postgres SQLSTATE for `foreign_key_violation`.
const PG_FOREIGN_KEY_VIOLATION: &str = "23503";

/// What kind of failure a sqlx error is, without the sqlx type.
pub(crate) fn data_error(e: sqlx::Error) -> DataError {
    if crate::pool::is_unavailable(&e) {
        return DataError::Unavailable(e.to_string());
    }
    let foreign_key = e
        .as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == PG_FOREIGN_KEY_VIOLATION);
    if foreign_key {
        DataError::MissingReference(e.to_string())
    } else {
        DataError::Other(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exhausted_pool_is_unavailable() {
        assert!(matches!(
            data_error(sqlx::Error::PoolTimedOut),
            DataError::Unavailable(_)
        ));
    }

    #[test]
    fn a_missing_row_is_a_fault() {
        assert!(matches!(
            data_error(sqlx::Error::RowNotFound),
            DataError::Other(_)
        ));
    }
}
