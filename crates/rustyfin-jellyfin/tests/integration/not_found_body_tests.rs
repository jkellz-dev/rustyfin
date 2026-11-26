//! Parity (plezy.hurl:1340): Jellyfin answers an unknown item with ASP.NET
//! problem details as JSON; Rustyfin sent plain text.
use crate::common;

use axum::http::StatusCode;
use uuid::Uuid;

#[tokio::test]
async fn a_404_is_problem_details_json() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let (status, body) = app.get(&format!("/Items/{}", Uuid::new_v4()), &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["status"], 404, "{body}");
    assert_eq!(body["title"], "Not Found", "{body}");
    assert!(body["type"].is_string(), "{body}");
    assert!(body["traceId"].is_string(), "{body}");
    app.cleanup().await;
}
