//! A collection needs at least two films to be listed (owner, 2026-11-16:
//! "we have a ton of collections with only 1 item"). An automatic collection
//! (from NFOs) holding one film is kept, with its art, but left out of every
//! list, count and search; asked for by id it still answers. Collections made
//! by hand are always listed: one film may be the start of one.
use crate::common;

use axum::http::StatusCode;
use rustyfin_services::LibraryScanner;
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rf-single-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        Tree(root)
    }
    fn film(&self, folder: &str, set: &str) {
        let dir = self.0.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{folder}.mkv")), b"").unwrap();
        std::fs::write(
            dir.join(format!("{folder}.nfo")),
            format!("<movie><title>{folder}</title>{set}</movie>"),
        )
        .unwrap();
    }
    fn root(&self) -> String {
        self.0.to_string_lossy().to_string()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn names(body: &Value) -> Vec<String> {
    let mut n: Vec<String> = body["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("{body}"))
        .iter()
        .map(|i| i["Name"].as_str().unwrap().to_string())
        .collect();
    n.sort();
    n
}

#[tokio::test]
async fn collections_of_one_film_are_not_listed() {
    let app = common::TestApp::new().await;
    let (uid, token) = app.create_user("viewer", true).await;
    let t = Tree::new();
    let pair = r#"<set tmdbcolid="10"><name>Pair Collection</name></set>"#;
    let lone = r#"<set tmdbcolid="20"><name>Lone Collection</name></set>"#;
    t.film("A (2001)", pair);
    t.film("B (2002)", pair);
    t.film("C (2003)", lone);
    let lib = rustyfin_data::repo::libraries::create(&app.pool, "Movies", "movies", &[t.root()])
        .await
        .unwrap()
        .id;
    LibraryScanner::new(app.data.clone(), None)
        .scan(lib, &[t.root()])
        .await
        .unwrap();
    let lone_id: Uuid =
        sqlx::query_scalar("SELECT id FROM media_items WHERE name = 'Lone Collection'")
            .fetch_one(&app.pool)
            .await
            .expect("the one-film collection is kept");

    // A hand-made collection of one film is listed.
    let c = app.create_media_item("C (2003)", "Movie", lib, None).await;
    let (status, made) = app
        .post(
            &format!("/Collections?Name=Mine&Ids={c}"),
            &json!({}),
            &token,
        )
        .await;
    assert!(status.is_success(), "{made}");

    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=BoxSet", &token)
        .await;
    assert_eq!(names(&body), ["Mine", "Pair Collection"], "{body}");
    assert_eq!(body["TotalRecordCount"], 2, "{body}");

    let (_, views) = app.get(&format!("/Users/{uid}/Views"), &token).await;
    let view = views["Items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["CollectionType"] == "boxsets")
        .unwrap_or_else(|| panic!("{views}"))["Id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, in_view) = app.get(&format!("/Items?ParentId={view}"), &token).await;
    assert_eq!(names(&in_view), ["Mine", "Pair Collection"], "{in_view}");

    let (_, counts) = app.get("/Items/Counts", &token).await;
    assert_eq!(counts["BoxSetCount"], 2, "{counts}");

    // The film's page no longer links to it.
    let film: Uuid =
        sqlx::query_scalar("SELECT id FROM media_items WHERE path LIKE '%C (2003).mkv'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    let (_, of_film) = app.get(&format!("/Items/{film}/Collections"), &token).await;
    assert!(
        !names(&of_film).contains(&"Lone Collection".to_string()),
        "{of_film}"
    );

    // Asked for by id it still answers.
    let (status, item) = app
        .get(&format!("/Users/{uid}/Items/{lone_id}"), &token)
        .await;
    assert_eq!(status, StatusCode::OK, "{item}");
    app.cleanup().await;
}
