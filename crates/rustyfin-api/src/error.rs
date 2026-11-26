//! The API's errors and the statuses they answer with.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rustyfin_models::data::DataError;

/// Why a request failed.
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

    /// Something on this side failed (a linked server not answering).
    #[error("Internal error: {0}")]
    Internal(String),

    /// A data-layer failure.
    #[error("{0}")]
    Data(#[from] DataError),
}

impl From<rustyfin_services::ServiceError> for ApiError {
    fn from(e: rustyfin_services::ServiceError) -> Self {
        use rustyfin_services::ServiceError;
        match e {
            ServiceError::NotFound => ApiError::NotFound,
            ServiceError::Forbidden => ApiError::Forbidden,
            ServiceError::Conflict(why) => ApiError::Conflict(why),
            ServiceError::Invalid(why) => ApiError::BadRequest(why),
            ServiceError::Data(e) => ApiError::Data(e),
        }
    }
}

/// A 404's body, as the routes answered before this API had its own crate
/// (problem details: `type`, `title`, `status`, `traceId`).
fn not_found_body() -> Response {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let body = serde_json::json!({
        "type": "https://tools.ietf.org/html/rfc9110#section-15.5.5",
        "title": "Not Found",
        "status": 404,
        "traceId": format!("00-{}-{}-00", id, &id[..16]),
    });
    (
        StatusCode::NOT_FOUND,
        [(
            axum::http::header::CONTENT_TYPE,
            "application/json; charset=utf-8",
        )],
        body.to_string(),
    )
        .into_response()
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Data(DataError::MissingReference(_)) => StatusCode::NOT_FOUND,
            ApiError::Data(DataError::Unavailable(_)) => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::Data(DataError::Other(_)) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if status.is_server_error() {
            tracing::error!("API error: {self}");
        } else {
            tracing::debug!("API error: {self}");
        }
        let mut response = if status == StatusCode::NOT_FOUND {
            not_found_body()
        } else {
            (status, self.to_string()).into_response()
        };
        if status == StatusCode::SERVICE_UNAVAILABLE {
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
    fn data_errors_answer_with_their_statuses() {
        let status = |e| ApiError::Data(e).into_response().status();
        assert_eq!(
            status(DataError::MissingReference("fk".into())),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status(DataError::Unavailable("pool".into())),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            status(DataError::Other("bad".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
