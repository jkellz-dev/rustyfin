//! Frontend crate — will embed the Svelte+WASM build assets and serve them via Axum.
//! For now, serves a minimal placeholder page.

use axum::{
    Router,
    response::{Html, IntoResponse},
    routing::get,
};

/// Create the frontend router.
pub fn router() -> Router {
    Router::new().fallback(get(serve_frontend))
}

async fn serve_frontend() -> impl IntoResponse {
    Html(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Rustyfin</title>
    <style>
        body { font-family: system-ui; background: #101010; color: #fff; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; }
        .container { text-align: center; }
        h1 { font-size: 2.5em; margin-bottom: 0.2em; }
        p { color: #888; font-size: 1.1em; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Rustyfin</h1>
        <p>Media server is running. Svelte frontend coming soon.</p>
    </div>
</body>
</html>"#,
    )
}
