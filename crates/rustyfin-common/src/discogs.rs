use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::nfo::NfoMetadata;
use crate::tmdb::SearchResult;

const DISCOGS_API_URL: &str = "https://api.discogs.com";
const DISCOGS_AUTH_URL: &str = "https://www.discogs.com/oauth/authorize";

pub struct DiscogsClient {
    consumer_key: String,
    consumer_secret: String,
    access_token: Option<String>,
    access_token_secret: Option<String>,
    client: reqwest::Client,
}

#[derive(Serialize, Deserialize)]
struct StoredTokens {
    access_token: String,
    access_token_secret: String,
}

impl DiscogsClient {
    pub fn new(consumer_key: String, consumer_secret: String) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("rustyfin/0.1.0")
            .build()
            .expect("failed to build HTTP client");

        Self {
            consumer_key,
            consumer_secret,
            access_token: None,
            access_token_secret: None,
            client,
        }
    }

    /// Returns true if access tokens have been loaded or obtained.
    pub fn is_authenticated(&self) -> bool {
        self.access_token.is_some()
    }

    /// Load previously stored tokens from a file.
    pub fn load_tokens(&mut self, token_file: &Path) {
        if let Ok(content) = std::fs::read_to_string(token_file)
            && let Ok(tokens) = serde_json::from_str::<StoredTokens>(&content)
        {
            self.access_token = Some(tokens.access_token);
            self.access_token_secret = Some(tokens.access_token_secret);
            tracing::info!("Loaded Discogs tokens from {}", token_file.display());
        }
    }

    /// Run the OAuth 1.0a flow to get access tokens.
    /// Opens browser for authorization (auto-detect display, fall back to printing URL).
    pub async fn authenticate(
        &mut self,
        token_file: &Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.access_token.is_some() {
            return Ok(());
        }

        // Step 1: Get request token
        let oauth_header = format!(
            r#"OAuth oauth_consumer_key="{}", oauth_signature_method="PLAINTEXT", oauth_signature="{}&", oauth_callback="oob", oauth_nonce="{}", oauth_timestamp="{}""#,
            self.consumer_key,
            self.consumer_secret,
            uuid::Uuid::new_v4(),
            chrono::Utc::now().timestamp(),
        );

        let resp = self
            .client
            .get(format!("{DISCOGS_API_URL}/oauth/request_token"))
            .header("Authorization", &oauth_header)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await?;

        let body = resp.text().await?;
        let params: std::collections::HashMap<String, String> = serde_urlencoded::from_str(&body)?;
        let request_token = params
            .get("oauth_token")
            .ok_or("missing oauth_token in request token response")?;
        let request_token_secret = params
            .get("oauth_token_secret")
            .ok_or("missing oauth_token_secret")?;

        // Step 2: Authorize — open browser or print URL
        let auth_url = format!("{DISCOGS_AUTH_URL}?oauth_token={request_token}");

        let has_display =
            std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok();

        if has_display {
            tracing::info!("Opening browser for Discogs authorization...");
            if open::that(&auth_url).is_err() {
                println!("Please open this URL in your browser:\n{auth_url}");
            }
        } else {
            println!("Please open this URL in your browser:\n{auth_url}");
        }

        // Step 3: Get verification code from user
        println!("Enter the verification code from Discogs:");
        let mut verifier = String::new();
        std::io::stdin().read_line(&mut verifier)?;
        let verifier = verifier.trim();

        // Step 4: Exchange for access token
        let oauth_header = format!(
            r#"OAuth oauth_consumer_key="{}", oauth_token="{}", oauth_signature_method="PLAINTEXT", oauth_signature="{}&{}", oauth_verifier="{}", oauth_nonce="{}", oauth_timestamp="{}""#,
            self.consumer_key,
            request_token,
            self.consumer_secret,
            request_token_secret,
            verifier,
            uuid::Uuid::new_v4(),
            chrono::Utc::now().timestamp(),
        );

        let resp = self
            .client
            .post(format!("{DISCOGS_API_URL}/oauth/access_token"))
            .header("Authorization", &oauth_header)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await?;

        let body = resp.text().await?;
        let params: std::collections::HashMap<String, String> = serde_urlencoded::from_str(&body)?;
        let access_token = params
            .get("oauth_token")
            .ok_or("missing oauth_token in access token response")?
            .clone();
        let access_token_secret = params
            .get("oauth_token_secret")
            .ok_or("missing oauth_token_secret in access token response")?
            .clone();

        // Store tokens
        self.access_token = Some(access_token.clone());
        self.access_token_secret = Some(access_token_secret.clone());

        // Persist to file
        if let Some(parent) = token_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let stored = StoredTokens {
            access_token,
            access_token_secret,
        };
        std::fs::write(token_file, serde_json::to_string_pretty(&stored)?)?;
        tracing::info!("Saved Discogs tokens to {}", token_file.display());

        Ok(())
    }

    /// Build OAuth 1.0a Authorization header for an authenticated request.
    fn auth_header(&self) -> Option<String> {
        let token = self.access_token.as_ref()?;
        let token_secret = self.access_token_secret.as_ref()?;
        Some(format!(
            r#"OAuth oauth_consumer_key="{}", oauth_token="{}", oauth_signature_method="PLAINTEXT", oauth_signature="{}&{}", oauth_nonce="{}", oauth_timestamp="{}""#,
            self.consumer_key,
            token,
            self.consumer_secret,
            token_secret,
            uuid::Uuid::new_v4(),
            chrono::Utc::now().timestamp(),
        ))
    }

    /// Search for an artist by name.
    pub async fn search_artist(&self, name: &str) -> Vec<SearchResult> {
        let Some(auth) = self.auth_header() else {
            return Vec::new();
        };
        let url = format!(
            "{DISCOGS_API_URL}/database/search?q={}&type=artist&per_page=10",
            urlencoded(name)
        );

        let resp = match self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                tracing::debug!("Discogs artist search: {}", r.status());
                return Vec::new();
            }
            Err(e) => {
                tracing::error!("Discogs search failed: {e}");
                return Vec::new();
            }
        };

        let body: DiscogsSearchResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("Discogs search parse error: {e}");
                return Vec::new();
            }
        };

        body.results
            .into_iter()
            .map(|r| SearchResult {
                id: r.id,
                title: r.title,
                year: None,
                overview: None,
                mbid: None,
            })
            .collect()
    }

    /// Fetch artist details by Discogs ID.
    pub async fn fetch_artist(&self, id: i64) -> Option<NfoMetadata> {
        let auth = self.auth_header()?;
        let url = format!("{DISCOGS_API_URL}/artists/{id}");

        let resp = self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }

        let artist: DiscogsArtist = resp.json().await.ok()?;

        Some(NfoMetadata {
            title: Some(artist.name),
            overview: artist.profile,
            genres: Vec::new(), // Discogs has genres on releases, not artists
            ..Default::default()
        })
    }

    /// Get artist image URLs from Discogs.
    pub async fn fetch_artist_image_urls(&self, id: i64) -> Vec<String> {
        let auth = self.auth_header();
        let Some(auth) = auth else {
            return Vec::new();
        };
        let url = format!("{DISCOGS_API_URL}/artists/{id}");

        let resp = match self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            _ => return Vec::new(),
        };

        let artist: DiscogsArtist = match resp.json().await {
            Ok(a) => a,
            _ => return Vec::new(),
        };

        artist
            .images
            .unwrap_or_default()
            .into_iter()
            .filter(|i| i.image_type == "primary" || i.image_type == "secondary")
            .map(|i| i.uri)
            .collect()
    }

    /// Search for a release by album name and artist.
    pub async fn search_release(&self, album: &str, artist: &str) -> Vec<SearchResult> {
        let Some(auth) = self.auth_header() else {
            return Vec::new();
        };
        let url = format!(
            "{DISCOGS_API_URL}/database/search?release_title={}&artist={}&type=release&per_page=10",
            urlencoded(album),
            urlencoded(artist)
        );

        let resp = match self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            _ => return Vec::new(),
        };

        let body: DiscogsSearchResponse = match resp.json().await {
            Ok(b) => b,
            _ => return Vec::new(),
        };

        body.results
            .into_iter()
            .map(|r| SearchResult {
                id: r.id,
                title: r.title,
                year: r.year.and_then(|y| y.parse().ok()),
                overview: None,
                mbid: None,
            })
            .collect()
    }

    /// Fetch release details by Discogs ID.
    pub async fn fetch_release(&self, id: i64) -> Option<NfoMetadata> {
        let auth = self.auth_header()?;
        let url = format!("{DISCOGS_API_URL}/releases/{id}");

        let resp = self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }

        let release: DiscogsRelease = resp.json().await.ok()?;

        let label = release
            .labels
            .and_then(|l| l.into_iter().next())
            .map(|l| l.name);

        Some(NfoMetadata {
            title: Some(release.title),
            production_year: release.year,
            genres: release.genres.unwrap_or_default(),
            studios: label.into_iter().collect(),
            ..Default::default()
        })
    }
}

fn urlencoded(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('+', "%2B")
}

// -- Discogs API response types -----------------------------------------------

#[derive(Deserialize)]
struct DiscogsSearchResponse {
    #[serde(default)]
    results: Vec<DiscogsSearchResult>,
}

#[derive(Deserialize)]
struct DiscogsSearchResult {
    id: i64,
    title: String,
    year: Option<String>,
}

#[derive(Deserialize)]
struct DiscogsArtist {
    name: String,
    profile: Option<String>,
    images: Option<Vec<DiscogsImage>>,
}

#[derive(Deserialize)]
struct DiscogsImage {
    #[serde(rename = "type")]
    image_type: String,
    uri: String,
}

#[derive(Deserialize)]
struct DiscogsRelease {
    title: String,
    year: Option<i32>,
    genres: Option<Vec<String>>,
    labels: Option<Vec<DiscogsLabel>>,
}

#[derive(Deserialize)]
struct DiscogsLabel {
    name: String,
}
