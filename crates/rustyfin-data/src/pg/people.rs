use rustyfin_models::data::*;

use super::{IntoData, PgPeople};
use crate::repo::people as repo;

impl PeopleStore for PgPeople {
    async fn find_by_id(&self, id: uuid::Uuid) -> DataResult<Option<Person>> {
        repo::find_by_id(&self.pool, id).await.data()
    }

    async fn find_by_name(&self, name: &str) -> DataResult<Option<Person>> {
        repo::find_by_name(&self.pool, name).await.data()
    }

    async fn find_by_name_and_type(
        &self,
        name: &str,
        person_type: &str,
    ) -> DataResult<Option<Person>> {
        repo::find_by_name_and_type(&self.pool, name, person_type)
            .await
            .data()
    }

    async fn insert(
        &self,
        name: &str,
        person_type: &str,
        sort_name: Option<&str>,
        external_ids: Option<&serde_json::Value>,
        image_url: Option<&str>,
        provider_person_id: Option<&str>,
    ) -> DataResult<Person> {
        repo::insert(
            &self.pool,
            name,
            person_type,
            sort_name,
            external_ids,
            image_url,
            provider_person_id,
        )
        .await
        .data()
    }

    async fn find_or_create(
        &self,
        name: &str,
        person_type: &str,
        image_url: Option<&str>,
        provider_person_id: Option<&str>,
    ) -> DataResult<Person> {
        repo::find_or_create(&self.pool, name, person_type, image_url, provider_person_id)
            .await
            .data()
    }

    async fn link_to_item(
        &self,
        item_id: uuid::Uuid,
        person_id: uuid::Uuid,
        role: Option<&str>,
        sort_order: Option<i32>,
    ) -> DataResult<()> {
        repo::link_to_item(&self.pool, item_id, person_id, role, sort_order)
            .await
            .data()
    }

    async fn delete_links_by_item(&self, item_id: uuid::Uuid) -> DataResult<()> {
        repo::delete_links_by_item(&self.pool, item_id).await.data()
    }

    async fn list_people(
        &self,
        person_types: &[String],
        exclude_person_types: &[String],
        search_term: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> DataResult<Vec<Person>> {
        repo::list_people(
            &self.pool,
            person_types,
            exclude_person_types,
            search_term,
            limit,
            offset,
        )
        .await
        .data()
    }

    async fn list_people_by_ids(
        &self,
        ids: &[uuid::Uuid],
        person_types: &[String],
        exclude_person_types: &[String],
        limit: i64,
        offset: i64,
    ) -> DataResult<Vec<Person>> {
        repo::list_people_by_ids(
            &self.pool,
            ids,
            person_types,
            exclude_person_types,
            limit,
            offset,
        )
        .await
        .data()
    }

    async fn count_people_by_ids(
        &self,
        ids: &[uuid::Uuid],
        person_types: &[String],
        exclude_person_types: &[String],
    ) -> DataResult<i64> {
        repo::count_people_by_ids(&self.pool, ids, person_types, exclude_person_types)
            .await
            .data()
    }

    async fn count_people(
        &self,
        person_types: &[String],
        exclude_person_types: &[String],
        search_term: Option<&str>,
    ) -> DataResult<i64> {
        repo::count_people(&self.pool, person_types, exclude_person_types, search_term)
            .await
            .data()
    }

    async fn list_missing_headshots(&self) -> DataResult<Vec<Person>> {
        repo::list_missing_headshots(&self.pool).await.data()
    }

    async fn count_items_by_person(&self, person_id: uuid::Uuid) -> DataResult<Vec<(String, i64)>> {
        repo::count_items_by_person(&self.pool, person_id)
            .await
            .data()
    }

    async fn list_by_items(&self, item_ids: &[uuid::Uuid]) -> DataResult<Vec<ItemCredit>> {
        repo::list_by_items(&self.pool, item_ids).await.data()
    }

    async fn list_by_item(&self, item_id: uuid::Uuid) -> DataResult<Vec<Credit>> {
        repo::list_by_item(&self.pool, item_id).await.data()
    }
}
