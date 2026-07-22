use async_trait::async_trait;
use color_eyre::eyre::Result;
use reqwest::Client;
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use super::client::{shared_client, ApiService};
use super::fallback::with_fallback_or_none;
use crate::store::{Database, TtlCache};

/// Cached USD quotes live for a few seconds: long enough to keep repeated
/// runs cheap, short enough to stay fresh within a terminal session.
const PRICE_CACHE_TTL_SECONDS: i64 = 25;

/// A tradeable asset the price feed knows how to quote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Asset {
    Ethereum,
    Bnb,
    Polygon,
    Bitcoin,
    Litecoin,
    Gram,
}

impl Asset {
    pub const fn cache_key(self) -> &'static str {
        match self {
            Self::Ethereum => "price:ethereum",
            Self::Bnb => "price:bnb",
            Self::Polygon => "price:polygon",
            Self::Bitcoin => "price:bitcoin",
            Self::Litecoin => "price:litecoin",
            Self::Gram => "price:gram",
        }
    }

    pub const fn coingecko_id(self) -> &'static str {
        match self {
            Self::Ethereum => "ethereum",
            Self::Bnb => "binancecoin",
            Self::Polygon => "matic-network",
            Self::Bitcoin => "bitcoin",
            Self::Litecoin => "litecoin",
            Self::Gram => "the-open-network",
        }
    }

    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Ethereum => "ETH",
            Self::Bnb => "BNB",
            Self::Polygon => "POL",
            Self::Bitcoin => "BTC",
            Self::Litecoin => "LTC",
            Self::Gram => "GRAM",
        }
    }
}

/// A USD quote source. Domains may plug in additional providers (exchange
/// APIs, on-chain oracles) without touching the cache or fallback wiring.
#[async_trait]
pub trait PriceProvider: Send + Sync {
    async fn usd_quote(&self, asset: Asset) -> Result<f64>;
}

struct CoinGeckoPriceProvider {
    client: Client,
}

#[async_trait]
impl PriceProvider for CoinGeckoPriceProvider {
    async fn usd_quote(&self, asset: Asset) -> Result<f64> {
        let prices = ApiService::new("CoinGecko")
            .json::<HashMap<String, CoinGeckoPrice>>(
                self.client
                    .get("https://api.coingecko.com/api/v3/simple/price")
                    .query(&[("ids", asset.coingecko_id()), ("vs_currencies", "usd")]),
            )
            .await?;
        prices
            .get(asset.coingecko_id())
            .map(|price| price.usd)
            .ok_or_else(|| {
                ApiService::new("CoinGecko")
                    .invalid_data(format!("missing {} quote", asset.symbol()))
                    .into()
            })
    }
}

struct CoinbasePriceProvider {
    client: Client,
}

#[async_trait]
impl PriceProvider for CoinbasePriceProvider {
    async fn usd_quote(&self, asset: Asset) -> Result<f64> {
        let response = ApiService::new("Coinbase")
            .json::<CoinbasePriceResponse>(self.client.get(format!(
                "https://api.coinbase.com/v2/prices/{}-USD/spot",
                asset.symbol()
            )))
            .await?;
        response.data.amount.parse::<f64>().map_err(|_| {
            ApiService::new("Coinbase")
                .invalid_data(format!("invalid {} quote amount", asset.symbol()))
                .into()
        })
    }
}

/// Cached USD price lookup with primary/fallback providers.
///
/// The first call within [`PRICE_CACHE_TTL_SECONDS`] is served from SQLite;
/// afterwards both providers are raced and the survivor's quote is cached.
#[derive(Clone)]
pub struct PriceService {
    database: Database,
    primary: Arc<dyn PriceProvider>,
    fallback: Arc<dyn PriceProvider>,
}

impl PriceService {
    pub fn new(database: Database) -> Self {
        Self {
            database,
            primary: Arc::new(CoinGeckoPriceProvider {
                client: shared_client(),
            }),
            fallback: Arc::new(CoinbasePriceProvider {
                client: shared_client(),
            }),
        }
    }

    /// Returns the USD quote, or `None` when every provider and the cache
    /// are unavailable. A missing quote must never abort a portfolio render.
    pub async fn usd_quote(&self, asset: Asset) -> Option<f64> {
        let now = unix_timestamp();

        let mut connection = match self.database.connect().await {
            Ok(connection) => connection,
            Err(error) => {
                tracing::debug!(%error, asset = asset.symbol(), "price cache unavailable");
                return self.fresh_quote(asset).await;
            }
        };
        match TtlCache::get(
            &mut connection,
            asset.cache_key(),
            now,
            PRICE_CACHE_TTL_SECONDS,
        )
        .await
        {
            Ok(Some(raw)) => match raw.parse::<f64>() {
                Ok(price) => return Some(price),
                Err(_) => {
                    tracing::debug!(asset = asset.symbol(), "price cache value unreadable");
                }
            },
            Ok(None) => {}
            Err(error) => {
                tracing::debug!(%error, asset = asset.symbol(), "price cache unavailable");
            }
        }

        let quote = self.fresh_quote(asset).await;
        if let Some(price) = quote {
            if let Err(error) =
                TtlCache::set(&mut connection, asset.cache_key(), &price.to_string(), now).await
            {
                tracing::debug!(%error, asset = asset.symbol(), "could not save price cache");
            }
        }
        quote
    }

    async fn fresh_quote(&self, asset: Asset) -> Option<f64> {
        with_fallback_or_none(
            self.primary.usd_quote(asset),
            self.fallback.usd_quote(asset),
        )
        .await
    }
}

#[derive(Deserialize)]
struct CoinGeckoPrice {
    #[serde(rename = "usd")]
    usd: f64,
}

#[derive(Deserialize)]
struct CoinbasePriceResponse {
    data: CoinbasePrice,
}

#[derive(Deserialize)]
struct CoinbasePrice {
    amount: String,
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_expose_stable_identifiers() {
        assert_eq!(Asset::Ethereum.cache_key(), "price:ethereum");
        assert_eq!(Asset::Bnb.coingecko_id(), "binancecoin");
        assert_eq!(Asset::Polygon.cache_key(), "price:polygon");
        assert_eq!(Asset::Polygon.coingecko_id(), "matic-network");
        assert_eq!(Asset::Polygon.symbol(), "POL");
        assert_eq!(Asset::Bitcoin.symbol(), "BTC");
        assert_eq!(Asset::Litecoin.symbol(), "LTC");
        assert_eq!(Asset::Gram.coingecko_id(), "the-open-network");
        assert_eq!(Asset::Gram.symbol(), "GRAM");
    }
}
