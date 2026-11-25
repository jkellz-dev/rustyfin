use std::fmt;

/// Why the data layer could not do what was asked. It says what kind of
/// failure it was, never which database produced it, so callers can answer
/// (retry later, unknown reference, fault) without knowing the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataError {
    /// The store cannot be reached right now (every connection busy, the
    /// store shut or unreachable): worth a retry, not a fault in the request.
    Unavailable(String),
    /// The change refers to something that does not exist (a user's data
    /// for an unknown item, say).
    MissingReference(String),
    /// Anything else: a fault, logged as an error.
    Other(String),
}

/// The result every data-layer method returns.
pub type DataResult<T> = Result<T, DataError>;

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataError::Unavailable(e) => write!(f, "data store unavailable: {e}"),
            DataError::MissingReference(e) => write!(f, "refers to something missing: {e}"),
            DataError::Other(e) => write!(f, "data error: {e}"),
        }
    }
}

impl std::error::Error for DataError {}
