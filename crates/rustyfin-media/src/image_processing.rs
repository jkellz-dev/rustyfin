use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct ImageOptions {
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub quality: u8,
    pub format: ImageFormat,
}

#[derive(Debug, Clone, Copy)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Webp,
    Original,
}

impl ImageOptions {
    pub fn needs_processing(&self) -> bool {
        self.max_width.is_some()
            || self.max_height.is_some()
            || self.width.is_some()
            || self.height.is_some()
            || !matches!(self.format, ImageFormat::Original)
    }
}

/// Part of every cached file's key and every ETag: bumped when what a
/// request answers with changes (v2: a transparent picture resized without a
/// named format stays a PNG instead of a JPEG on black).
const CACHE_VERSION: &str = "v2";

pub fn cache_path(cache_dir: &Path, source_path: &str, opts: &ImageOptions) -> PathBuf {
    let key = format!(
        "{}:{}:{}:{}:{}:{}:{:?}:{}",
        source_path,
        opts.max_width.unwrap_or(0),
        opts.max_height.unwrap_or(0),
        opts.width.unwrap_or(0),
        opts.height.unwrap_or(0),
        opts.quality,
        opts.format,
        CACHE_VERSION,
    );
    let hash: String = Sha256::digest(key.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let ext = match opts.format {
        ImageFormat::Webp => "webp",
        ImageFormat::Png => "png",
        _ => "jpg",
    };
    let subdir = &hash[..2];
    cache_dir.join(subdir).join(format!("{hash}.{ext}"))
}

pub async fn process_image(
    source_path: &str,
    opts: &ImageOptions,
    cache_dir: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    process_image_traced(source_path, opts, cache_dir)
        .await
        .map(|(path, _)| path)
}

/// Resizes run at most this many at once: decoding a large poster takes a CPU
/// for a moment and a few hundred MB, and a grid of fifty would ask for fifty.
fn resize_permits() -> &'static tokio::sync::Semaphore {
    static PERMITS: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
    PERMITS.get_or_init(|| {
        let cpus = std::thread::available_parallelism().map_or(2, |n| n.get());
        tokio::sync::Semaphore::new(cpus.max(2))
    })
}

type InFlight =
    std::sync::Mutex<std::collections::HashMap<PathBuf, std::sync::Arc<tokio::sync::Mutex<()>>>>;

fn in_flight_map() -> std::sync::MutexGuard<
    'static,
    std::collections::HashMap<PathBuf, std::sync::Arc<tokio::sync::Mutex<()>>>,
> {
    static MAP: std::sync::OnceLock<InFlight> = std::sync::OnceLock::new();
    MAP.get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// One lock per cached file being made, so identical requests wait for the
/// first one instead of resizing too.
fn in_flight(path: &Path) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    in_flight_map()
        .entry(path.to_path_buf())
        .or_default()
        .clone()
}

/// Forget a finished file's lock. Waiters hold their own clone of it, and once
/// the file exists nobody needs a new one, so the map stays small.
fn done_in_flight(path: &Path) {
    in_flight_map().remove(path);
}

/// `process_image`, also saying whether this call did the resize (false: the
/// file was cached, or another request made it meanwhile).
pub async fn process_image_traced(
    source_path: &str,
    opts: &ImageOptions,
    cache_dir: &Path,
) -> Result<(PathBuf, bool), Box<dyn std::error::Error + Send + Sync>> {
    let cached = cache_path(cache_dir, source_path, opts);
    if cached.exists() {
        return Ok((cached, false));
    }
    let lock = in_flight(&cached);
    let _guard = lock.lock().await;
    if cached.exists() {
        done_in_flight(&cached);
        return Ok((cached, false));
    }
    let _permit = resize_permits().acquire().await?;

    // Written under a temporary name and renamed, so a reader never sees a
    // half-written file.
    let tmp = cached.with_file_name(format!(
        ".{}.{}.tmp",
        cached
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image"),
        uuid::Uuid::new_v4().simple()
    ));
    let source = source_path.to_string();
    let opts = opts.clone();
    let tmp_clone = tmp.clone();
    let made =
        tokio::task::spawn_blocking(move || resize_and_save(&source, &opts, &tmp_clone)).await;
    let result = match made {
        Ok(Ok(())) => std::fs::rename(&tmp, &cached).map_err(Into::into),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(e.into()),
    };
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done_in_flight(&cached);
    result.map(|()| (cached, true))
}

/// A frame from `video` at `at_secs`, as a JPEG under `{cache_dir}/frames/`,
/// named by `key` (which should change when the video does). Grabbed once:
/// later and concurrent calls share the file. Shares the resize limit, since
/// decoding a frame costs about as much as resizing a poster.
pub async fn video_frame(
    ffmpeg: &str,
    video: &Path,
    cache_dir: &Path,
    key: &str,
    at_secs: f64,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let safe_key: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let dir = cache_dir.join("frames");
    let out = dir.join(format!("{safe_key}.jpg"));
    if out.exists() {
        return Ok(out);
    }
    let lock = in_flight(&out);
    let _guard = lock.lock().await;
    if out.exists() {
        done_in_flight(&out);
        return Ok(out);
    }
    let _permit = resize_permits().acquire().await?;
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!(
        ".{safe_key}.{}.tmp.jpg",
        uuid::Uuid::new_v4().simple()
    ));
    // Past the opening credits; from the start if the video is shorter.
    let mut result = grab(ffmpeg, video, &tmp, at_secs).await;
    if result.is_err() && at_secs > 0.0 {
        result = grab(ffmpeg, video, &tmp, 0.0).await;
    }
    let result = result.and_then(|()| std::fs::rename(&tmp, &out).map_err(Into::into));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done_in_flight(&out);
    result.map(|()| out)
}

/// The cover art embedded in `track` (the front cover, else the first
/// picture), written once to `{cache_dir}/covers/{key}.img` and shared by later
/// calls; `None` when the track has no picture or cannot be read. For albums
/// without a cover file, as Jellyfin does (owner, 2026-11-16). Like frames,
/// covers are pruned with the cache and written again when asked for.
pub async fn embedded_cover(track: &Path, cache_dir: &Path, key: &str) -> Option<PathBuf> {
    let safe_key: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let dir = cache_dir.join("covers");
    let out = dir.join(format!("{safe_key}.img"));
    if out.exists() {
        return Some(out);
    }
    let lock = in_flight(&out);
    let _guard = lock.lock().await;
    if out.exists() {
        done_in_flight(&out);
        return Some(out);
    }
    let track = track.to_path_buf();
    let picture = tokio::task::spawn_blocking(move || read_embedded_picture(&track))
        .await
        .ok()
        .flatten();
    let result = picture.and_then(|bytes| {
        std::fs::create_dir_all(&dir).ok()?;
        let tmp = dir.join(format!(".{safe_key}.{}.tmp", uuid::Uuid::new_v4().simple()));
        if std::fs::write(&tmp, bytes)
            .and_then(|()| std::fs::rename(&tmp, &out))
            .is_err()
        {
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
        Some(out.clone())
    });
    done_in_flight(&out);
    result
}

fn read_embedded_picture(track: &Path) -> Option<Vec<u8>> {
    use lofty::file::TaggedFileExt;
    use lofty::picture::PictureType;
    let tagged = lofty::read_from_path(track).ok()?;
    let pictures: Vec<&lofty::picture::Picture> =
        tagged.tags().iter().flat_map(|t| t.pictures()).collect();
    let picture = pictures
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures.first())?;
    let data = picture.data();
    (!data.is_empty()).then(|| data.to_vec())
}

async fn grab(
    ffmpeg: &str,
    video: &Path,
    out: &Path,
    at_secs: f64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let run = tokio::process::Command::new(ffmpeg)
        .args(["-v", "error", "-ss", &format!("{at_secs:.3}"), "-i"])
        .arg(video)
        .args([
            "-frames:v",
            "1",
            "-vf",
            "scale='min(1280,iw)':-2",
            "-q:v",
            "3",
            "-f",
            "image2",
            "-y",
        ])
        .arg(out)
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(std::time::Duration::from_secs(60), run)
        .await
        .map_err(|_| "ffmpeg took over a minute")??;
    let written = std::fs::metadata(out).map(|m| m.len() > 0).unwrap_or(false);
    if !output.status.success() || !written {
        let _ = std::fs::remove_file(out);
        return Err(format!(
            "ffmpeg could not grab a frame: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(())
}

/// The ETag for `source` served with `opts`: it changes when the file does
/// (size, mtime) or the requested size or format does.
pub fn etag_for(source_path: &str, len: u64, mtime_ms: u128, opts: &ImageOptions) -> String {
    let key = format!(
        "{source_path}:{len}:{mtime_ms}:{}:{}:{}:{}:{}:{:?}:{CACHE_VERSION}",
        opts.max_width.unwrap_or(0),
        opts.max_height.unwrap_or(0),
        opts.width.unwrap_or(0),
        opts.height.unwrap_or(0),
        opts.quality,
        opts.format,
    );
    let hash = Sha256::digest(key.as_bytes());
    let hex: String = hash[..12].iter().map(|b| format!("{b:02x}")).collect();
    format!("\"{hex}\"")
}

/// An image file opened for streaming: its length, the type its first bytes
/// name, and a stream of its contents.
pub struct OpenImage {
    pub len: u64,
    pub content_type: Option<&'static str>,
    file: tokio::fs::File,
}

impl OpenImage {
    pub async fn open(path: &Path) -> std::io::Result<Self> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        let mut file = tokio::fs::File::open(path).await?;
        let len = file.metadata().await?.len();
        let mut head = [0u8; 16];
        let mut read = 0;
        while read < head.len() {
            let n = file.read(&mut head[read..]).await?;
            if n == 0 {
                break;
            }
            read += n;
        }
        file.seek(std::io::SeekFrom::Start(0)).await?;
        Ok(Self {
            len,
            content_type: sniff_content_type(&head[..read]),
            file,
        })
    }

    /// The file's contents, read in chunks as they are sent.
    pub fn into_stream(self) -> tokio_util::io::ReaderStream<tokio::fs::File> {
        tokio_util::io::ReaderStream::new(self.file)
    }
}

fn resize_and_save(
    source_path: &str,
    opts: &ImageOptions,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use fast_image_resize as fr;
    use image::{DynamicImage, ImageEncoder};

    // By contents, not extension: Kodi `.tbn` thumbnails are JPEGs, and artwork
    // saved under the wrong extension (a PNG as `poster.jpg`) is common.
    let img = image::ImageReader::open(source_path)?
        .with_guessed_format()?
        .decode()?;
    let (src_w, src_h) = (img.width(), img.height());

    let (dst_w, dst_h) = calculate_dimensions(src_w, src_h, opts);

    if dst_w == src_w && dst_h == src_h && matches!(opts.format, ImageFormat::Original) {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source_path, output_path)?;
        return Ok(());
    }

    // Transparency (a title logo) needs a format that keeps it: without a
    // named format such a picture is answered as a PNG, as Jellyfin does.
    // An alpha channel that is opaque everywhere (a poster saved as RGBA)
    // still goes out as the smaller JPEG.
    let src_rgba = img.to_rgba8();
    let keeps_alpha = img.color().has_alpha() && src_rgba.pixels().any(|p| p[3] < 255);
    let src_image =
        fr::images::Image::from_vec_u8(src_w, src_h, src_rgba.into_raw(), fr::PixelType::U8x4)?;

    let mut dst_image = fr::images::Image::new(dst_w, dst_h, fr::PixelType::U8x4);
    let mut resizer = fr::Resizer::new();
    resizer.resize(
        &src_image,
        &mut dst_image,
        &fr::ResizeOptions::new().resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3)),
    )?;

    let result_buf = image::RgbaImage::from_raw(dst_w, dst_h, dst_image.into_vec())
        .ok_or("Failed to create output image")?;
    let result_img = DynamicImage::ImageRgba8(result_buf);

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match opts.format {
        ImageFormat::Webp => {
            result_img.save_with_format(output_path, image::ImageFormat::WebP)?;
        }
        ImageFormat::Png => {
            result_img.save_with_format(output_path, image::ImageFormat::Png)?;
        }
        ImageFormat::Original if keeps_alpha => {
            result_img.save_with_format(output_path, image::ImageFormat::Png)?;
        }
        _ => {
            let rgb = result_img.to_rgb8();
            let mut writer = std::io::BufWriter::new(std::fs::File::create(output_path)?);
            let encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, opts.quality);
            encoder.write_image(&rgb, dst_w, dst_h, image::ExtendedColorType::Rgb8)?;
        }
    }

    Ok(())
}

fn calculate_dimensions(src_w: u32, src_h: u32, opts: &ImageOptions) -> (u32, u32) {
    if let (Some(w), Some(h)) = (opts.width, opts.height) {
        return (w.max(1), h.max(1));
    }
    if let Some(w) = opts.width {
        let ratio = w as f64 / src_w as f64;
        return (w.max(1), (src_h as f64 * ratio).round().max(1.0) as u32);
    }
    if let Some(h) = opts.height {
        let ratio = h as f64 / src_h as f64;
        return ((src_w as f64 * ratio).round().max(1.0) as u32, h.max(1));
    }

    let mut w = src_w;
    let mut h = src_h;
    if let Some(max_w) = opts.max_width
        && w > max_w
    {
        let ratio = max_w as f64 / w as f64;
        w = max_w;
        h = (h as f64 * ratio).round().max(1.0) as u32;
    }
    if let Some(max_h) = opts.max_height
        && h > max_h
    {
        let ratio = max_h as f64 / h as f64;
        h = max_h;
        w = (w as f64 * ratio).round().max(1.0) as u32;
    }
    (w.max(1), h.max(1))
}

pub async fn cleanup_cache(cache_dir: &Path, max_age: std::time::Duration) -> std::io::Result<u64> {
    let mut removed = 0u64;
    let now = std::time::SystemTime::now();

    let mut entries = tokio::fs::read_dir(cache_dir).await?;
    while let Some(subdir) = entries.next_entry().await? {
        if !subdir.file_type().await?.is_dir() {
            continue;
        }
        let mut files = tokio::fs::read_dir(subdir.path()).await?;
        while let Some(file) = files.next_entry().await? {
            if let Ok(meta) = file.metadata().await
                && let Ok(modified) = meta.modified()
                && let Ok(age) = now.duration_since(modified)
                && age > max_age
            {
                let _ = tokio::fs::remove_file(file.path()).await;
                removed += 1;
            }
        }
    }
    Ok(removed)
}

/// The plain tile served as an item's primary image while it has no artwork
/// (owner decision 2026-11-14: a placeholder rather than frames grabbed from
/// the video). One per shape, drawn on first use into the image cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder {
    /// 2:3, films, series, seasons and anything without a better shape.
    Poster,
    /// 16:9, episodes, videos and libraries.
    Landscape,
    /// 1:1, music.
    Square,
}

impl Placeholder {
    /// BlurHash of the placeholder's single colour (#303038).
    pub const BLURHASH: &'static str = "005hZ5";
    const COLOUR: [u8; 3] = [0x30, 0x30, 0x38];

    /// The shape Jellyfin clients expect for an item type.
    pub fn for_item_type(item_type: &str) -> Self {
        match item_type {
            "Episode" | "Video" | "MusicVideo" | "Trailer" | "CollectionFolder" | "UserView"
            | "Folder" => Placeholder::Landscape,
            "Audio" | "MusicAlbum" | "MusicArtist" | "Playlist" | "AudioBook" => {
                Placeholder::Square
            }
            _ => Placeholder::Poster,
        }
    }

    /// The image tag clients send back; bump the suffix if the picture changes.
    pub fn tag(self) -> &'static str {
        match self {
            Placeholder::Poster => "placeholder-poster-1",
            Placeholder::Landscape => "placeholder-landscape-1",
            Placeholder::Square => "placeholder-square-1",
        }
    }

    /// Width and height of the drawn tile.
    pub fn size(self) -> (u32, u32) {
        match self {
            Placeholder::Poster => (600, 900),
            Placeholder::Landscape => (960, 540),
            Placeholder::Square => (600, 600),
        }
    }

    pub fn aspect_ratio(self) -> f64 {
        let (w, h) = self.size();
        f64::from(w) / f64::from(h)
    }

    /// The placeholder PNG under `cache_dir`, drawn the first time it is asked
    /// for. Written to a unique temporary name and renamed, so concurrent first
    /// requests never see a half-written file.
    pub fn ensure_file(self, cache_dir: &Path) -> std::io::Result<PathBuf> {
        let dir = cache_dir.join("placeholders");
        let path = dir.join(format!("{}.png", self.tag()));
        if path.exists() {
            return Ok(path);
        }
        std::fs::create_dir_all(&dir)?;
        let (w, h) = self.size();
        let tile = image::RgbImage::from_pixel(w, h, image::Rgb(Self::COLOUR));
        let tmp = dir.join(format!(
            "{}.{}.tmp",
            self.tag(),
            uuid::Uuid::new_v4().simple()
        ));
        tile.save_with_format(&tmp, image::ImageFormat::Png)
            .map_err(std::io::Error::other)?;
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }
}

/// The MIME type of image bytes from their signature, if recognised.
/// Whether `process_image` failed because the file is not an image it can read
/// (not a picture at all, a format it does not know, or a truncated one), as
/// opposed to a fault of the server (cache not writable, a task that died).
pub fn is_unreadable_image(e: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    e.downcast_ref::<image::ImageError>().is_some_and(|e| {
        matches!(
            e,
            image::ImageError::Decoding(_)
                | image::ImageError::Unsupported(_)
                | image::ImageError::Limits(_)
        )
    })
}

pub fn sniff_content_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        None
    }
}
