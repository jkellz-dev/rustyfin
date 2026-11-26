//! Libraries set in `rustyfin.toml` (`[[library]]`; owner, 2026-11-26:
//! "my goal with configuration is to have it completely config file
//! driven"): made or updated at start, matched by name so a library keeps
//! its id, and changed only in the file.
use crate::common;

use axum::http::StatusCode;
use rustyfin_models::data::{Data, LibraryStore, LinkStore};
use rustyfin_services::libraries::{ConfiguredLibrary, Libraries};
use serde_json::{Value, json};
use uuid::Uuid;

fn real_dir(tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("rustyfin-cfg-{tag}-{}", Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir.to_string_lossy().into_owned()
}

fn entry(name: &str, kind: &str, paths: &[&str]) -> ConfiguredLibrary {
    ConfiguredLibrary {
        name: name.into(),
        kind: kind.into(),
        paths: paths.iter().map(|p| p.to_string()).collect(),
    }
}

fn find<'a>(folders: &'a Value, name: &str) -> &'a Value {
    folders
        .as_array()
        .expect("array")
        .iter()
        .find(|f| f["Name"] == name)
        .unwrap_or_else(|| panic!("library {name} not listed in {folders}"))
}

#[tokio::test]
async fn the_config_makes_libraries_and_updates_those_it_names_keeping_their_ids() {
    let app = common::TestApp::new().await;
    let (a, b, c) = (real_dir("a"), real_dir("b"), real_dir("c"));
    // Made before the file named libraries, as the admin page did.
    let movies = app
        .data
        .libraries()
        .create("movies", "tvshows", std::slice::from_ref(&a))
        .await
        .unwrap();
    let stray = app
        .data
        .libraries()
        .create("Old", "movies", std::slice::from_ref(&c))
        .await
        .unwrap();

    let config = [
        entry("Movies", "movies", &[&a, &b]),
        entry("Music", "music", &[&c]),
    ];
    let report = Libraries::over(&app.data)
        .apply_config(&config)
        .await
        .unwrap();
    assert_eq!(report.created, ["Music"]);
    assert_eq!(report.updated, ["Movies"]);
    assert_eq!(report.unconfigured, ["Old"]);
    assert!(report.refused.is_empty(), "{report:?}");

    let now = app
        .data
        .libraries()
        .find_by_id(movies.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(now.name, "Movies", "the file's case");
    assert_eq!(now.library_type, "movies", "the file's type");
    assert_eq!(now.paths, [a.clone(), b.clone()]);
    assert!(now.configured);
    let old = app
        .data
        .libraries()
        .find_by_id(stray.id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !old.configured,
        "a library the file does not name is kept, unmarked"
    );

    // Applying the same file again changes nothing; folder order is not a change.
    let again = [
        entry("Movies", "movies", &[&b, &a]),
        entry("Music", "music", &[&c]),
    ];
    let report = Libraries::over(&app.data)
        .apply_config(&again)
        .await
        .unwrap();
    assert!(
        report.created.is_empty() && report.updated.is_empty(),
        "{report:?}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn entries_that_cannot_be_libraries_are_refused_and_the_rest_applied() {
    let app = common::TestApp::new().await;
    let dir = real_dir("ok");
    app.data
        .links()
        .ensure_library(
            app.data
                .links()
                .upsert("Friend", "jellyfin", "http://friend")
                .await
                .unwrap(),
            "Stand-Up · Friend",
            "movies",
        )
        .await
        .unwrap();
    let config = [
        entry("Films", "movies", &[&dir]),
        entry("Films", "movies", &[&dir]),
        entry("Books", "books", &[&dir]),
        entry("Empty", "movies", &[" "]),
        entry("Stand-Up · Friend", "movies", &[&dir]),
    ];
    let report = Libraries::over(&app.data)
        .apply_config(&config)
        .await
        .unwrap();
    assert_eq!(report.created, ["Films"]);
    let refused: Vec<&str> = report.refused.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        refused,
        ["Films", "Books", "Empty", "Stand-Up · Friend"],
        "{report:?}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn libraries_in_the_config_are_not_changed_here() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("admin", true).await;
    let (a, b) = (real_dir("a"), real_dir("b"));
    let stray = app
        .data
        .libraries()
        .create("Old", "movies", std::slice::from_ref(&b))
        .await
        .unwrap();
    Libraries::over(&app.data)
        .apply_config(&[entry("Movies", "movies", &[&a])])
        .await
        .unwrap();
    let (_, list) = app.get("/Library/VirtualFolders", &token).await;
    let movies = find(&list, "Movies");
    assert_eq!(movies["Configured"], true, "{movies}");
    assert!(find(&list, "Old").get("Configured").is_none(), "{list}");
    let id = movies["ItemId"].as_str().unwrap().to_string();

    // Nothing is made or changed through the API while the file sets libraries.
    let conflict = |(status, body): (StatusCode, Value)| {
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
    };
    conflict(
        app.post(
            &format!("/Library/VirtualFolders?name=New&collectionType=movies&paths={b}"),
            &json!({}),
            &token,
        )
        .await,
    );
    conflict(
        app.post(
            &format!("/Library/VirtualFolders/Update?Id={id}"),
            &json!({ "Name": "Films", "Paths": [a] }),
            &token,
        )
        .await,
    );
    conflict(
        app.post(
            "/Library/VirtualFolders/Name?name=Movies&newName=Films",
            &json!({}),
            &token,
        )
        .await,
    );
    conflict(
        app.post(
            "/Library/VirtualFolders/Paths",
            &json!({ "Name": "Movies", "Path": b }),
            &token,
        )
        .await,
    );
    conflict(
        app.post(
            "/Library/VirtualFolders/LibraryOptions",
            &json!({ "Id": id, "LibraryOptions": { "Enabled": false } }),
            &token,
        )
        .await,
    );
    conflict(
        app.post(
            &format!("/Library/VirtualFolders/{id}/ScanSchedule"),
            &json!({ "ScanSchedule": "nightly" }),
            &token,
        )
        .await,
    );
    conflict(
        app.delete(&format!("/Library/VirtualFolders?Id={id}"), &token)
            .await,
    );
    // Nor a library the file does not name: `rustyfin prune` removes those.
    conflict(
        app.post(
            "/Library/VirtualFolders/Name?name=Old&newName=Older",
            &json!({}),
            &token,
        )
        .await,
    );
    conflict(
        app.delete(&format!("/Library/VirtualFolders?Id={}", stray.id), &token)
            .await,
    );

    let lib = app
        .data
        .libraries()
        .find_by_name("Movies")
        .await
        .unwrap()
        .unwrap();
    assert_eq!((lib.name.as_str(), lib.paths.clone()), ("Movies", vec![a]));
    assert_eq!(lib.scan_schedule, None);
    assert!(lib.scanning_enabled());

    // Out of the file again (no [[library]] at all): editable as before.
    Libraries::over(&app.data).apply_config(&[]).await.unwrap();
    let (status, body) = app
        .post(
            "/Library/VirtualFolders/Name?name=Movies&newName=Films",
            &json!({}),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn prune_removes_the_libraries_the_config_does_not_set() {
    let app = common::TestApp::new().await;
    let (a, b) = (real_dir("a"), real_dir("b"));
    let libs = app.data.libraries();
    let kept = libs
        .create("Movies", "movies", std::slice::from_ref(&a))
        .await
        .unwrap();
    let stray = libs
        .create("Old", "movies", std::slice::from_ref(&b))
        .await
        .unwrap();
    let link = app
        .data
        .links()
        .upsert("Friend", "jellyfin", "http://friend")
        .await
        .unwrap();
    let theirs = app
        .data
        .links()
        .ensure_library(link, "Stand-Up · Friend", "movies")
        .await
        .unwrap();
    let config = [entry("movies", "movies", &[&a])];

    // Without a config that sets libraries, nothing: it would empty the server.
    assert!(Libraries::over(&app.data).prune(&[], true).await.is_err());

    // A look first: what would go, nothing gone.
    let would: Vec<String> = Libraries::over(&app.data)
        .prune(&config, false)
        .await
        .unwrap()
        .into_iter()
        .map(|l| l.name)
        .collect();
    assert_eq!(would, ["Old"]);
    assert!(libs.find_by_id(stray.id).await.unwrap().is_some());

    let gone: Vec<String> = Libraries::over(&app.data)
        .prune(&config, true)
        .await
        .unwrap()
        .into_iter()
        .map(|l| l.name)
        .collect();
    assert_eq!(gone, ["Old"]);
    assert!(libs.find_by_id(stray.id).await.unwrap().is_none());
    assert!(
        libs.find_by_id(kept.id).await.unwrap().is_some(),
        "named in any case"
    );
    assert!(
        libs.find_by_id(theirs).await.unwrap().is_some(),
        "a linked server's library is the link's"
    );
    app.cleanup().await;
}
