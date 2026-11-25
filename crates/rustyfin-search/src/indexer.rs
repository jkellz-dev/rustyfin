//! The Meilisearch indexes, their settings and documents: one for items
//! (films, shows, episodes, albums, artists, tracks...) and one for people.

use std::time::Duration;

use meilisearch_sdk::client::{Client, SwapIndexes};
use meilisearch_sdk::settings::Settings;
use meilisearch_sdk::task_info::TaskInfo;
use rustyfin_data::repo::media_items::MediaItemRow;
use rustyfin_data::repo::people::PersonRow;
use serde::Serialize;
use uuid::Uuid;

/// The items index's uid.
pub const INDEX_NAME: &str = "media_items";

/// The people index beside the items index `items`.
pub fn people_uid(items: &str) -> String {
    format!("{items}_people")
}

/// What an item is found by. Only what a search matches or narrows on: the
/// rest is read from Postgres.
#[derive(Serialize)]
pub struct ItemDoc {
    id: String,
    name: String,
    /// The name without the marks inside its words, when it has any.
    name_joined: Option<String>,
    original_title: Option<String>,
    series_name: Option<String>,
    genres: Option<Vec<String>>,
    overview: Option<String>,
    item_type: String,
    production_year: Option<i32>,
    library_id: Option<String>,
}

impl From<&MediaItemRow> for ItemDoc {
    fn from(item: &MediaItemRow) -> Self {
        Self {
            id: item.id.to_string(),
            name: item.name.clone(),
            name_joined: joined_name(&item.name),
            original_title: item
                .original_title
                .clone()
                .filter(|t| !t.eq_ignore_ascii_case(&item.name)),
            series_name: item.series_name.clone(),
            genres: item.genres.clone(),
            overview: item.overview.clone(),
            item_type: item.item_type.clone(),
            production_year: item.production_year,
            library_id: item.library_id.map(|id| id.to_string()),
        }
    }
}

/// What a person is found by.
#[derive(Serialize)]
pub struct PersonDoc {
    id: String,
    name: String,
    name_joined: Option<String>,
    person_type: String,
}

impl From<&PersonRow> for PersonDoc {
    fn from(p: &PersonRow) -> Self {
        Self {
            id: p.id.to_string(),
            name: p.name.clone(),
            name_joined: joined_name(&p.name),
            person_type: p.person_type.clone(),
        }
    }
}

/// A name with the marks inside its words dropped ("P.U.L.S.E" → "PULSE",
/// "AC/DC" → "ACDC"), when that changes it. Meilisearch splits such a word
/// at every mark, so "pulse" would find nothing; Jellyfin matches names with
/// their punctuation removed.
pub fn joined_name(name: &str) -> Option<String> {
    let words: Vec<String> = name
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect();
    let joined = words.join(" ");
    let plain = name.split_whitespace().collect::<Vec<_>>().join(" ");
    (!joined.is_empty() && joined != plain).then_some(joined)
}

/// The items index's searchable attributes, in order of weight: a match in
/// the name counts most, one in the overview least.
pub const ITEM_SEARCHABLE: &[&str] = &[
    "name",
    "name_joined",
    "original_title",
    "series_name",
    "genres",
    "overview",
];
/// The people index's searchable attributes.
pub const PEOPLE_SEARCHABLE: &[&str] = &["name", "name_joined"];

/// The items index's settings.
pub fn item_settings() -> Settings {
    Settings::new()
        .with_searchable_attributes(ITEM_SEARCHABLE)
        .with_filterable_attributes(ITEM_FILTERS)
}

/// What the items index filters on.
pub const ITEM_FILTERS: &[&str] = &["item_type", "library_id", "production_year"];
/// What the people index filters on.
pub const PEOPLE_FILTERS: &[&str] = &["person_type"];

/// Whether Meilisearch has the index `uid` with every attribute in
/// `filters` filterable and `searchable` searchable (as [`create`] leaves
/// it). An index made before a searchable attribute was added is rebuilt, so
/// every document gains it.
pub async fn is_built(client: &Client, uid: &str, filters: &[&str], searchable: &[&str]) -> bool {
    let Ok(index) = client.get_index(uid).await else {
        return false;
    };
    let Ok(settings) = index.get_settings().await else {
        return false;
    };
    let Ok(json) = serde_json::to_value(&settings) else {
        return false;
    };
    let have = json["filterableAttributes"].to_string();
    let search = json["searchableAttributes"].to_string();
    filters.iter().all(|f| have.contains(&format!("\"{f}\"")))
        && searchable
            .iter()
            .all(|f| search.contains(&format!("\"{f}\"")))
}

/// The people index's settings.
pub fn people_settings() -> Settings {
    Settings::new()
        .with_searchable_attributes(PEOPLE_SEARCHABLE)
        .with_filterable_attributes(PEOPLE_FILTERS)
}

async fn finish(client: &Client, task: TaskInfo, timeout: Duration) -> Result<(), String> {
    let task = task
        .wait_for_completion(client, Some(Duration::from_millis(50)), Some(timeout))
        .await
        .map_err(|e| e.to_string())?;
    if task.is_failure() {
        return Err(format!("{:?}", task.unwrap_failure()));
    }
    Ok(())
}

/// Create the index `uid` (if new) with `settings`.
pub async fn create(
    client: &Client,
    uid: &str,
    settings: &Settings,
    timeout: Duration,
) -> Result<(), String> {
    if client.get_index(uid).await.is_err() {
        let task = client
            .create_index(uid, Some("id"))
            .await
            .map_err(|e| e.to_string())?;
        finish(client, task, timeout).await?;
    }
    let task = client
        .index(uid)
        .set_settings(settings)
        .await
        .map_err(|e| e.to_string())?;
    finish(client, task, timeout).await
}

/// Add or update `docs` in `uid`, waiting until they are searchable.
pub async fn upsert<T: Serialize + Send + Sync>(
    client: &Client,
    uid: &str,
    docs: &[T],
    timeout: Duration,
) -> Result<(), String> {
    let task = client
        .index(uid)
        .add_or_update(docs, Some("id"))
        .await
        .map_err(|e| e.to_string())?;
    finish(client, task, timeout).await
}

/// Remove `ids` from `uid`.
pub async fn delete(
    client: &Client,
    uid: &str,
    ids: &[Uuid],
    timeout: Duration,
) -> Result<(), String> {
    let ids: Vec<String> = ids.iter().map(Uuid::to_string).collect();
    let task = client
        .index(uid)
        .delete_documents(&ids)
        .await
        .map_err(|e| e.to_string())?;
    finish(client, task, timeout).await
}

/// Put `building` in place of `uid` (creating `uid` first if need be, so the
/// swap has two sides).
pub async fn swap_in(
    client: &Client,
    uid: &str,
    building: &str,
    timeout: Duration,
) -> Result<(), String> {
    // The old index is dropped after the swap, so it is never given the
    // settings: re-indexing it first (a bare index Meilisearch made searches
    // every field) took seconds and logged Meilisearch errors for nothing.
    if client.get_index(uid).await.is_err() {
        let task = client
            .create_index(uid, Some("id"))
            .await
            .map_err(|e| e.to_string())?;
        finish(client, task, timeout).await?;
    }
    let task = client
        .swap_indexes([&SwapIndexes {
            indexes: (uid.to_string(), building.to_string()),
            rename: None,
        }])
        .await
        .map_err(|e| e.to_string())?;
    finish(client, task, timeout).await
}

/// Delete the index `uid` if there is one.
pub async fn drop_index(client: &Client, uid: &str) {
    if let Ok(task) = client.delete_index(uid).await {
        let _ = task
            .wait_for_completion(
                client,
                Some(Duration::from_millis(50)),
                Some(Duration::from_secs(60)),
            )
            .await;
    }
}

#[cfg(test)]
mod joined_name_tests {
    use super::joined_name;

    #[test]
    fn marks_inside_words_are_dropped() {
        assert_eq!(joined_name("P.U.L.S.E").as_deref(), Some("PULSE"));
        assert_eq!(joined_name("AC/DC").as_deref(), Some("ACDC"));
        assert_eq!(joined_name("J.J. Abrams").as_deref(), Some("JJ Abrams"));
        assert_eq!(joined_name("Pink Floyd"), None);
        assert_eq!(joined_name("..."), None);
    }
}
