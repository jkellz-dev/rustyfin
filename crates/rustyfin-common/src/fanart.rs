const FANART_BASE_URL: &str = "https://webservice.fanart.tv/v3.2";

pub struct FanartClient {
    api_key: String,
    client: reqwest::Client,
}

/// A downloadable image from fanart.tv.
#[derive(Debug, Clone)]
pub struct FanartImage {
    pub url: String,
    pub image_type: String,
    pub sidecar_name: String,
    pub likes: i32,
}

impl FanartClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// Fetch movie artwork by TMDB ID.
    pub async fn fetch_movie_images(&self, tmdb_id: &str) -> Vec<FanartImage> {
        let url = format!(
            "{}/movies/{}?api_key={}",
            FANART_BASE_URL, tmdb_id, self.api_key
        );
        let body = match self.fetch_json(&url, "movie", tmdb_id).await {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut images = Vec::new();
        extract_images(&body, "hdmovielogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "movielogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "moviedisc", "Disc", "disc.png", &mut images);
        extract_images(&body, "moviebanner", "Banner", "banner.jpg", &mut images);
        extract_images(&body, "moviethumb", "Thumb", "landscape.jpg", &mut images);
        extract_images(&body, "hdmovieclearart", "Art", "clearart.png", &mut images);
        extract_backdrop_images(&body, "moviebackground", &mut images);
        images
    }

    /// Fetch TV show artwork by TVDB ID.
    pub async fn fetch_show_images(&self, tvdb_id: &str) -> Vec<FanartImage> {
        let url = format!(
            "{}/tv/{}?api_key={}",
            FANART_BASE_URL, tvdb_id, self.api_key
        );
        let body = match self.fetch_json(&url, "show", tvdb_id).await {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut images = Vec::new();
        extract_images(&body, "hdtvlogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "clearlogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "hdclearart", "Art", "clearart.png", &mut images);
        extract_images(&body, "tvthumb", "Thumb", "landscape.jpg", &mut images);
        extract_images(&body, "tvbanner", "Banner", "banner.jpg", &mut images);
        extract_images(
            &body,
            "characterart",
            "Art",
            "characterart.png",
            &mut images,
        );
        extract_backdrop_images(&body, "showbackground", &mut images);
        images
    }

    /// Fetch music artist artwork by MusicBrainz ID.
    pub async fn fetch_artist_images(&self, mb_artist_id: &str) -> Vec<FanartImage> {
        let url = format!(
            "{}/music/{}?api_key={}",
            FANART_BASE_URL, mb_artist_id, self.api_key
        );
        let body = match self.fetch_json(&url, "artist", mb_artist_id).await {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut images = Vec::new();
        extract_images(&body, "hdmusiclogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "musiclogo", "Logo", "logo.png", &mut images);
        extract_images(&body, "artistthumb", "Primary", "artist.jpg", &mut images);
        extract_images(&body, "musicbanner", "Banner", "banner.jpg", &mut images);
        extract_backdrop_images(&body, "artistbackground", &mut images);
        images
    }

    /// Fetch album artwork by MusicBrainz Release Group ID.
    pub async fn fetch_album_images(&self, release_group_id: &str) -> Vec<FanartImage> {
        let url = format!(
            "{}/music/albums/{}?api_key={}",
            FANART_BASE_URL, release_group_id, self.api_key
        );
        let body = match self.fetch_json(&url, "album", release_group_id).await {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut images = Vec::new();
        if let Some(albums) = body.get("albums").and_then(|a| a.as_object()) {
            for (_album_id, album_data) in albums {
                extract_images(
                    album_data,
                    "albumcover",
                    "Primary",
                    "cover.jpg",
                    &mut images,
                );
                extract_images(album_data, "cdart", "Disc", "disc.png", &mut images);
            }
        }
        images
    }

    /// Fetch JSON from a URL, handling errors consistently.
    async fn fetch_json(
        &self,
        url: &str,
        entity_type: &str,
        id: &str,
    ) -> Option<serde_json::Value> {
        let resp = match self.client.get(url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                tracing::debug!("fanart.tv {} {}: {}", entity_type, id, r.status());
                return None;
            }
            Err(e) => {
                tracing::error!("fanart.tv {} request failed: {e}", entity_type);
                return None;
            }
        };

        resp.json().await.ok()
    }
}

/// Extract images of a given type from a fanart.tv JSON response.
/// Takes the first image (sorted by likes descending).
fn extract_images(
    body: &serde_json::Value,
    json_key: &str,
    image_type: &str,
    sidecar_name: &str,
    out: &mut Vec<FanartImage>,
) {
    // Don't add if we already have this sidecar name (e.g., hdmovielogo before movielogo)
    if out.iter().any(|i| i.sidecar_name == sidecar_name) {
        return;
    }

    let Some(arr) = body.get(json_key).and_then(|v| v.as_array()) else {
        return;
    };

    // Sort by likes descending, take first
    let mut items: Vec<_> = arr
        .iter()
        .filter_map(|item| {
            let url = item.get("url")?.as_str()?.to_string();
            let likes = item
                .get("likes")
                .and_then(|l| l.as_str())
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0);
            Some((url, likes))
        })
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item.1));

    if let Some((url, likes)) = items.into_iter().next() {
        out.push(FanartImage {
            url,
            image_type: image_type.to_string(),
            sidecar_name: sidecar_name.to_string(),
            likes,
        });
    }
}

/// Extract backdrop/background images with numbered filenames (fanart.jpg, fanart1.jpg, ...).
fn extract_backdrop_images(body: &serde_json::Value, json_key: &str, out: &mut Vec<FanartImage>) {
    let Some(arr) = body.get(json_key).and_then(|v| v.as_array()) else {
        return;
    };

    let mut items: Vec<_> = arr
        .iter()
        .filter_map(|item| {
            let url = item.get("url")?.as_str()?.to_string();
            let likes = item
                .get("likes")
                .and_then(|l| l.as_str())
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0);
            Some((url, likes))
        })
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item.1));

    for (i, (url, likes)) in items.into_iter().enumerate() {
        let sidecar_name = if i == 0 {
            "fanart.jpg".to_string()
        } else {
            format!("fanart{i}.jpg")
        };
        out.push(FanartImage {
            url,
            image_type: "Backdrop".to_string(),
            sidecar_name,
            likes,
        });
    }
}
