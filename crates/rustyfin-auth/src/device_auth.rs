use chrono::Utc;
use rand::RngExt;
use rustyfin_data::PgData;
use rustyfin_models::data::{Data, DataError, DeviceAuthStore};

use rustyfin_data::repo::device_auth;
use rustyfin_models::dto::{DeviceCodeResponse, DeviceTokenPending, DeviceTokenResponse};

/// Generate a random device code (long, for machine use).
fn generate_device_code() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    hex::encode(bytes)
}

/// Generate a user-friendly code (e.g., "ABCD-1234").
fn generate_user_code() -> String {
    let mut rng = rand::rng();
    let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
    let part1: String = (0..4)
        .map(|_| chars[rng.random_range(0..chars.len())])
        .collect();
    let part2: String = (0..4)
        .map(|_| chars[rng.random_range(0..chars.len())])
        .collect();
    format!("{part1}-{part2}")
}

/// Initiate device authorization flow.
pub async fn initiate(
    data: &PgData,
    client_info: &serde_json::Value,
    base_url: &str,
) -> Result<DeviceCodeResponse, DataError> {
    let device_code = generate_device_code();
    let user_code = generate_user_code();
    let expires_at = Utc::now() + chrono::Duration::minutes(15);

    data.device_auth()
        .create(&device_code, &user_code, client_info, expires_at)
        .await?;

    Ok(DeviceCodeResponse {
        device_code,
        user_code: user_code.clone(),
        verification_uri: format!("{base_url}/auth/device/verify"),
        verification_uri_complete: Some(format!("{base_url}/auth/device/verify?code={user_code}")),
        expires_in: 900,
        interval: 5,
    })
}

/// Poll for device authorization result.
pub async fn poll_token(
    data: &PgData,
    device_code: &str,
) -> Result<(DeviceTokenResponse, Option<uuid::Uuid>), DataError> {
    let request = data.device_auth().find_by_device_code(device_code).await?;

    match request {
        None => Ok((
            DeviceTokenResponse::Pending(DeviceTokenPending {
                error: "invalid_grant".to_string(),
            }),
            None,
        )),
        Some(req) => {
            if req.expires_at < Utc::now() {
                Ok((
                    DeviceTokenResponse::Pending(DeviceTokenPending {
                        error: "expired_token".to_string(),
                    }),
                    None,
                ))
            } else if req.status == "authorized" {
                // Return user_id so caller can create a session
                Ok((
                    DeviceTokenResponse::Pending(DeviceTokenPending {
                        error: "placeholder".to_string(),
                    }),
                    req.user_id,
                ))
            } else {
                Ok((
                    DeviceTokenResponse::Pending(DeviceTokenPending {
                        error: "authorization_pending".to_string(),
                    }),
                    None,
                ))
            }
        }
    }
}
