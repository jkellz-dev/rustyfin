use std::path::Path;

pub async fn download_image(
    client: &reqwest::Client,
    url: &str,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} for {url}", resp.status()).into());
    }
    let bytes = resp.bytes().await?;
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, &bytes)?;
    Ok(())
}

pub fn tmdb_image_url(path: &str, size: &str) -> String {
    format!("https://image.tmdb.org/t/p/{size}{path}")
}
