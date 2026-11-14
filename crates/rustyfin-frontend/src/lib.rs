//! Frontend crate — embeds the Sycamore WASM build assets via `rust-embed` and serves them.
//! Falls back to a placeholder page if the frontend hasn't been built yet.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use assets::Encoding;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
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
    warm_compressed();
    Router::new()
        .route("/favicon.svg", get(serve_favicon))
        .route("/favicon.ico", get(serve_favicon))
        .route("/web", get(serve_index))
        .route("/web/", get(serve_index))
        .route("/web/{*path}", get(serve_frontend))
}

async fn serve_favicon(headers: HeaderMap) -> Response {
    serve_file("favicon.svg", &headers)
}

async fn serve_index(headers: HeaderMap) -> Response {
    serve_index_or_fallback(&headers)
}

async fn serve_frontend(uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().strip_prefix("/web/").unwrap_or(uri.path());

    // Try exact file path first (JS, WASM, CSS, etc.)
    if !path.is_empty() {
        let resp = serve_file(path, &headers);
        if resp.status() != StatusCode::NOT_FOUND {
            return resp;
        }
    }

    // SPA fallback — serve index.html for all non-file routes
    serve_index_or_fallback(&headers)
}

type Compressed = Mutex<HashMap<(String, Encoding), Arc<Vec<u8>>>>;

fn compressed() -> &'static Compressed {
    static CACHE: OnceLock<Compressed> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Compress every text and WASM file once, in the background at startup, so no
/// request waits on it; until a file is ready it goes out uncompressed.
fn warm_compressed() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::spawn(|| {
            for path in FrontendAssets::iter() {
                let mime = mime_guess::from_path(path.as_ref()).first_or_octet_stream();
                if !assets::is_compressible(mime.as_ref()) {
                    continue;
                }
                let Some(file) = FrontendAssets::get(&path) else {
                    continue;
                };
                for enc in [Encoding::Brotli, Encoding::Gzip] {
                    let body = Arc::new(assets::compress(&file.data, enc));
                    compressed()
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert((path.to_string(), enc), body);
                }
            }
        });
    });
}

fn serve_file(path: &str, headers: &HeaderMap) -> Response {
    let Some(content) = FrontendAssets::get(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let etag = assets::etag(&content.metadata.sha256_hash());
    respond(
        path,
        mime.as_ref(),
        &etag,
        assets::cache_control(path),
        &content.data,
        headers,
    )
}

/// One file's answer: 304 when the client has it, else the body, compressed
/// when the client accepts it and a compressed copy is ready.
fn respond(
    path: &str,
    mime: &str,
    etag: &str,
    cache_control: &str,
    data: &[u8],
    headers: &HeaderMap,
) -> Response {
    let if_none_match = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok());
    let compressible = assets::is_compressible(mime);
    let mut builder = Response::builder()
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, cache_control);
    if compressible {
        builder = builder.header(header::VARY, "Accept-Encoding");
    }
    if assets::none_match(if_none_match, etag) {
        return builder
            .status(StatusCode::NOT_MODIFIED)
            .body(Body::empty())
            .unwrap();
    }
    builder = builder.header(header::CONTENT_TYPE, mime);
    if compressible {
        let accept = headers
            .get(header::ACCEPT_ENCODING)
            .and_then(|v| v.to_str().ok());
        let enc = assets::choose_encoding(accept);
        let ready = compressed()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(path.to_string(), enc))
            .cloned();
        if let (Some(body), Some(value)) = (ready, enc.header_value()) {
            return builder
                .header(header::CONTENT_ENCODING, value)
                .status(StatusCode::OK)
                .body(Body::from(body.as_ref().clone()))
                .unwrap();
        }
    }
    builder
        .status(StatusCode::OK)
        .body(Body::from(data.to_vec()))
        .unwrap()
}

/// Serve index.html, falling back to a placeholder if the frontend hasn't been built.
fn serve_index_or_fallback(headers: &HeaderMap) -> Response {
    match FrontendAssets::get("index.html") {
        Some(_) => serve_file("index.html", headers),
        None => respond(
            "",
            "text/html",
            "\"fallback-1\"",
            "no-cache",
            FALLBACK_HTML.as_bytes(),
            headers,
        ),
    }
}
