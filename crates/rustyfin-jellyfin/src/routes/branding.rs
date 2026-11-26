use axum::Json;
use rustyfin_models::dto::BrandingOptions;

/// `GET /Branding/Configuration` — No auth required.
pub async fn get_branding_configuration() -> Json<BrandingOptions> {
    Json(BrandingOptions::default())
}

/// `GET /Branding/Css` (and `/Branding/Css.css`) — No auth required. Custom
/// CSS is Jellyfin web's; Rustyfin has none to add.
pub async fn get_branding_css() -> impl axum::response::IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "text/css")],
        String::new(),
    )
}
