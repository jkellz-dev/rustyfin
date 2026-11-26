//! Access tokens: issuing one for a session and deciding whether one still
//! works. Who the user is was settled before (the proxy in front); a token
//! only ties later requests to that user and their device.

use rand::RngExt;
use rustyfin_models::data::{Data, DataError, Session, SessionStore, User, UserStore};
use uuid::Uuid;

/// Generate a random access token (64-char hex string).
pub fn generate_token() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    hex::encode(bytes)
}

/// Create a new session for a user and return (access_token, session_id).
pub async fn create_session<D: Data>(
    data: &D,
    user_id: Uuid,
    device_id: &str,
    device_name: &str,
    app_name: &str,
    app_version: Option<&str>,
) -> Result<(String, Uuid), DataError> {
    let token = generate_token();
    let session = data
        .sessions()
        .create(
            &token,
            user_id,
            device_id,
            device_name,
            app_name,
            app_version,
        )
        .await?;
    Ok((token, session.id))
}

/// Why an access token was refused.
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    /// No active session has this token (never issued, or logged out).
    #[error("unknown access token")]
    Unknown,
    /// The session exists but its `expires_at` has passed.
    #[error("session expired")]
    Expired,
    /// The session's user no longer exists.
    #[error("user not found")]
    UserMissing,
    /// The session's user is disabled.
    #[error("user is disabled")]
    UserDisabled,
    #[error("database error: {0}")]
    Database(#[from] DataError),
}

/// A token that passed every check.
#[derive(Debug, Clone)]
pub struct ResolvedToken {
    pub session: Session,
    pub user: User,
}

/// The single answer to "does this access token still work?".
///
/// Used by the HTTP extractor on every request and by the WebSocket, both when it
/// connects and while it stays open, so a session that expires, is logged out, or
/// belongs to a user who is then disabled stops working everywhere at once.
pub async fn resolve_token<D: Data>(data: &D, token: &str) -> Result<ResolvedToken, TokenError> {
    let session = data
        .sessions()
        .find_by_token(token)
        .await?
        .ok_or(TokenError::Unknown)?;

    if let Some(expires_at) = session.expires_at
        && expires_at < chrono::Utc::now()
    {
        return Err(TokenError::Expired);
    }

    let user = data
        .users()
        .find_by_id(session.user_id)
        .await?
        .ok_or(TokenError::UserMissing)?;
    if user.is_disabled {
        return Err(TokenError::UserDisabled);
    }

    Ok(ResolvedToken { session, user })
}
