use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// API error type.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Not found")]
    NotFound,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Postgres SQLSTATE for `foreign_key_violation`.
const PG_FOREIGN_KEY_VIOLATION: &str = "23503";

fn is_foreign_key_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == PG_FOREIGN_KEY_VIOLATION)
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            // A foreign-key violation means the request referenced a row
            // that does not exist (e.g. user data for an unknown item), which
            // Jellyfin reports as 404 rather than a server error.
            ApiError::Database(e) if is_foreign_key_violation(e) => StatusCode::NOT_FOUND,
            ApiError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        tracing::error!("API error: {self}");
        (status, self.to_string()).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_database_sqlx_error_is_not_fk_violation() {
        assert!(!is_foreign_key_violation(&sqlx::Error::RowNotFound));
    }

    #[test]
    fn plain_database_error_maps_to_500() {
        let resp = ApiError::Database(sqlx::Error::RowNotFound).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
