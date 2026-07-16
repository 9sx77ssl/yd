use thiserror::Error;

/// Typed errors shared across every domain.
///
/// Domain-specific failures keep a dedicated variant. Generic infra failures
/// (storage, secrets, network) carry a short `context` label so the same
/// variant reads naturally for wallet, notes, or any future module.
#[derive(Debug, Error)]
pub enum YdError {
    #[error("the seed phrase is invalid: {0}")]
    InvalidMnemonic(String),

    #[error("could not access the system keyring; install and unlock a supported keyring service before using yd")]
    KeyringUnavailable,

    #[error("{context} data is corrupted or cannot be decrypted")]
    Corrupted { context: &'static str },

    #[error("{service} API request failed")]
    ApiRequest {
        service: &'static str,
        #[source]
        source: reqwest::Error,
    },

    #[error("{service} API returned invalid data: {detail}")]
    ApiData {
        service: &'static str,
        detail: String,
    },
}
