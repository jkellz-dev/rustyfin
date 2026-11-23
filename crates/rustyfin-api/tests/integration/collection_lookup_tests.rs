//! Collections split by stale NFOs (owner, 2026-11-16): 28 Days Later and
//! 28 Weeks Later's NFOs name "28 Days/Weeks Later Collection" with no id;
//! 28 Years Later's (newer NFO) names "28 Days/Weeks/Years Later
//! Collection" with `tmdbcolid` 1565. Once `rustyfin artwork
//! --check-collections` has recorded TMDB's answer in the films'
//! `.rustyfin.toml`, the scan makes one collection of all three.
use crate::common;

use rustyfin_core::LibraryScanner;
use std::path::PathBuf;
use uuid::Uuid;

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rf-cl-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        Tree(root)
    }
    fn write(&self, rel: &str, body: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    fn film(&self, folder: &str, set: &str) {
        self.write(&format!("{folder}/{folder}.mkv"), "");
        self.write(
            &format!("{folder}/{folder}.nfo"),
            &format!("<movie><title>{folder}</title>{set}</movie>"),
        );
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

async fn sets(app: &common::TestApp, token: &str) -> Vec<(String, usize)> {
    let (_, body) = app
        .get("/Items?Recursive=true&IncludeItemTypes=BoxSet", token)
        .await;
    let mut out = Vec::new();
    for s in body["Items"].as_array().unwrap() {
        let id = s["Id"].as_str().unwrap();
        let (_, films) = app.get(&format!("/Items?ParentId={id}"), token).await;
        out.push((
            s["Name"].as_str().unwrap().to_string(),
            films["Items"].as_array().unwrap().len(),
        ));
    }
    out.sort();
    out
}

#[tokio::test]
async fn a_collection_recorded_from_tmdb_joins_films_with_stale_nfos() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let t = Tree::new();
    let old = "<set>28 Days/Weeks Later Collection</set>";
    let new = r#"<set tmdbcolid="1565"><name>28 Days/Weeks/Years Later Collection</name></set>"#;
    t.film("28 Days Later (2002) [tmdbid-170]", old);
    t.film("28 Weeks Later (2007) [tmdbid-1562]", old);
    t.film("28 Years Later (2025) [tmdbid-1100988]", new);
    let lib = rustyfin_db::repo::libraries::create(&app.pool, "Movies", "movies", &[t.root()])
        .await
        .unwrap()
        .id;
    let scanner = LibraryScanner::new(app.pool.clone(), None);
    scanner.scan(lib, &[t.root()]).await.unwrap();
    // Without a lookup, the stale names stay apart (as before); the new
    // name's collection has one film, so it is not listed.
    assert_eq!(
        sets(&app, &token).await,
        [("28 Days/Weeks Later Collection".to_string(), 2)]
    );

    let found = "[collection]\ntmdb = 1565\nname = \"28 Days/Weeks/Years Later Collection\"\n\
                 checked = 2026-11-16T20:00:00Z\n";
    t.write("28 Days Later (2002) [tmdbid-170]/.rustyfin.toml", found);
    t.write("28 Weeks Later (2007) [tmdbid-1562]/.rustyfin.toml", found);
    scanner.scan(lib, &[t.root()]).await.unwrap();
    assert_eq!(
        sets(&app, &token).await,
        [("28 Days/Weeks/Years Later Collection".to_string(), 3)]
    );
    app.cleanup().await;
}
