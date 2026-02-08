use axum::Json;
use rustyfin_models::dto::BrandingOptions;

/// `GET /Branding/Configuration` — No auth required.
pub async fn get_branding_configuration() -> Json<BrandingOptions> {
    Json(BrandingOptions::default())
}

/// `GET /Branding/Css` — No auth required.
pub async fn get_branding_css() -> String {
    String::new()
}
