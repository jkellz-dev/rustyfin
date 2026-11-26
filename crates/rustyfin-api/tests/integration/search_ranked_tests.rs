//! Meilisearch ranks, Postgres filters (owner, 2026-11-19). These run against
//! a real Meilisearch (`MEILI_URL`; CI and the builder start one); each test
//! has its own index.

use crate::common;

use axum::http::StatusCode;
use rustyfin_models::data::SearchIndex;
use serde_json::Value;
use uuid::Uuid;

fn names(list: &Value) -> Vec<String> {
    list["Items"]
        .as_array()
        .unwrap_or_else(|| panic!("{list}"))
        .iter()
        .map(|i| i["Name"].as_str().unwrap().to_string())
        .collect()
}

async fn films(app: &common::TestApp, lib: Uuid, names: &[&str]) -> Vec<Uuid> {
    let mut ids = Vec::new();
    for name in names {
        ids.push(app.create_media_item(name, "Movie", lib, None).await);
    }
    ids
}

#[tokio::test]
async fn a_search_forgives_typos_and_ranks_best_first() {
    let app = common::TestApp::with_meilisearch().await;
    let (_, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("Films", "movies").await;
    films(&app, lib, &["Alien", "Aliens", "Heat"]).await;
    let report = app.search.sync().await.expect("sync");
    assert!(report.rebuilt, "{report:?}");
    assert_eq!(report.indexed, 3, "{report:?}");

    // A typo: no name contains "alein", yet both films are found.
    let (status, list) = app
        .get(
            "/Items?SearchTerm=alein&Recursive=true&IncludeItemTypes=Movie",
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let mut found = names(&list);
    found.sort();
    assert_eq!(found, vec!["Alien", "Aliens"], "{list}");
    assert_eq!(list["TotalRecordCount"], 2, "{list}");

    // Best first, whether or not the app sorts: Jellyfin puts its search
    // score before the app's sort (SqliteItemRepository.GetOrderByText).
    let (_, list) = app
        .get("/Items?SearchTerm=aliens&Recursive=true", &token)
        .await;
    assert_eq!(names(&list), vec!["Aliens", "Alien"], "{list}");
    let (_, list) = app
        .get(
            "/Items?SearchTerm=aliens&Recursive=true&SortBy=SortName",
            &token,
        )
        .await;
    assert_eq!(names(&list), vec!["Aliens", "Alien"], "{list}");

    // Search hints come from the same index.
    let (_, hints) = app.get("/Search/Hints?SearchTerm=alein", &token).await;
    let hinted: Vec<&str> = hints["SearchHints"]
        .as_array()
        .unwrap_or_else(|| panic!("{hints}"))
        .iter()
        .filter_map(|h| h["Name"].as_str())
        .collect();
    assert!(hinted.contains(&"Alien"), "{hints}");
    app.cleanup().await;
}

#[tokio::test]
async fn the_index_follows_renames_and_removals() {
    let app = common::TestApp::with_meilisearch().await;
    let (_, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("Films", "movies").await;
    let ids = films(&app, lib, &["Heat", "Ronin"]).await;
    app.search.sync().await.expect("first sync");

    sqlx::query("UPDATE media_items SET name = 'Heatwave' WHERE id = $1")
        .bind(ids[0])
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM media_items WHERE id = $1")
        .bind(ids[1])
        .execute(&app.pool)
        .await
        .unwrap();
    let report = app.search.sync().await.expect("sync");
    assert!(!report.rebuilt, "only what changed: {report:?}");
    assert_eq!((report.indexed, report.removed), (1, 1), "{report:?}");

    // Found by its new name, with a typo (so by Meilisearch, not by name in
    // Postgres).
    let (_, list) = app
        .get("/Items?SearchTerm=haetwave&Recursive=true", &token)
        .await;
    assert_eq!(names(&list), vec!["Heatwave"], "{list}");
    // Gone from the index itself, not only filtered out.
    let left = app
        .search
        .ranked_ids("ronin", &[], 10)
        .await
        .expect("ready");
    assert!(left.is_empty(), "{left:?}");
    // Nothing changed: nothing to do.
    let report = app.search.sync().await.expect("sync");
    assert_eq!((report.indexed, report.removed), (0, 0), "{report:?}");
    app.cleanup().await;
}

#[tokio::test]
async fn postgres_still_filters_what_meilisearch_finds() {
    let app = common::TestApp::with_meilisearch().await;
    let (_, token) = app.create_user("viewer", false).await;
    let films_lib = app.create_library("Films", "movies").await;
    let docs = app.create_library("Docs", "movies").await;
    films(&app, films_lib, &["Planet Terror"]).await;
    films(&app, docs, &["Planet Earth"]).await;
    app.search.sync().await.expect("sync");

    let (_, list) = app
        .get(
            &format!("/Items?SearchTerm=planet&Recursive=true&ParentId={docs}"),
            &token,
        )
        .await;
    assert_eq!(names(&list), vec!["Planet Earth"], "{list}");
    let (_, list) = app
        .get(
            "/Items?SearchTerm=planet&Recursive=true&Limit=1&StartIndex=1&SortBy=SortName",
            &token,
        )
        .await;
    assert_eq!(names(&list), vec!["Planet Terror"], "{list}");
    assert_eq!(list["TotalRecordCount"], 2, "{list}");
    app.cleanup().await;
}

#[tokio::test]
async fn without_a_built_index_names_are_matched_in_postgres() {
    let app = common::TestApp::with_meilisearch().await;
    let (_, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("Films", "movies").await;
    films(&app, lib, &["Heat"]).await;
    // No sync yet: the index is not trusted, Postgres answers.
    assert!(app.search.ranked_ids("heat", &[], 10).await.is_none());
    let (_, list) = app
        .get("/Items?SearchTerm=hea&Recursive=true", &token)
        .await;
    assert_eq!(names(&list), vec!["Heat"], "{list}");
    app.cleanup().await;
}

/// Owner, 2026-11-20: people and artists through Meilisearch too.
#[tokio::test]
async fn people_and_artists_are_found_with_typos() {
    let app = common::TestApp::with_meilisearch().await;
    let (_, token) = app.create_user("viewer", false).await;
    let music = app.create_library("Music", "music").await;
    app.create_media_item("Radiohead", "MusicArtist", music, None)
        .await;
    app.create_media_item("Portishead", "MusicArtist", music, None)
        .await;
    for (name, kind) in [
        ("Sigourney Weaver", "Actor"),
        ("Ridley Scott", "Director"),
        ("Tom Skerritt", "Actor"),
    ] {
        rustyfin_data::repo::people::insert(&app.pool, name, kind, None, None, None, None)
            .await
            .unwrap();
    }
    let report = app.search.sync().await.expect("sync");
    assert_eq!(report.people_indexed, 3, "{report:?}");

    // Typos: no name contains "radiohed" or "sigorney".
    let (status, list) = app.get("/Artists?SearchTerm=radiohed", &token).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(names(&list), vec!["Radiohead"], "{list}");
    assert_eq!(list["TotalRecordCount"], 1, "{list}");
    let (_, list) = app.get("/Persons?SearchTerm=sigorney", &token).await;
    assert_eq!(names(&list), vec!["Sigourney Weaver"], "{list}");
    assert_eq!(list["TotalRecordCount"], 1, "{list}");
    // Postgres still filters: Ridley Scott is no actor.
    let (_, list) = app
        .get("/Persons?SearchTerm=scot&PersonTypes=Actor", &token)
        .await;
    assert_eq!(names(&list), Vec::<String>::new(), "{list}");

    // Search hints list people too (Jellyfin's IncludePeople, on by default).
    let (_, hints) = app.get("/Search/Hints?SearchTerm=sigorney", &token).await;
    let hinted: Vec<(&str, &str)> = hints["SearchHints"]
        .as_array()
        .unwrap_or_else(|| panic!("{hints}"))
        .iter()
        .map(|h| (h["Name"].as_str().unwrap(), h["Type"].as_str().unwrap()))
        .collect();
    assert_eq!(hinted, vec![("Sigourney Weaver", "Person")], "{hints}");
    let (_, hints) = app
        .get(
            "/Search/Hints?SearchTerm=sigorney&IncludePeople=false",
            &token,
        )
        .await;
    assert_eq!(
        hints["SearchHints"].as_array().map(Vec::len),
        Some(0),
        "{hints}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn the_people_index_follows_renames_and_removals() {
    let app = common::TestApp::with_meilisearch().await;
    let (_, token) = app.create_user("viewer", false).await;
    let weaver = rustyfin_data::repo::people::insert(
        &app.pool,
        "Sigourney Weaver",
        "Actor",
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let scott = rustyfin_data::repo::people::insert(
        &app.pool,
        "Ridley Scott",
        "Director",
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    app.search.sync().await.expect("first sync");

    sqlx::query("UPDATE people SET name = 'Sigourney Weaver-Simpson' WHERE id = $1")
        .bind(weaver.id)
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM people WHERE id = $1")
        .bind(scott.id)
        .execute(&app.pool)
        .await
        .unwrap();
    let report = app.search.sync().await.expect("sync");
    assert_eq!(
        (report.people_indexed, report.people_removed),
        (1, 1),
        "{report:?}"
    );
    let (_, list) = app.get("/Persons?SearchTerm=simpsom", &token).await;
    assert_eq!(names(&list), vec!["Sigourney Weaver-Simpson"], "{list}");
    let left = app
        .search
        .ranked_people("ridley", &[], 10)
        .await
        .expect("ready");
    assert!(left.is_empty(), "{left:?}");
    app.cleanup().await;
}

/// Live, 2026-11-20: Meilisearch kept its data in an emptyDir, so a restart
/// lost both indexes while Postgres kept the sync cursors. The next sync
/// only sent what had changed, Meilisearch made a bare `media_items` for it,
/// and every search fell back to Postgres ("Attribute `item_type` is not
/// filterable"). A lost index (or one without its settings) is rebuilt whole.
#[tokio::test]
async fn an_index_meilisearch_lost_is_rebuilt_whole() {
    let app = common::TestApp::with_meilisearch().await;
    let (_, token) = app.create_user("viewer", false).await;
    let lib = app.create_library("Films", "movies").await;
    films(&app, lib, &["Alien", "Aliens", "Heat"]).await;
    app.search.sync().await.expect("first sync");

    // Meilisearch restarts with nothing; one film changes meanwhile.
    app.search.drop_indexes().await;
    sqlx::query("UPDATE media_items SET overview = 'Changed' WHERE name = 'Heat'")
        .execute(&app.pool)
        .await
        .unwrap();
    let report = app.search.sync().await.expect("sync");
    assert!(report.rebuilt, "{report:?}");
    assert_eq!(
        report.indexed, 3,
        "every film, not just the changed one: {report:?}"
    );
    assert!(report.people_rebuilt, "{report:?}");

    // Answered by Meilisearch (a typo, a type filter), not the fallback.
    let (status, list) = app
        .get(
            "/Items?SearchTerm=alein&Recursive=true&IncludeItemTypes=Movie",
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let mut found = names(&list);
    found.sort();
    assert_eq!(found, vec!["Alien", "Aliens"], "{list}");
    app.cleanup().await;
}

/// Owner, 2026-11-22: "Finamp ... search sucks. It doesn't appear to be
/// hitting meilisearch at all". Finamp searches as you type and always sends
/// its sort (`SortBy=SortName`), so the best match sank among the
/// alphabetical rest of Meilisearch's candidates.
#[tokio::test]
async fn a_sorted_search_as_you_type_puts_the_best_match_first() {
    let app = common::TestApp::with_meilisearch().await;
    let (user, token) = app.create_user("listener", false).await;
    let lib = app.create_library("Music", "music").await;
    for name in [
        "Aardvark Blues",
        "Blue Sky",
        "Abyss of Blue Skies",
        "Skyline",
    ] {
        app.create_media_item(name, "Audio", lib, None).await;
    }
    app.search.sync().await.expect("sync");

    let (status, list) = app
        .get(
            &format!(
                "/Users/{user}/Items?IncludeItemTypes=Audio&Recursive=true\
                 &SortBy=SortName&SortOrder=Ascending&SearchTerm=blue%20sky&Limit=100"
            ),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(names(&list)[0], "Blue Sky", "{list}");
    app.cleanup().await;
}

/// Owner, 2026-11-22: "Looking for Pink Floyd's 'p.u.l.s.e' album finds
/// nothing when searching 'pulse'." Meilisearch splits "P.U.L.S.E" into
/// letters; Jellyfin matches names with their punctuation removed.
#[tokio::test]
async fn names_with_marks_inside_words_are_found_without_them() {
    let app = common::TestApp::with_meilisearch().await;
    let (_, token) = app.create_user("listener", false).await;
    let lib = app.create_library("Music", "music").await;
    app.create_media_item("P.U.L.S.E", "MusicAlbum", lib, None)
        .await;
    app.create_media_item("AC/DC", "MusicArtist", lib, None)
        .await;
    app.create_media_item("Pulsar", "MusicAlbum", lib, None)
        .await;
    app.search.sync().await.expect("sync");

    for (term, kind, want) in [
        ("pulse", "MusicAlbum", "P.U.L.S.E"),
        ("acdc", "MusicArtist", "AC/DC"),
    ] {
        let (status, list) = app
            .get(
                &format!(
                    "/Items?SearchTerm={term}&Recursive=true&IncludeItemTypes={kind}&SortBy=SortName"
                ),
                &token,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{list}");
        assert_eq!(
            names(&list).first().map(String::as_str),
            Some(want),
            "{term}: {list}"
        );
    }
    app.cleanup().await;
}
