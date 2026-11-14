//! The admin's pages are revalidated and its build files cached.
//! (Tests build with an empty `frontend/dist`, so the index is the fallback page.)

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

#[tokio::test]
async fn the_admin_page_is_revalidated_by_etag() {
    let app = rustyfin_frontend::router();
    let resp = app
        .clone()
        .oneshot(Request::get("/web/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()[header::CACHE_CONTROL], "no-cache");
    let etag = resp.headers()[header::ETAG].to_str().unwrap().to_string();

    let again = app
        .oneshot(
            Request::get("/web/media")
                .header(header::IF_NONE_MATCH, &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
}
