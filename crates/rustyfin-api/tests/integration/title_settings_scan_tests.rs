//! A title's `.rustyfin.toml` overrides its NFO and folder name at scan time,
//! and editing it is picked up by the next scan.
use crate::common;

use axum::http::StatusCode;
use rustyfin_core::LibraryScanner;
use rustyfin_db::repo::libraries;
use serde_json::Value;
use uuid::Uuid;

async fn scan(app: &common::TestApp, lib: Uuid, root: &str) {
    LibraryScanner::new(app.pool.clone(), None)
        .scan(lib, &[root.to_string()])
        .await
        .expect("scan");
}

async fn films(app: &common::TestApp, token: &str, lib: Uuid) -> Vec<Value> {
    let (status, body) = app
        .get(
            &format!(
                "/Items?ParentId={lib}&IncludeItemTypes=Movie&Recursive=true&Fields=ProviderIds"
            ),
            token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut items = body["Items"].as_array().unwrap().clone();
    items.sort_by_key(|i| i["Name"].as_str().unwrap().to_string());
    items
}

#[tokio::test]
async fn title_settings_override_the_nfo_and_folder_name() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let root = std::env::temp_dir().join(format!("rf-tss-{}", Uuid::new_v4().simple()));
    let wrong = root.join("Wrong Match (2021)");
    std::fs::create_dir_all(&wrong).unwrap();
    std::fs::write(wrong.join("Wrong Match (2021).mkv"), b"").unwrap();
    std::fs::write(
        wrong.join("Wrong Match (2021).nfo"),
        "<movie><title>Wrong Match</title><uniqueid type=\"tmdb\">1</uniqueid></movie>",
    )
    .unwrap();
    std::fs::write(
        wrong.join(".rustyfin.toml"),
        "[ids]\ntmdb = 802217\n\n[metadata]\ntitle = \"Right Match\"\n",
    )
    .unwrap();
    let bare = root.join("Baby Driver 2 ()");
    std::fs::create_dir_all(&bare).unwrap();
    std::fs::write(bare.join("Baby Driver 2 ().mkv"), b"").unwrap();
    std::fs::write(
        bare.join(".rustyfin.toml"),
        "[metadata]\ntitle = \"Baby Driver 2\"\n",
    )
    .unwrap();

    let root_s = root.to_string_lossy().to_string();
    let lib = libraries::create(&app.pool, "Movies", "movies", std::slice::from_ref(&root_s))
        .await
        .unwrap()
        .id;
    scan(&app, lib, &root_s).await;
    let items = films(&app, &token, lib).await;
    let names: Vec<&str> = items.iter().map(|i| i["Name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["Baby Driver 2", "Right Match"],
        "no NFO needed either"
    );
    assert_eq!(items[1]["ProviderIds"]["Tmdb"], "802217", "{}", items[1]);

    // Editing the settings alone is enough for the next scan to notice.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(
        wrong.join(".rustyfin.toml"),
        "[metadata]\ntitle = \"Edited\"\n",
    )
    .unwrap();
    scan(&app, lib, &root_s).await;
    let items = films(&app, &token, lib).await;
    let names: Vec<&str> = items.iter().map(|i| i["Name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Baby Driver 2", "Edited"]);
    let _ = std::fs::remove_dir_all(&root);
    app.cleanup().await;
}
