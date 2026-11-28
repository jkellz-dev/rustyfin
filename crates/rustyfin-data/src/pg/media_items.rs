use rustyfin_models::data::*;

use super::{IntoData, PgMediaItems};
use crate::repo::media_items as repo;

impl MediaItemStore for PgMediaItems {
    async fn set_has_lyrics(&self, id: uuid::Uuid, has: bool) -> DataResult<()> {
        repo::set_has_lyrics(&self.pool, id, has).await.data()
    }

    async fn set_extra(
        &self,
        id: uuid::Uuid,
        owner: Option<uuid::Uuid>,
        extra_type: Option<&str>,
    ) -> DataResult<()> {
        repo::set_extra(&self.pool, id, owner, extra_type)
            .await
            .data()
    }

    async fn list_extras(
        &self,
        owner: uuid::Uuid,
        extra_types: &[&str],
    ) -> DataResult<Vec<MediaItem>> {
        repo::list_extras(&self.pool, owner, extra_types)
            .await
            .data()
    }

    async fn list_versions(&self, owners: &[uuid::Uuid]) -> DataResult<Vec<MediaItem>> {
        repo::list_versions(&self.pool, owners).await.data()
    }

    async fn count_extras(
        &self,
        owners: &[uuid::Uuid],
    ) -> DataResult<std::collections::HashMap<uuid::Uuid, (i64, i64)>> {
        repo::count_extras(&self.pool, owners).await.data()
    }

    async fn find_by_id(&self, id: uuid::Uuid) -> DataResult<Option<MediaItem>> {
        repo::find_by_id(&self.pool, id).await.data()
    }

    async fn list_ancestors(&self, id: uuid::Uuid) -> DataResult<Vec<MediaItem>> {
        repo::list_ancestors(&self.pool, id).await.data()
    }

    async fn existing_paths(&self, paths: &[String]) -> DataResult<Vec<String>> {
        repo::existing_paths(&self.pool, paths).await.data()
    }

    async fn list_by_parents(
        &self,
        parent_ids: &[uuid::Uuid],
        item_types: &[String],
        limit_per_parent: usize,
    ) -> DataResult<Vec<MediaItem>> {
        repo::list_by_parents(&self.pool, parent_ids, item_types, limit_per_parent)
            .await
            .data()
    }

    async fn list_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        item_types: &[String],
        start_index: i64,
        limit: i64,
    ) -> DataResult<Vec<MediaItem>> {
        repo::list_by_parent(&self.pool, parent_id, item_types, start_index, limit)
            .await
            .data()
    }

    async fn cumulative_runtime_ticks(&self, id: uuid::Uuid) -> DataResult<Option<i64>> {
        repo::cumulative_runtime_ticks(&self.pool, id).await.data()
    }

    async fn count_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        item_types: &[String],
    ) -> DataResult<i64> {
        repo::count_by_parent(&self.pool, parent_id, item_types)
            .await
            .data()
    }

    async fn list_by_library(
        &self,
        library_id: uuid::Uuid,
        start_index: i64,
        limit: i64,
    ) -> DataResult<Vec<MediaItem>> {
        repo::list_by_library(&self.pool, library_id, start_index, limit)
            .await
            .data()
    }

    async fn list_top_level_by_library(
        &self,
        library_id: uuid::Uuid,
        item_types: &[String],
        start_index: i64,
        limit: i64,
    ) -> DataResult<Vec<MediaItem>> {
        repo::list_top_level_by_library(&self.pool, library_id, item_types, start_index, limit)
            .await
            .data()
    }

    async fn count_top_level_by_library(
        &self,
        library_id: uuid::Uuid,
        item_types: &[String],
    ) -> DataResult<i64> {
        repo::count_top_level_by_library(&self.pool, library_id, item_types)
            .await
            .data()
    }

    async fn latest_by_library(
        &self,
        library_id: Option<uuid::Uuid>,
        item_types: &[String],
        limit: i64,
    ) -> DataResult<Vec<MediaItem>> {
        repo::latest_by_library(&self.pool, library_id, item_types, limit)
            .await
            .data()
    }

    async fn set_etag(&self, id: uuid::Uuid, etag: Option<&str>) -> DataResult<()> {
        repo::set_etag(&self.pool, id, etag).await.data()
    }

    async fn find_by_path(&self, path: &str) -> DataResult<Option<MediaItem>> {
        repo::find_by_path(&self.pool, path).await.data()
    }

    async fn find_folder_by_path(
        &self,
        path: &str,
        item_type: &str,
        parent_id: Option<uuid::Uuid>,
    ) -> DataResult<Option<MediaItem>> {
        repo::find_folder_by_path(&self.pool, path, item_type, parent_id)
            .await
            .data()
    }

    async fn find_folder_by_name(
        &self,
        library_id: uuid::Uuid,
        name: &str,
        item_type: &str,
        parent_id: Option<uuid::Uuid>,
    ) -> DataResult<Option<MediaItem>> {
        repo::find_folder_by_name(&self.pool, library_id, name, item_type, parent_id)
            .await
            .data()
    }

    async fn update_folder_identity(
        &self,
        id: uuid::Uuid,
        name: &str,
        sort_name: &str,
        year: Option<i32>,
        path: &str,
    ) -> DataResult<()> {
        repo::update_folder_identity(&self.pool, id, name, sort_name, year, path)
            .await
            .data()
    }

    async fn find_by_name_and_type(
        &self,
        name: &str,
        item_type: &str,
    ) -> DataResult<Option<MediaItem>> {
        repo::find_by_name_and_type(&self.pool, name, item_type)
            .await
            .data()
    }

    async fn find_by_name_and_parent(
        &self,
        name: &str,
        item_type: &str,
        parent_id: Option<uuid::Uuid>,
    ) -> DataResult<Option<MediaItem>> {
        repo::find_by_name_and_parent(&self.pool, name, item_type, parent_id)
            .await
            .data()
    }

    async fn insert(&self, item: &NewMediaItem) -> DataResult<MediaItem> {
        repo::insert(&self.pool, item).await.data()
    }

    async fn update(&self, id: uuid::Uuid, item: &NewMediaItem) -> DataResult<MediaItem> {
        repo::update(&self.pool, id, item).await.data()
    }

    async fn list_paths_in_library(
        &self,
        library_id: uuid::Uuid,
    ) -> DataResult<Vec<(uuid::Uuid, String, bool)>> {
        repo::list_paths_in_library(&self.pool, library_id)
            .await
            .data()
    }

    async fn list_film_paths(
        &self,
        library_id: uuid::Uuid,
    ) -> DataResult<Vec<(uuid::Uuid, String)>> {
        repo::list_film_paths(&self.pool, library_id).await.data()
    }

    async fn delete_ids(&self, ids: &[uuid::Uuid]) -> DataResult<u64> {
        repo::delete_ids(&self.pool, ids).await.data()
    }

    async fn delete_empty_pathless_folders(&self, library_id: uuid::Uuid) -> DataResult<u64> {
        repo::delete_empty_pathless_folders(&self.pool, library_id)
            .await
            .data()
    }

    async fn delete_by_path(&self, path: &str) -> DataResult<bool> {
        repo::delete_by_path(&self.pool, path).await.data()
    }

    async fn delete_under_path(&self, path: &str) -> DataResult<u64> {
        repo::delete_under_path(&self.pool, path).await.data()
    }

    async fn paths_by_external_id(
        &self,
        item_type: &str,
        key: &str,
        value: &str,
    ) -> DataResult<Vec<String>> {
        repo::paths_by_external_id(&self.pool, item_type, key, value)
            .await
            .data()
    }

    async fn delete_by_library(&self, library_id: uuid::Uuid) -> DataResult<u64> {
        repo::delete_by_library(&self.pool, library_id).await.data()
    }

    async fn count_children_batch(
        &self,
        parent_ids: &[uuid::Uuid],
    ) -> DataResult<std::collections::HashMap<uuid::Uuid, i64>> {
        repo::count_children_batch(&self.pool, parent_ids)
            .await
            .data()
    }

    async fn update_metadata(&self, id: uuid::Uuid, meta: &ItemMetadata) -> DataResult<()> {
        repo::update_metadata(&self.pool, id, meta).await.data()
    }

    async fn count_by_type(
        &self,
        libraries: Option<&[uuid::Uuid]>,
    ) -> DataResult<Vec<(String, i64)>> {
        repo::count_by_type(&self.pool, libraries).await.data()
    }

    async fn count_all(&self) -> DataResult<i64> {
        repo::count_all(&self.pool).await.data()
    }

    async fn distinct_genres(&self, parent_id: Option<uuid::Uuid>) -> DataResult<Vec<String>> {
        repo::distinct_genres(&self.pool, parent_id).await.data()
    }

    async fn distinct_years(&self, parent_id: Option<uuid::Uuid>) -> DataResult<Vec<i32>> {
        repo::distinct_years(&self.pool, parent_id).await.data()
    }

    async fn count_items_by_years(
        &self,
        years: &[i32],
        parent_id: Option<uuid::Uuid>,
    ) -> DataResult<std::collections::HashMap<i32, Vec<(String, i64)>>> {
        repo::count_items_by_years(&self.pool, years, parent_id)
            .await
            .data()
    }

    async fn count_items_by_year(
        &self,
        year: i32,
        parent_id: Option<uuid::Uuid>,
    ) -> DataResult<Vec<(String, i64)>> {
        repo::count_items_by_year(&self.pool, year, parent_id)
            .await
            .data()
    }

    async fn distinct_official_ratings(
        &self,
        parent_id: Option<uuid::Uuid>,
    ) -> DataResult<Vec<String>> {
        repo::distinct_official_ratings(&self.pool, parent_id)
            .await
            .data()
    }

    async fn distinct_tags(&self, parent_id: Option<uuid::Uuid>) -> DataResult<Vec<String>> {
        repo::distinct_tags(&self.pool, parent_id).await.data()
    }

    async fn get_active_series(
        &self,
        user_id: uuid::Uuid,
        library_id: Option<uuid::Uuid>,
        limit: i64,
    ) -> DataResult<Vec<(uuid::Uuid, chrono::DateTime<chrono::Utc>)>> {
        repo::get_active_series(&self.pool, user_id, library_id, limit)
            .await
            .data()
    }

    async fn series_never_started(
        &self,
        user_id: uuid::Uuid,
        library_id: Option<uuid::Uuid>,
        limit: i64,
    ) -> DataResult<Vec<uuid::Uuid>> {
        repo::series_never_started(&self.pool, user_id, library_id, limit)
            .await
            .data()
    }

    async fn visible_ids(&self, ids: &[uuid::Uuid]) -> DataResult<Vec<uuid::Uuid>> {
        repo::visible_ids(&self.pool, ids).await.data()
    }

    async fn find_by_ids(&self, ids: &[uuid::Uuid]) -> DataResult<Vec<MediaItem>> {
        repo::find_by_ids(&self.pool, ids).await.data()
    }

    async fn add_external_ids(&self, id: uuid::Uuid, ids: &serde_json::Value) -> DataResult<()> {
        repo::add_external_ids(&self.pool, id, ids).await.data()
    }

    async fn album_of(&self, folder_id: uuid::Uuid) -> DataResult<Option<uuid::Uuid>> {
        repo::album_of(&self.pool, folder_id).await.data()
    }

    async fn set_artists(&self, id: uuid::Uuid, artists: &[String]) -> DataResult<()> {
        repo::set_artists(&self.pool, id, artists).await.data()
    }

    async fn runtime_totals(
        &self,
        folder_ids: &[uuid::Uuid],
    ) -> DataResult<std::collections::HashMap<uuid::Uuid, i64>> {
        repo::runtime_totals(&self.pool, folder_ids).await.data()
    }

    async fn find_artists_by_names(
        &self,
        names: &[String],
    ) -> DataResult<Vec<(uuid::Uuid, String, Option<uuid::Uuid>)>> {
        repo::find_artists_by_names(&self.pool, names).await.data()
    }

    async fn get_last_watched_episode(
        &self,
        user_id: uuid::Uuid,
        series_id: uuid::Uuid,
    ) -> DataResult<Option<MediaItem>> {
        repo::get_last_watched_episode(&self.pool, user_id, series_id)
            .await
            .data()
    }

    async fn get_next_episode(
        &self,
        user_id: uuid::Uuid,
        series_id: uuid::Uuid,
        min_season: Option<i32>,
        min_episode: Option<i32>,
    ) -> DataResult<Option<MediaItem>> {
        repo::get_next_episode(&self.pool, user_id, series_id, min_season, min_episode)
            .await
            .data()
    }

    async fn get_next_episodes(
        &self,
        user_id: uuid::Uuid,
        series_ids: &[uuid::Uuid],
    ) -> DataResult<Vec<MediaItem>> {
        repo::get_next_episodes(&self.pool, user_id, series_ids)
            .await
            .data()
    }

    async fn get_random_items(
        &self,
        user_id: uuid::Uuid,
        item_types: &[String],
        library_ids: &[uuid::Uuid],
        limit: i64,
        offset: i64,
    ) -> DataResult<Vec<MediaItem>> {
        repo::get_random_items(&self.pool, user_id, item_types, library_ids, limit, offset)
            .await
            .data()
    }

    async fn search_by_name(&self, query: &str, limit: i64) -> DataResult<Vec<MediaItem>> {
        repo::search_by_name(&self.pool, query, limit).await.data()
    }

    async fn search_index_page(
        &self,
        after: Option<uuid::Uuid>,
        limit: i64,
    ) -> DataResult<Vec<MediaItem>> {
        repo::search_index_page(&self.pool, after, limit)
            .await
            .data()
    }

    async fn mix_seed_genres(&self, id: uuid::Uuid) -> DataResult<Vec<String>> {
        repo::mix_seed_genres(&self.pool, id).await.data()
    }

    async fn instant_mix(
        &self,
        genres: &[String],
        libraries: Option<&[uuid::Uuid]>,
        limit: i64,
    ) -> DataResult<Vec<MediaItem>> {
        repo::instant_mix(&self.pool, genres, libraries, limit)
            .await
            .data()
    }

    async fn set_genres(&self, id: uuid::Uuid, genres: &[String]) -> DataResult<()> {
        repo::set_genres(&self.pool, id, genres).await.data()
    }

    async fn first_added_with(
        &self,
        kind: NameKind<'_>,
    ) -> DataResult<Option<chrono::DateTime<chrono::Utc>>> {
        repo::first_added_with(&self.pool, kind).await.data()
    }

    async fn artist_counts(&self, artist: uuid::Uuid) -> DataResult<(i64, i64)> {
        repo::artist_counts(&self.pool, artist).await.data()
    }

    async fn find_similar(
        &self,
        item_id: uuid::Uuid,
        item_type: &str,
        limit: i64,
    ) -> DataResult<Vec<MediaItem>> {
        repo::find_similar(&self.pool, item_id, item_type, limit)
            .await
            .data()
    }

    async fn count_items_with_genre(
        &self,
        genre: &str,
        parent_id: Option<uuid::Uuid>,
    ) -> DataResult<Vec<(String, i64)>> {
        repo::count_items_with_genre(&self.pool, genre, parent_id)
            .await
            .data()
    }

    async fn distinct_genres_by_item_types(
        &self,
        parent_id: Option<uuid::Uuid>,
        item_types: &[&str],
    ) -> DataResult<Vec<String>> {
        repo::distinct_genres_by_item_types(&self.pool, parent_id, item_types)
            .await
            .data()
    }

    async fn count_items_with_genre_by_types(
        &self,
        genre: &str,
        parent_id: Option<uuid::Uuid>,
        item_types: &[&str],
    ) -> DataResult<Vec<(String, i64)>> {
        repo::count_items_with_genre_by_types(&self.pool, genre, parent_id, item_types)
            .await
            .data()
    }

    async fn count_genres_by_types(
        &self,
        genres: &[String],
        parent_id: Option<uuid::Uuid>,
        item_types: &[&str],
    ) -> DataResult<std::collections::HashMap<String, Vec<(String, i64)>>> {
        repo::count_genres_by_types(&self.pool, genres, parent_id, item_types)
            .await
            .data()
    }

    async fn genre_albums(
        &self,
        genres: &[String],
        library_id: Option<uuid::Uuid>,
    ) -> DataResult<std::collections::HashMap<String, uuid::Uuid>> {
        repo::genre_albums(&self.pool, genres, library_id)
            .await
            .data()
    }

    async fn distinct_studios(&self, parent_id: Option<uuid::Uuid>) -> DataResult<Vec<String>> {
        repo::distinct_studios(&self.pool, parent_id).await.data()
    }

    async fn count_items_with_studio(
        &self,
        studio: &str,
        parent_id: Option<uuid::Uuid>,
    ) -> DataResult<Vec<(String, i64)>> {
        repo::count_items_with_studio(&self.pool, studio, parent_id)
            .await
            .data()
    }

    async fn get_active_series_or_movies(
        &self,
        user_id: uuid::Uuid,
        item_type: &str,
        limit: i64,
    ) -> DataResult<Vec<uuid::Uuid>> {
        repo::get_active_series_or_movies(&self.pool, user_id, item_type, limit)
            .await
            .data()
    }

    async fn list_artists(
        &self,
        filter: &ArtistFilter<'_>,
        start_index: i64,
        limit: i64,
    ) -> DataResult<(Vec<MediaItem>, i64)> {
        repo::list_artists(&self.pool, filter, start_index, limit)
            .await
            .data()
    }

    async fn count_albums_by_artist(&self, artist_id: uuid::Uuid) -> DataResult<i32> {
        repo::count_albums_by_artist(&self.pool, artist_id)
            .await
            .data()
    }

    async fn count_albums_and_songs_by_artists(
        &self,
        artist_ids: &[uuid::Uuid],
    ) -> DataResult<std::collections::HashMap<uuid::Uuid, (i32, i32)>> {
        repo::count_albums_and_songs_by_artists(&self.pool, artist_ids)
            .await
            .data()
    }

    async fn count_songs_by_artist(&self, artist_id: uuid::Uuid) -> DataResult<i32> {
        repo::count_songs_by_artist(&self.pool, artist_id)
            .await
            .data()
    }

    async fn upcoming_episodes(
        &self,
        parent_id: Option<uuid::Uuid>,
        limit: i64,
        offset: i64,
    ) -> DataResult<Vec<MediaItem>> {
        repo::upcoming_episodes(&self.pool, parent_id, limit, offset)
            .await
            .data()
    }

    async fn count_upcoming_episodes(&self, parent_id: Option<uuid::Uuid>) -> DataResult<i64> {
        repo::count_upcoming_episodes(&self.pool, parent_id)
            .await
            .data()
    }

    async fn genre_image(
        &self,
        genre: &str,
        library: Option<uuid::Uuid>,
    ) -> DataResult<Option<Image>> {
        repo::genre_image(&self.pool, genre, library).await.data()
    }

    async fn album_genres_from_tracks(&self, library: uuid::Uuid) -> DataResult<u64> {
        repo::album_genres_from_tracks(&self.pool, library)
            .await
            .data()
    }

    async fn genre_images(
        &self,
        genres: &[String],
        library: Option<uuid::Uuid>,
    ) -> DataResult<Vec<(String, Image)>> {
        repo::genre_images(&self.pool, genres, library).await.data()
    }

    async fn count_items_with_genres(
        &self,
        genres: &[String],
        parent_id: Option<uuid::Uuid>,
    ) -> DataResult<Vec<(String, String, i64)>> {
        repo::count_items_with_genres(&self.pool, genres, parent_id)
            .await
            .data()
    }

    async fn added_since(
        &self,
        library_id: uuid::Uuid,
        since: chrono::DateTime<chrono::Utc>,
    ) -> DataResult<Vec<uuid::Uuid>> {
        repo::added_since(&self.pool, library_id, since)
            .await
            .data()
    }

    async fn query(&self, query: &ItemQuery) -> DataResult<Vec<MediaItem>> {
        use crate::repo::media_items::ItemQuerySql;
        query.fetch(&self.pool).await.data()
    }

    async fn count_query(&self, query: &ItemQuery) -> DataResult<i64> {
        use crate::repo::media_items::ItemQuerySql;
        query.count(&self.pool).await.data()
    }
}
