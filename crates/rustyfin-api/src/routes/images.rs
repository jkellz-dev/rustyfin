use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use uuid::Uuid;

use rustyfin_auth::middleware::{AuthenticatedUser, OptionalUser};
use rustyfin_data::repo::{images, libraries, media_items, people, user_library_access};
use rustyfin_media::image_processing::{self, ImageFormat, ImageOptions, Placeholder};
use rustyfin_models::dto::ImageInfo;

use crate::error::ApiError;
use crate::router::AppState;

/// Query parameters for image requests (Jellyfin-compatible; keys are normalized to PascalCase by `query_case`).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImageParams {
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub quality: Option<u8>,
    pub format: Option<String>,
    pub tag: Option<String>,
}

/// `GET /Items/{itemId}/Images/{imageType}` -- Serve item image.
pub async fn get_item_image(
    State(state): State<AppState>,
    user: OptionalUser,
    Path((item_id, image_type)): Path<(Uuid, String)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    check_image_access(&state, &user, item_id).await?;
    let opts = build_options(&params, &headers);
    let req = ImageRequest::new(&params, &headers);
    match images::find_by_item_and_type(&state.pool, item_id, &image_type, 0)
        .await
        .map_err(ApiError::Database)?
    {
        // Missing or unreadable artwork falls back as if there were none.
        Some(image) => match serve_image(&image.path, &opts, &state, &req).await {
            Err(ApiError::NotFound) => {
                serve_placeholder(&state, item_id, &image_type, &opts, &req).await
            }
            served => served,
        },
        None => serve_placeholder(&state, item_id, &image_type, &opts, &req).await,
    }
}

/// When the request names a user, images of items and libraries they may not see
/// answer 404, as every other item route does. Requests without a user are served
/// as before: clients load images through plain `<img>` tags that carry no token,
/// and item ids are random.
async fn check_image_access(
    state: &AppState,
    user: &OptionalUser,
    item_id: Uuid,
) -> Result<(), ApiError> {
    let Some(user) = &user.0 else {
        return Ok(());
    };
    if media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .is_some()
    {
        return super::access::require_item_access(state, user.user_id, item_id).await;
    }
    // Otherwise it may be a library (a collection folder's image).
    let accessible = user_library_access::get_accessible_library_ids(&state.pool, user.user_id)
        .await
        .map_err(ApiError::Database)?;
    match accessible {
        // A genre's picture is anyone's (its albums are checked when opened).
        Some(allowed)
            if !allowed.contains(&item_id)
                && super::music_genres::genre_name_by_id(&state.pool, item_id)
                    .await
                    .is_none() =>
        {
            Err(ApiError::NotFound)
        }
        _ => Ok(()),
    }
}

/// A person's headshot in the shared people folder, if the metadata CLI has
/// fetched one: by TMDB id, else by the photo's address (never by name).
pub(crate) fn headshot_path(
    state: &AppState,
    person: &people::PersonRow,
) -> Option<std::path::PathBuf> {
    let dir = state.people_cache_dir.as_ref()?;
    let path = dir.join(rustyfin_core::headshot_file_name(
        person.provider_person_id.as_deref(),
        person.image_url.as_deref(),
    )?);
    path.is_file().then_some(path)
}

/// The image tag of a headshot file: changes when the file is replaced.
pub(crate) fn headshot_tag(path: &std::path::Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(format!("hs-{modified:x}-{:x}", meta.len()))
}

/// The placeholder for an item or library with no primary artwork. Other image
/// types have no placeholder: clients only require a primary image.
async fn serve_placeholder(
    state: &AppState,
    item_id: Uuid,
    image_type: &str,
    opts: &ImageOptions,
    req: &ImageRequest<'_>,
) -> Result<Response, ApiError> {
    if !image_type.eq_ignore_ascii_case("Primary") {
        return Err(ApiError::NotFound);
    }
    let shape = if let Some(item) = media_items::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
    {
        // An episode without a thumb: a frame from the video, grabbed once into
        // the image cache. If that fails (unreadable file, no ffmpeg), the
        // landscape placeholder below stands in.
        if item.item_type == "Episode"
            && let Some(video) = item.path.as_deref()
        {
            let at = item
                .runtime_ticks
                .map_or(60.0, |t| (t as f64 / 10_000_000.0 * 0.1).clamp(1.0, 600.0));
            let key = super::items::frame_tag(item.id, item.etag.as_deref());
            match image_processing::video_frame(
                &state.transcode_manager.ffmpeg_path,
                std::path::Path::new(video),
                &state.image_cache_dir,
                &key,
                at,
            )
            .await
            {
                Ok(frame) => return serve_image(&frame.to_string_lossy(), opts, state, req).await,
                Err(e) => tracing::debug!("No frame for episode {}: {e}", item.id),
            }
        }
        // Music without a cover file: the cover embedded in the track, or in
        // an album's first track that has one (as Jellyfin). The placeholder
        // stands in when none has a picture.
        if matches!(item.item_type.as_str(), "MusicAlbum" | "Audio") {
            let tracks = if item.item_type == "Audio" {
                vec![item.clone()]
            } else {
                media_items::list_by_parents(&state.pool, &[item.id], &["Audio".to_string()], 5)
                    .await
                    .map_err(ApiError::Database)?
            };
            let key = super::items::cover_tag(item.id, item.etag.as_deref());
            for track in &tracks {
                let Some(path) = track.path.as_deref() else {
                    continue;
                };
                if let Some(cover) = image_processing::embedded_cover(
                    std::path::Path::new(path),
                    &state.image_cache_dir,
                    &key,
                )
                .await
                {
                    return serve_image(&cover.to_string_lossy(), opts, state, req).await;
                }
            }
        }
        // A collection without a cover shows its earliest film's poster.
        if item.item_type == "BoxSet" {
            let members = rustyfin_data::repo::collections::list_item_ids(&state.pool, item.id)
                .await
                .map_err(ApiError::Database)?;
            let mut films = media_items::find_by_ids(&state.pool, &members)
                .await
                .map_err(ApiError::Database)?;
            films.sort_by_key(|f| (f.production_year.unwrap_or(i32::MAX), f.name.clone()));
            let posters = images::list_by_items(&state.pool, &members)
                .await
                .map_err(ApiError::Database)?;
            let poster = films.iter().find_map(|film| {
                posters.iter().find(|i| {
                    i.entity_id == Some(film.id) && i.image_type == "Primary" && i.image_index == 0
                })
            });
            if let Some(poster) = poster {
                return serve_image(&poster.path, opts, state, req).await;
            }
        }
        // A season without a cover shows its show's.
        if item.item_type == "Season"
            && let Some(series) = item.series_id.or(item.parent_id)
            && let Some(poster) = images::find_by_item_and_type(&state.pool, series, "Primary", 0)
                .await
                .map_err(ApiError::Database)?
        {
            return serve_image(&poster.path, opts, state, req).await;
        }
        Placeholder::for_item_type(&item.item_type)
    } else if libraries::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
        .is_some()
    {
        Placeholder::Landscape
    } else if let Some(person) = people::find_by_id(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?
    {
        // People: the headshot from the shared folder, or nothing (clients
        // draw initials).
        let path = headshot_path(state, &person).ok_or(ApiError::NotFound)?;
        return serve_image(&path.to_string_lossy(), opts, state, req).await;
    } else if let Some(genre) = super::music_genres::genre_name_by_id(&state.pool, item_id).await {
        // A genre: one of its albums' covers, else a title's art (films).
        let album = media_items::genre_albums(&state.pool, std::slice::from_ref(&genre), None)
            .await
            .map_err(ApiError::Database)?
            .remove(&genre);
        if let Some(album) = album {
            if let Some(cover) = images::find_by_item_and_type(&state.pool, album, "Primary", 0)
                .await
                .map_err(ApiError::Database)?
            {
                return serve_image(&cover.path, opts, state, req).await;
            }
            return Box::pin(serve_placeholder(state, album, image_type, opts, req)).await;
        }
        let image = media_items::genre_image(&state.pool, &genre, None)
            .await
            .map_err(ApiError::Database)?
            .ok_or(ApiError::NotFound)?;
        return serve_image(&image.path, opts, state, req).await;
    } else {
        return Err(ApiError::NotFound);
    };
    let path = shape.ensure_file(&state.image_cache_dir).map_err(|e| {
        tracing::error!("Could not draw the {shape:?} placeholder: {e}");
        ApiError::Internal(format!("placeholder: {e}"))
    })?;
    serve_image(&path.to_string_lossy(), opts, state, req).await
}
/// `GET /Items/{itemId}/Images/{imageType}/{imageIndex}` -- Serve item image by index.
/// The image path's segments after the index: tag, format, max width, max
/// height, percent played, unplayed count.
type ImagePathTail = (Uuid, String, i32, String, String, u32, u32, String, String);

/// `GET /Items/{itemId}/Images/{imageType}/{imageIndex}/{tag}/{format}/{maxWidth}/{maxHeight}/{percentPlayed}/{unplayedCount}`:
/// Jellyfin's older form, the query's options as path segments. The played
/// and unplayed overlays are not drawn (as in Jellyfin, which ignores them).
pub async fn get_item_image_by_path(
    state: State<AppState>,
    user: OptionalUser,
    Path((item_id, image_type, image_index, tag, format, max_width, max_height, _, _)): Path<
        ImagePathTail,
    >,
    Query(mut params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    params.tag.get_or_insert(tag);
    params.format.get_or_insert(format);
    params.max_width.get_or_insert(max_width);
    params.max_height.get_or_insert(max_height);
    get_item_image_by_index(
        state,
        user,
        Path((item_id, image_type, image_index)),
        Query(params),
        headers,
    )
    .await
}

pub async fn get_item_image_by_index(
    State(state): State<AppState>,
    user: OptionalUser,
    Path((item_id, image_type, image_index)): Path<(Uuid, String, i32)>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    check_image_access(&state, &user, item_id).await?;
    let opts = build_options(&params, &headers);
    let req = ImageRequest::new(&params, &headers);
    match images::find_by_item_and_type(&state.pool, item_id, &image_type, image_index)
        .await
        .map_err(ApiError::Database)?
    {
        Some(image) => serve_image(&image.path, &opts, &state, &req).await,
        None if image_index == 0 => {
            serve_placeholder(&state, item_id, &image_type, &opts, &req).await
        }
        None => Err(ApiError::NotFound),
    }
}
/// `GET /Items/{itemId}/Images` -- List all images for an item.
pub async fn get_item_images_list(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
) -> Result<Json<Vec<ImageInfo>>, ApiError> {
    super::access::require_item_access(&state, auth.user_id, item_id).await?;
    let item_images = images::list_by_item(&state.pool, item_id)
        .await
        .map_err(ApiError::Database)?;

    let mut infos = Vec::with_capacity(item_images.len());
    for img in &item_images {
        let size = tokio::fs::metadata(&img.path)
            .await
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        let tag = img
            .blurhash
            .as_deref()
            .map(|bh| bh.chars().take(8).collect::<String>())
            .unwrap_or_else(|| {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                img.path.hash(&mut hasher);
                format!("{:x}", hasher.finish())
            });
        infos.push(ImageInfo {
            image_type: img.image_type.clone(),
            image_index: img.image_index,
            image_tag: tag,
            blur_hash: img.blurhash.clone(),
            height: img.height,
            width: img.width,
            size,
        });
    }

    Ok(Json(infos))
}

/// Serve an image for any entity by UUID lookup.
pub(crate) async fn serve_entity_image(
    state: &AppState,
    entity_id: Uuid,
    image_type: &str,
    image_index: i32,
    params: &ImageParams,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let image = images::find_by_item_and_type(&state.pool, entity_id, image_type, image_index)
        .await
        .map_err(ApiError::Database)?
        .ok_or(ApiError::NotFound)?;

    let opts = build_options(params, headers);
    let req = ImageRequest::new(params, headers);
    serve_image(&image.path, &opts, state, &req).await
}

/// Build `ImageOptions` from query parameters and request headers.
pub(crate) fn build_options(params: &ImageParams, _headers: &HeaderMap) -> ImageOptions {
    let format = match params.format.as_deref() {
        Some("webp" | "Webp" | "WEBP") => ImageFormat::Webp,
        Some("png" | "Png" | "PNG") => ImageFormat::Png,
        Some("jpg" | "jpeg" | "Jpg" | "Jpeg" | "JPEG") => ImageFormat::Jpeg,
        // Not from `Accept: image/webp`, which every browser sends: the WebP
        // encoder is lossless only, so that re-encoded every image, often into
        // a larger file than the JPEG it came from.
        _ => ImageFormat::Original,
    };

    ImageOptions {
        max_width: params.max_width,
        max_height: params.max_height,
        width: params.width,
        height: params.height,
        quality: params.quality.unwrap_or(90),
        format,
    }
}

/// What a request says about caching: the image tag it names, and the ETag it
/// already holds.
pub(crate) struct ImageRequest<'a> {
    tag: Option<&'a str>,
    if_none_match: Option<&'a str>,
}

impl<'a> ImageRequest<'a> {
    pub(crate) fn new(params: &'a ImageParams, headers: &'a HeaderMap) -> Self {
        Self {
            tag: params.tag.as_deref().filter(|t| !t.is_empty()),
            if_none_match: headers
                .get(header::IF_NONE_MATCH)
                .and_then(|v| v.to_str().ok()),
        }
    }
}

/// A URL naming the image's tag changes when the image does, so it can be kept
/// for a year; without one, an hour, then revalidated by ETag.
const CACHE_TAGGED: &str = "public, max-age=31536000, immutable";
const CACHE_UNTAGGED: &str = "public, max-age=3600";

fn etag_matches(if_none_match: Option<&str>, etag: &str) -> bool {
    let Some(header) = if_none_match else {
        return false;
    };
    let bare = etag.trim_start_matches("W/");
    header
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == "*" || candidate.trim_start_matches("W/") == bare)
}

/// Serve an image, processing (resize/format convert) if needed. A request that
/// already holds the current version gets a 304 before any resizing.
pub(crate) async fn serve_image(
    source_path: &str,
    opts: &ImageOptions,
    state: &AppState,
    req: &ImageRequest<'_>,
) -> Result<Response, ApiError> {
    if source_path.starts_with(rustyfin_core::link::LINK_SCHEME) {
        let cache_control = if req.tag.is_some() {
            CACHE_TAGGED
        } else {
            CACHE_UNTAGGED
        };
        return super::link_relay::image(
            state,
            source_path,
            opts,
            |etag| etag_matches(req.if_none_match, etag),
            cache_control,
        )
        .await;
    }
    let meta = tokio::fs::metadata(source_path)
        .await
        .map_err(|_| ApiError::NotFound)?;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis());
    let etag = image_processing::etag_for(source_path, meta.len(), mtime_ms, opts);
    let cache_control = if req.tag.is_some() {
        CACHE_TAGGED
    } else {
        CACHE_UNTAGGED
    };
    if etag_matches(req.if_none_match, &etag) {
        return Ok(Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, &etag)
            .header(header::CACHE_CONTROL, cache_control)
            .body(Body::empty())
            .unwrap());
    }

    let path = if opts.needs_processing() {
        image_processing::process_image(source_path, opts, &state.image_cache_dir)
            .await
            .map_err(|e| {
                // Artwork that is not a readable image is the library's problem,
                // not the server's: answer as if it were missing (the primary
                // image route then shows the placeholder) and say which file.
                if image_processing::is_unreadable_image(e.as_ref()) {
                    tracing::warn!("Artwork {source_path} is not a readable image: {e}");
                    ApiError::NotFound
                } else {
                    tracing::error!("Image processing failed for {source_path}: {e}");
                    ApiError::Internal(format!("Image processing failed: {e}"))
                }
            })?
    } else {
        std::path::PathBuf::from(source_path)
    };
    serve_image_file(&path, &etag, cache_control).await
}

/// Stream an image file with its type (from its bytes) and caching headers.
async fn serve_image_file(
    path: &std::path::Path,
    etag: &str,
    cache_control: &str,
) -> Result<Response, ApiError> {
    let file = image_processing::OpenImage::open(path)
        .await
        .map_err(|_| ApiError::NotFound)?;
    let content_type = file
        .content_type
        .unwrap_or_else(|| extension_content_type(&path.to_string_lossy()));
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, file.len)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, cache_control)
        .body(Body::from_stream(file.into_stream()))
        .unwrap())
}

fn extension_content_type(path: &str) -> &'static str {
    match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        _ => "image/jpeg",
    }
}
