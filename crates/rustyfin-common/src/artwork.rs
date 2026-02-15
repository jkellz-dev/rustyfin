use std::path::Path;

/// Download an image from a URL to disk.
/// - Retries up to 3 times with exponential backoff (1s, 2s, 4s) on transient errors.
/// - Does NOT retry on 404 (image doesn't exist).
pub async fn download_image(
    client: &reqwest::Client,
    url: &str,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    download_image_with_options(client, url, output_path, false).await
}

/// Download an image with skip-existing option.
/// If `skip_existing` is true and the file already exists, returns `Ok(())` immediately.
pub async fn download_image_with_options(
    client: &reqwest::Client,
    url: &str,
    output_path: &Path,
    skip_existing: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if skip_existing && output_path.exists() {
        tracing::debug!("Skipping existing file: {}", output_path.display());
        return Ok(());
    }

    let mut last_err = None;
    for attempt in 0..3u32 {
        if attempt > 0 {
            let delay = std::time::Duration::from_secs(1 << attempt); // 2s, 4s
            tracing::debug!("Retry {attempt}/2 for {url} after {delay:?}");
            tokio::time::sleep(delay).await;
        }

        match client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    let bytes = resp.bytes().await?;
                    if let Some(parent) = output_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(output_path, &bytes)?;
                    return Ok(());
                }
                // Don't retry 404
                if status.as_u16() == 404 {
                    return Err(format!("HTTP 404 for {url}").into());
                }
                // Retry on 5xx
                if status.is_server_error() {
                    last_err = Some(format!("HTTP {status} for {url}"));
                    continue;
                }
                // Other client errors: don't retry
                return Err(format!("HTTP {status} for {url}").into());
            }
            Err(e) => {
                // Connection/timeout errors: retry
                last_err = Some(format!("{e}"));
                continue;
            }
        }
    }

    Err(last_err
        .unwrap_or_else(|| "Unknown error".to_string())
        .into())
}

pub fn tmdb_image_url(path: &str, size: &str) -> String {
    format!("https://image.tmdb.org/t/p/{size}{path}")
}
