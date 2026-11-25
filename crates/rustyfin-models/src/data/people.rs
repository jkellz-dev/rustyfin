use std::future::Future;
use uuid::Uuid;

use super::DataResult;

/// A person credited on an item: the person, their role (a character, say)
/// and their place in the billing order.
pub type Credit = (Person, Option<String>, Option<i32>);

/// A credit with the id of the item it is on.
pub type ItemCredit = (Uuid, Person, Option<String>, Option<i32>);

/// A person (actor, director, writer, ...).
#[derive(Debug, Clone)]
pub struct Person {
    pub id: Uuid,
    pub name: String,
    pub sort_name: Option<String>,
    pub person_type: String,
    pub overview: Option<String>,
    pub external_ids: Option<serde_json::Value>,
    pub image_url: Option<String>,
    pub provider_person_id: Option<String>,
}

/// A person in an item's cast or crew: their role and billing order.
#[derive(Debug, Clone)]
pub struct MediaItemPerson {
    pub item_id: Uuid,
    pub person_id: Uuid,
    pub role: Option<String>,
    pub sort_order: Option<i32>,
}

/// What the data layer keeps for `people`.
pub trait PeopleStore {
    /// Find a person by name (case-insensitive exact match).
    /// A person by id.
    fn find_by_id(&self, id: uuid::Uuid)
    -> impl Future<Output = DataResult<Option<Person>>> + Send;

    /// The person with this name, ignoring case.
    fn find_by_name(&self, name: &str) -> impl Future<Output = DataResult<Option<Person>>> + Send;

    /// Find a person by name and type.
    fn find_by_name_and_type(
        &self,
        name: &str,
        person_type: &str,
    ) -> impl Future<Output = DataResult<Option<Person>>> + Send;

    /// Insert a new person.
    fn insert(
        &self,
        name: &str,
        person_type: &str,
        sort_name: Option<&str>,
        external_ids: Option<&serde_json::Value>,
        image_url: Option<&str>,
        provider_person_id: Option<&str>,
    ) -> impl Future<Output = DataResult<Person>> + Send;

    /// Find a person by name and type, or create if not found.
    fn find_or_create(
        &self,
        name: &str,
        person_type: &str,
        image_url: Option<&str>,
        provider_person_id: Option<&str>,
    ) -> impl Future<Output = DataResult<Person>> + Send;

    /// Link a person to a media item. Uses ON CONFLICT to update role/sort_order if already linked.
    fn link_to_item(
        &self,
        item_id: uuid::Uuid,
        person_id: uuid::Uuid,
        role: Option<&str>,
        sort_order: Option<i32>,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// Delete all person links for a media item (used before re-linking).
    fn delete_links_by_item(
        &self,
        item_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<()>> + Send;

    /// List people with optional type filtering, search, and pagination.
    fn list_people(
        &self,
        person_types: &[String],
        exclude_person_types: &[String],
        search_term: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> impl Future<Output = DataResult<Vec<Person>>> + Send;

    /// People among `ids` (Meilisearch's best matches, best first) with the type
    /// filters, in that order, a page at a time.
    fn list_people_by_ids(
        &self,
        ids: &[uuid::Uuid],
        person_types: &[String],
        exclude_person_types: &[String],
        limit: i64,
        offset: i64,
    ) -> impl Future<Output = DataResult<Vec<Person>>> + Send;

    /// How many of `ids` pass the type filters.
    fn count_people_by_ids(
        &self,
        ids: &[uuid::Uuid],
        person_types: &[String],
        exclude_person_types: &[String],
    ) -> impl Future<Output = DataResult<i64>> + Send;

    /// Count people matching the given filters.
    fn count_people(
        &self,
        person_types: &[String],
        exclude_person_types: &[String],
        search_term: Option<&str>,
    ) -> impl Future<Output = DataResult<i64>> + Send;

    /// List people that have a provider_person_id but no Primary image row.
    fn list_missing_headshots(&self) -> impl Future<Output = DataResult<Vec<Person>>> + Send;

    /// Count items by type that a person appears in.
    fn count_items_by_person(
        &self,
        person_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<(String, i64)>>> + Send;

    /// The cast and crew of each of `item_ids`, billing order first.
    fn list_by_items(
        &self,
        item_ids: &[uuid::Uuid],
    ) -> impl Future<Output = DataResult<Vec<ItemCredit>>> + Send;

    /// Get people associated with a media item.
    fn list_by_item(
        &self,
        item_id: uuid::Uuid,
    ) -> impl Future<Output = DataResult<Vec<Credit>>> + Send;
}
