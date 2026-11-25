use rustyfin_models::data::DataError;

/// Why a service refused or failed. Neutral: each API crate decides what
/// status and body a kind becomes.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// What was asked for does not exist, or the caller may not know it
    /// exists.
    #[error("not found")]
    NotFound,

    /// The caller may see it but may not do this.
    #[error("forbidden")]
    Forbidden,

    /// The request makes no sense as asked.
    #[error("invalid: {0}")]
    Invalid(String),

    /// The data layer failed.
    #[error(transparent)]
    Data(#[from] DataError),
}

/// What a service call returns.
pub type ServiceResult<T> = Result<T, ServiceError>;
