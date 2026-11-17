//! What went wrong with a request, and how to say it (roadmap 12c and 12g:
//! structured errors in the API client, messages people can read instead of
//! "HTTP 500", with Retry offered on every error notice).

use std::fmt;

/// A failed request to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// The request never got an answer (server down, network gone).
    Network,
    /// The server answered with an error status; `detail` is its message.
    Status { code: u16, detail: String },
    /// The answer was not what the page expected.
    Parse,
}

impl ApiError {
    /// From a status code and the response body.
    pub fn from_status(code: u16, body: &str) -> ApiError {
        // Rustyfin's own errors are "Kind: reason" text; Jellyfin-style 404s
        // are JSON problem details, whose title says nothing new.
        let text = body.trim();
        let detail = if text.starts_with('{') {
            String::new()
        } else {
            ["Bad request: ", "Conflict: "]
                .iter()
                .find_map(|p| text.strip_prefix(p))
                .unwrap_or("")
                .trim()
                .to_string()
        };
        ApiError::Status { code, detail }
    }

    /// The message to show.
    pub fn message(&self) -> String {
        match self {
            ApiError::Network => {
                "Can't reach the server. Check your connection and try again.".to_string()
            }
            ApiError::Parse => {
                "The server sent an answer this page doesn't understand.".to_string()
            }
            ApiError::Status { code, detail } => match code {
                400 | 409 if !detail.is_empty() => detail.clone(),
                400 | 409 | 422 => "The server didn't accept that request.".to_string(),
                401 => "Your sign-in has expired. Reload the page to sign in again.".to_string(),
                403 => "You don't have permission to do that.".to_string(),
                404 => "That item no longer exists.".to_string(),
                429 | 503 => "The server is busy. Try again in a moment.".to_string(),
                _ => format!("The server ran into a problem ({code})."),
            },
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl From<ApiError> for String {
    fn from(e: ApiError) -> String {
        e.message()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_read_as_sentences() {
        assert_eq!(
            ApiError::from_status(401, "").message(),
            "Your sign-in has expired. Reload the page to sign in again."
        );
        assert_eq!(
            ApiError::from_status(403, "").message(),
            "You don't have permission to do that."
        );
        assert_eq!(
            ApiError::from_status(404, r#"{"title":"Not Found","status":404}"#).message(),
            "That item no longer exists."
        );
        assert_eq!(
            ApiError::from_status(503, "").message(),
            "The server is busy. Try again in a moment."
        );
        assert_eq!(
            ApiError::from_status(500, "Internal error: boom").message(),
            "The server ran into a problem (500)."
        );
        assert_eq!(
            ApiError::Network.message(),
            "Can't reach the server. Check your connection and try again."
        );
        assert_eq!(
            ApiError::Parse.message(),
            "The server sent an answer this page doesn't understand."
        );
    }

    #[test]
    fn the_servers_own_reason_is_kept_for_bad_requests() {
        let e = ApiError::from_status(
            400,
            "Bad request: Cannot read directory '/mnt/x': Permission denied (os error 13)",
        );
        assert_eq!(
            e.message(),
            "Cannot read directory '/mnt/x': Permission denied (os error 13)"
        );
        let e = ApiError::from_status(409, "Conflict: A library named Movies already exists");
        assert_eq!(e.message(), "A library named Movies already exists");
        assert_eq!(
            ApiError::from_status(400, "").message(),
            "The server didn't accept that request."
        );
    }
}
