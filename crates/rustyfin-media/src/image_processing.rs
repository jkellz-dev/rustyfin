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

const CACHE_VERSION: &str = "v1";

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
    let hash = format!("{:x}", Sha256::digest(key.as_bytes()));
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
    let cached = cache_path(cache_dir, source_path, opts);

    if cached.exists() {
        return Ok(cached);
    }

    let source = source_path.to_string();
    let opts = opts.clone();
    let cached_clone = cached.clone();

    tokio::task::spawn_blocking(move || resize_and_save(&source, &opts, &cached_clone)).await??;

    Ok(cached)
}

fn resize_and_save(
    source_path: &str,
    opts: &ImageOptions,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use fast_image_resize as fr;
    use image::{DynamicImage, ImageEncoder};

    let img = image::open(source_path)?;
    let (src_w, src_h) = (img.width(), img.height());

    let (dst_w, dst_h) = calculate_dimensions(src_w, src_h, opts);

    if dst_w == src_w && dst_h == src_h && matches!(opts.format, ImageFormat::Original) {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source_path, output_path)?;
        return Ok(());
    }

    let src_rgba = img.to_rgba8();
    let src_image =
        fr::images::Image::from_vec_u8(src_w, src_h, src_rgba.into_raw(), fr::PixelType::U8x4)?;

    let mut dst_image = fr::images::Image::new(dst_w, dst_h, fr::PixelType::U8x4);
    let mut resizer = fr::Resizer::new();
    resizer.resize(
        &src_image,
        &mut dst_image,
        &fr::ResizeOptions::new()
            .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3)),
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
