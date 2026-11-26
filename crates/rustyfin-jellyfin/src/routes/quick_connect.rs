use axum::Json;
use axum::http::StatusCode;

/// `GET /QuickConnect/Enabled` — Quick connect is disabled.
pub async fn get_enabled() -> Json<bool> {
    Json(false)
}

/// `POST /QuickConnect/Initiate` — Not available.
pub async fn initiate() -> StatusCode {
    StatusCode::UNAUTHORIZED
}

/// `GET /QuickConnect/Connect` — Not available.
pub async fn connect() -> StatusCode {
    StatusCode::UNAUTHORIZED
}

/// `POST /QuickConnect/Authorize` — Not available.
pub async fn authorize() -> StatusCode {
    StatusCode::UNAUTHORIZED
}
