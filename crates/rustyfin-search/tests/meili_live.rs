//! Live checks against a real Meilisearch. Run with:
//!
//! ```text
//! MEILI_URL=http://127.0.0.1:7700 MEILI_KEY=... \
//!   cargo test -p rustyfin-search --features meilisearch -- --ignored --nocapture
//! ```
#![cfg(feature = "meilisearch")]

use meilisearch_sdk::client::Client;
use rustyfin_search::{indexer, query};

fn client() -> Client {
    let url = std::env::var("MEILI_URL").expect("MEILI_URL");
    let key = std::env::var("MEILI_KEY").ok();
    Client::new(url, key).expect("client")
}

#[tokio::test]
#[ignore = "needs a running Meilisearch (MEILI_URL, MEILI_KEY)"]
async fn search_succeeds_on_an_existing_index() {
    let client = client();
    indexer::ensure_index(&client).await.expect("ensure_index");

    let result = query::search(&client, "findable", 20, None).await;
    assert!(result.is_ok(), "search failed: {result:?}");
}
