#[allow(dead_code)]
mod common;

/// Smoke test: the test harness can stand up and tear down cleanly.
#[tokio::test]
async fn test_app_starts_and_cleans_up() {
    let app = common::TestApp::new().await;

    // Verify we can hit a public endpoint
    let (status, body) = app.get_no_auth("/System/Info/Public").await;
    assert_eq!(status, 200);
    assert!(body.get("ServerName").is_some());

    app.cleanup().await;
}
