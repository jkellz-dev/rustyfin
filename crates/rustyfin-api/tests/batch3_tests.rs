//! Batch 3 (parity, browse.hurl:355): a mixed list sorted by SortName orders
//! numbered items the way Jellyfin's sort names do: a season by its number
//! ("0001"), an episode by season then episode ("001 - 0001 - Name"), a track
//! by disc then track ("0001 - 0001 - Name"). So a season comes before its
//! episodes and tracks, and numbers sort as numbers.
#[allow(dead_code)]
mod common;

use axum::http::StatusCode;
use uuid::Uuid;

async fn numbered(
    app: &common::TestApp,
    name: &str,
    item_type: &str,
    library: Uuid,
    parent: Option<Uuid>,
    season: Option<i32>,
    index: Option<i32>,
) -> Uuid {
    let id = app
        .create_media_item(name, item_type, library, parent)
        .await;
    sqlx::query(
        "UPDATE media_items SET parent_index_number = $2, index_number = $3, sort_name = $4 \
         WHERE id = $1",
    )
    .bind(id)
    .bind(season)
    .bind(index)
    .bind(name.to_lowercase())
    .execute(&app.pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn a_sorted_mixed_list_orders_numbered_items_like_jellyfin() {
    let app = common::TestApp::new().await;
    let (_, token) = app.create_user("viewer", true).await;
    let lib = app.create_library("Mixed", "mixed").await;
    let show = numbered(&app, "Zed Show", "Series", lib, None, None, None).await;
    let s1 = numbered(&app, "Season 1", "Season", lib, Some(show), None, Some(1)).await;
    let s2 = numbered(&app, "Season 2", "Season", lib, Some(show), None, Some(2)).await;
    numbered(&app, "Alpha", "Episode", lib, Some(s1), Some(1), Some(10)).await;
    numbered(&app, "Beta", "Episode", lib, Some(s1), Some(1), Some(2)).await;
    numbered(&app, "Aardvark", "Episode", lib, Some(s2), Some(2), Some(1)).await;
    numbered(&app, "Song", "Audio", lib, None, Some(1), Some(3)).await;

    let (status, body) = app
        .get(
            &format!(
                "/Items?ParentId={lib}&Recursive=true&SortBy=SortName&SortOrder=Ascending\
                 &IncludeItemTypes=Season,Episode,Audio"
            ),
            &token,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let names: Vec<&str> = body["Items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["Name"].as_str().unwrap())
        .collect();
    // Keys: seasons "0001", "0002"; track "0001 - 0003 - song"; episodes
    // "001 - 0002 - beta", "001 - 0010 - alpha", "002 - 0001 - aardvark".
    assert_eq!(
        names,
        ["Season 1", "Song", "Season 2", "Beta", "Alpha", "Aardvark"],
        "{body}"
    );
    app.cleanup().await;
}
