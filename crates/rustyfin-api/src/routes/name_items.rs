//! Items that stand for a name: a person, genre, music genre, studio or year.
//! Jellyfin sends them with the fields of any item (user data, image fields,
//! counts, dates); clients read those off them without checking.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rustyfin_models::dto::BaseItemDto;
use uuid::Uuid;

use super::items::attach_user_data;

/// The fields every name item in a list carries; `user_data` for the lists
/// Jellyfin sends it on (people, studios, years).
pub(crate) async fn finish_list(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    dtos: &mut [BaseItemDto],
    user_data: bool,
) {
    for dto in dtos.iter_mut() {
        list_defaults(dto);
    }
    if user_data {
        attach_user_data(pool, user_id, dtos).await;
    }
}

/// A single name item, as `/Persons/{name}` and the like answer it.
pub(crate) async fn finish_detail(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    mut dto: BaseItemDto,
    date_created: Option<DateTime<Utc>>,
) -> BaseItemDto {
    list_defaults(&mut dto);
    let id = dto.id.simple().to_string();
    let sort_name = dto.name.to_lowercase();
    dto.etag.get_or_insert_with(|| id.clone());
    dto.date_created = dto
        .date_created
        .or(date_created)
        .or(Some(DateTime::<Utc>::UNIX_EPOCH));
    dto.sort_name.get_or_insert(sort_name);
    dto.can_delete.get_or_insert(false);
    dto.can_download.get_or_insert(false);
    dto.external_urls.get_or_insert_with(Vec::new);
    dto.taglines.get_or_insert_with(Vec::new);
    dto.genres.get_or_insert_with(Vec::new);
    dto.remote_trailers.get_or_insert_with(Vec::new);
    dto.provider_ids.get_or_insert_with(HashMap::new);
    dto.people.get_or_insert_with(Vec::new);
    dto.studios.get_or_insert_with(Vec::new);
    dto.genre_items.get_or_insert_with(Vec::new);
    dto.tags.get_or_insert_with(Vec::new);
    dto.locked_fields.get_or_insert_with(Vec::new);
    dto.enable_media_source_display.get_or_insert(true);
    dto.play_access.get_or_insert_with(|| "Full".to_string());
    dto.lock_data.get_or_insert(false);
    dto.display_preferences_id.get_or_insert(id);
    for count in [
        &mut dto.child_count,
        &mut dto.local_trailer_count,
        &mut dto.special_feature_count,
        &mut dto.trailer_count,
        &mut dto.movie_count,
        &mut dto.series_count,
        &mut dto.program_count,
        &mut dto.episode_count,
        &mut dto.song_count,
        &mut dto.album_count,
        &mut dto.artist_count,
        &mut dto.music_video_count,
    ] {
        count.get_or_insert(0);
    }
    attach_user_data(pool, user_id, std::slice::from_mut(&mut dto)).await;
    dto
}

fn list_defaults(dto: &mut BaseItemDto) {
    dto.image_tags.get_or_insert_with(HashMap::new);
    dto.backdrop_image_tags.get_or_insert_with(Vec::new);
    dto.image_blur_hashes.get_or_insert_with(HashMap::new);
    dto.location_type
        .get_or_insert_with(|| "FileSystem".to_string());
    dto.media_type.get_or_insert_with(|| "Unknown".to_string());
}
