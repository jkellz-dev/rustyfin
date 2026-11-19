//! A Rustyfin linked to a friend's Rustyfin (roadmap 16b.2r, owner
//! 2026-11-18: "By the time a request hits rustyfin it has an identity").
//!
//! The friend's server is a real Rustyfin behind forward auth: a layer stands
//! in for the friend's proxy and vouches for "jonathan", an ordinary user the
//! friend's admin lets link. This server signs in through it with no password
//! and syncs from the change feed: the first sync takes everything, the next
//! ones only what changed.

#[allow(dead_code)]
mod common;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::Request;
use axum::http::HeaderValue;
use rustyfin_core::link::{LibraryMap, LinkKind, LinkSettings};
use rustyfin_db::repo::{media_items, users};
use serde_json::json;
use uuid::Uuid;

/// Serve the friend's app behind a stand-in proxy that vouches for `user`;
/// returns its URL. `seen` gets every `/Items` query with `Ids`.
async fn serve_behind_proxy(
    friend: &common::TestApp,
    user: &'static str,
    seen: Arc<Mutex<Vec<String>>>,
) -> String {
    let router =
        friend
            .router
            .clone()
            .layer(axum::middleware::map_request(move |mut req: Request| {
                let seen = seen.clone();
                async move {
                    if req.uri().path() == "/Items"
                        && let Some(q) = req.uri().query()
                        && q.contains("Ids=")
                    {
                        seen.lock().unwrap().push(q.to_string());
                    }
                    req.headers_mut()
                        .insert("Remote-User", HeaderValue::from_static(user));
                    req
                }
            }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}")
}

async fn film(friend: &common::TestApp, name: &str, tmdb: &str, library: Uuid) -> Uuid {
    let id = friend.create_media_item(name, "Movie", library, None).await;
    media_items::add_external_ids(&friend.pool, id, &json!({ "Tmdb": tmdb }))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO media_streams (item_id, stream_index, stream_type, codec, width, height, is_default) \
         VALUES ($1, 0, 'Video', 'hevc', 3840, 2160, true)",
    )
    .bind(id)
    .execute(&friend.pool)
    .await
    .unwrap();
    id
}

fn settings(url: &str) -> LinkSettings {
    LinkSettings {
        name: "Alex".into(),
        kind: LinkKind::Rustyfin,
        url: url.into(),
        // Forward auth: no user name or password to give.
        user: String::new(),
        password: String::new(),
        libraries: vec![LibraryMap {
            remote: "Films".into(),
            local: Some("Movies".into()),
        }],
    }
}

async fn names(app: &common::TestApp, link: Uuid) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM media_items WHERE link_id = $1 ORDER BY name")
        .bind(link)
        .fetch_all(&app.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_rustyfin_links_through_forward_auth_and_syncs_only_what_changed() {
    // The friend's server: an admin, a "Films" library it shares, a
    // "Private" one it does not, and jonathan, allowed to link and to see
    // Films only.
    let friend = common::TestApp::with_forward_auth().await;
    let (_admin, _) = friend.create_user("alex", true).await;
    let films = friend.create_library("Films", "movies").await;
    let private = friend.create_library("Private", "movies").await;
    let shared = film(&friend, "Shared Film", "100", films).await;
    let theirs = film(&friend, "Only Theirs", "200", films).await;
    film(&friend, "Secret", "300", private).await;
    let jonathan =
        users::upsert_from_forward_auth(&friend.pool, "jonathan", "jonathan", None, None)
            .await
            .unwrap();
    users::set_may_link(&friend.pool, jonathan.id, true)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET enable_all_libraries = false WHERE id = $1")
        .bind(jonathan.id)
        .execute(&friend.pool)
        .await
        .unwrap();
    rustyfin_db::repo::user_library_access::set_enabled_libraries(
        &friend.pool,
        jonathan.id,
        &[films],
    )
    .await
    .unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let url = serve_behind_proxy(&friend, "jonathan", seen.clone()).await;

    // This server: its own Movies with its own copy of "Shared Film".
    let home = common::TestApp::new().await;
    let movies = home.create_library("Movies", "movies").await;
    let mine = home
        .create_media_item("Shared Film", "Movie", movies, None)
        .await;
    media_items::add_external_ids(&home.pool, mine, &json!({"Tmdb": "100"}))
        .await
        .unwrap();
    let link = home
        .links
        .register(&home.pool, settings(&url))
        .await
        .unwrap();

    // First sync: everything jonathan can see, nothing he cannot.
    let report = home.links.sync(&home.pool, link).await.expect("first sync");
    assert!(report.errors.is_empty(), "{report:?}");
    assert_eq!(report.added, 2, "{report:?}");
    assert_eq!(names(&home, link).await, vec!["Only Theirs", "Shared Film"]);
    let owner: Option<Uuid> = sqlx::query_scalar(
        "SELECT owner_id FROM media_items WHERE link_id = $1 AND name = 'Shared Film'",
    )
    .bind(link)
    .fetch_one(&home.pool)
    .await
    .unwrap();
    assert_eq!(owner, Some(mine), "the friend's copy is a version of mine");
    // Signed in as jonathan himself: no other user was made over there.
    let friend_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&friend.pool)
        .await
        .unwrap();
    assert_eq!(friend_users, 2);

    // Nothing changed: no details fetched again.
    seen.lock().unwrap().clear();
    let report = home.links.sync(&home.pool, link).await.expect("quiet sync");
    assert_eq!(
        (report.added, report.updated, report.removed),
        (0, 0, 0),
        "{report:?}"
    );
    assert!(
        seen.lock().unwrap().is_empty(),
        "{:?}",
        seen.lock().unwrap()
    );

    // One renamed: only it is fetched.
    sqlx::query("UPDATE media_items SET name = 'Only Theirs (Extended)' WHERE id = $1")
        .bind(theirs)
        .execute(&friend.pool)
        .await
        .unwrap();
    let report = home.links.sync(&home.pool, link).await.expect("sync");
    assert_eq!((report.added, report.updated), (0, 1), "{report:?}");
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert!(
            seen[0].contains(&theirs.simple().to_string()) || seen[0].contains(&theirs.to_string())
        );
        assert!(!seen[0].contains(&shared.simple().to_string()), "{seen:?}");
    }
    assert_eq!(
        names(&home, link).await,
        vec!["Only Theirs (Extended)", "Shared Film"]
    );

    // One removed over there: gone here.
    sqlx::query("DELETE FROM media_items WHERE id = $1")
        .bind(theirs)
        .execute(&friend.pool)
        .await
        .unwrap();
    let report = home.links.sync(&home.pool, link).await.expect("sync");
    assert_eq!(report.removed, 1, "{report:?}");
    assert_eq!(names(&home, link).await, vec!["Shared Film"]);

    // The friend's admin stops jonathan linking: the sync says why, and
    // keeps what it had.
    users::set_may_link(&friend.pool, jonathan.id, false)
        .await
        .unwrap();
    let failed = home.links.sync(&home.pool, link).await;
    let why = match failed {
        Err(e) => format!("{e:#}"),
        Ok(report) => report.errors.join("; "),
    };
    assert!(why.contains("403"), "{why}");
    assert_eq!(names(&home, link).await, vec!["Shared Film"]);
    home.cleanup().await;
    friend.cleanup().await;
}
