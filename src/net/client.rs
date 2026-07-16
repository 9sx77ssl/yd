use color_eyre::eyre::Result;
use reqwest::{Client, RequestBuilder};
use serde::de::DeserializeOwned;

use crate::error::YdError;

/// A shared HTTP client with the yd user agent and a conservative timeout.
pub fn shared_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .user_agent(concat!("yd/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("valid client")
}

/// Identifies an external API for error attribution.
///
/// A plain service name is all the typed errors need; callers pass a string
/// literal (or a chain's static name) and every transport/parse failure is
/// reported with that name, so wallet, notes, or weather fail identically.
#[derive(Clone, Copy, Debug)]
pub struct ApiService {
    service: &'static str,
}

impl ApiService {
    pub const fn new(service: &'static str) -> Self {
        Self { service }
    }

    /// Sends `request`, maps transport and HTTP-status failures into a typed
    /// [`YdError::ApiRequest`], and deserialises the JSON body into `T`.
    pub async fn json<T>(self, request: RequestBuilder) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = request.send().await.map_err(|source| YdError::ApiRequest {
            service: self.service,
            source,
        })?;
        let response = response
            .error_for_status()
            .map_err(|source| YdError::ApiRequest {
                service: self.service,
                source,
            })?;
        response
            .json::<T>()
            .await
            .map_err(|source| YdError::ApiRequest {
                service: self.service,
                source,
            })
            .map_err(Into::into)
    }

    /// Builds an invalid-payload error attributed to this service.
    pub fn invalid_data(self, detail: impl Into<String>) -> YdError {
        YdError::ApiData {
            service: self.service,
            detail: detail.into(),
        }
    }
}
