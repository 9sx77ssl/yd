use thiserror::Error;

#[derive(Debug, Error)]
pub enum YdError {
    #[error("the seed phrase is invalid: {0}")]
    InvalidMnemonic(String),
    #[error("could not access the system keyring; install and unlock a supported keyring service before using yd")]
    KeyringUnavailable,
    #[error("wallet data is corrupted or cannot be decrypted")]
    WalletCorrupted,
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
