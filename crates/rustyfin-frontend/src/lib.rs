//! Frontend crate — embeds the Sycamore WASM build assets via `rust-embed` and serves them.
//! Falls back to a placeholder page if the frontend hasn't been built yet.

use axum::{
    Router,
    http::{StatusCode, Uri, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
pub mod assets;

use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../frontend/dist/"]
struct FrontendAssets;

const FALLBACK_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Rustyfin</title>
    <style>
        body { font-family: system-ui; background: #101010; color: #fff; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; }
        .container { text-align: center; max-width: 500px; }
        h1 { font-size: 2.5em; margin-bottom: 0.2em; }
        p { color: #888; font-size: 1.1em; }
        code { background: #222; padding: 2px 8px; border-radius: 4px; color: #6cf; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Rustyfin</h1>
        <p>Media server is running. The admin frontend has not been built yet.</p>
        <p>Build it with:<br><code>cd frontend && trunk build --release</code></p>
    </div>
</body>
</html>"#;

/// Create the frontend router. Serves under `/web` prefix with SPA fallback.
pub fn router() -> Router {
    Router::new()
        .route("/favicon.svg", get(serve_favicon))
        .route("/favicon.ico", get(serve_favicon))
        .route("/web", get(serve_index))
        .route("/web/", get(serve_index))
        .route("/web/{*path}", get(serve_frontend))
}

async fn serve_favicon() -> Response {
    serve_file("favicon.svg")
}

async fn serve_index() -> Response {
    serve_index_or_fallback()
}

async fn serve_frontend(uri: Uri) -> Response {
    let path = uri.path().strip_prefix("/web/").unwrap_or(uri.path());

    // Try exact file path first (JS, WASM, CSS, etc.)
    if !path.is_empty() {
        let resp = serve_file(path);
        if resp.status() != StatusCode::NOT_FOUND {
            return resp;
        }
    }

    // SPA fallback — serve index.html for all non-file routes
    serve_index_or_fallback()
}

fn serve_file(path: &str) -> Response {
    match FrontendAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data.to_vec(),
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Serve index.html, falling back to a placeholder if the frontend hasn't been built.
fn serve_index_or_fallback() -> Response {
    match FrontendAssets::get("index.html") {
        Some(content) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html")],
            content.data.to_vec(),
        )
            .into_response(),
        None => (StatusCode::OK, Html(FALLBACK_HTML)).into_response(),
    }
}
