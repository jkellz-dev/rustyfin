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

    /// Too many failed logins: try again after this many seconds.
    #[error("Too many failed logins; retry in {0} s")]
    TooManyRequests(u64),
}

/// Lets a handler carry on without optional data (images, counts, people) when its
/// query fails, while saying so in the log.
///
/// This is for extras only. The data a route exists to return must use `?`, because
/// `.unwrap_or_default()` turns a database failure into an empty `200` that clients
/// cannot tell apart from a genuinely empty library.
pub trait DegradeExt<T> {
    fn or_log_default(self, what: &str) -> T;
}

impl<T: Default> DegradeExt<T> for Result<T, sqlx::Error> {
    fn or_log_default(self, what: &str) -> T {
        self.unwrap_or_else(|e| {
            tracing::warn!("{what} failed, continuing without it: {e}");
            T::default()
        })
    }
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
            ApiError::Database(e) if rustyfin_db::pool::is_unavailable(e) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            ApiError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
        };

        // A missing image or an unknown id is the client's answer, not a fault:
        // clients ask for every image type of every item they show.
        if status.is_server_error() {
            tracing::error!("API error: {self}");
        } else {
            tracing::debug!("API error: {self}");
        }
        let mut response = (status, self.to_string()).into_response();
        if let ApiError::TooManyRequests(secs) = self {
            response
                .headers_mut()
                .insert(axum::http::header::RETRY_AFTER, secs.into());
        } else if status == StatusCode::SERVICE_UNAVAILABLE {
            response
                .headers_mut()
                .insert(axum::http::header::RETRY_AFTER, 5u64.into());
        }
        response
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
