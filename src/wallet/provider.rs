use async_trait::async_trait;
use color_eyre::eyre::Result;
use reqwest::{Client, RequestBuilder};
use serde::{de::DeserializeOwned, Deserialize};
use std::{collections::HashMap, sync::Arc};

use crate::error::YdError;

#[derive(Clone, Copy, Debug)]
pub enum NetworkKind {
    Ethereum,
    Bitcoin,
    Litecoin,
}

#[derive(Clone, Copy, Debug)]
enum Asset {
    Ethereum,
    Bitcoin,
    Litecoin,
}

impl Asset {
    fn coingecko_id(self) -> &'static str {
        match self {
            Self::Ethereum => "ethereum",
            Self::Bitcoin => "bitcoin",
            Self::Litecoin => "litecoin",
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::Ethereum => "ETH",
            Self::Bitcoin => "BTC",
            Self::Litecoin => "LTC",
        }
    }
}

pub struct PortfolioEntry {
    pub name: &'static str,
    pub symbol: &'static str,
    pub address: String,
    pub balance: String,
    pub usd_value: Option<f64>,
}

#[async_trait]
pub trait NetworkProvider: Send + Sync {
    fn kind(&self) -> NetworkKind;
    fn name(&self) -> &'static str;
    async fn fetch(&self, address: String) -> Result<PortfolioEntry>;
}

fn client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .user_agent(concat!("yd/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("valid client")
}

#[derive(Clone, Copy, Debug)]
enum ApiService {
    EthereumRpc,
    Blockstream,
    LitecoinSpace,
    CoinGecko,
    Coinbase,
}

impl ApiService {
    const fn name(self) -> &'static str {
        match self {
            Self::EthereumRpc => "Ethereum RPC",
            Self::Blockstream => "Blockstream",
            Self::LitecoinSpace => "Litecoin Space",
            Self::CoinGecko => "CoinGecko",
            Self::Coinbase => "Coinbase",
        }
    }

    async fn json<T>(self, request: RequestBuilder) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = request.send().await.map_err(|source| YdError::ApiRequest {
            service: self.name(),
            source,
        })?;
        let response = response
            .error_for_status()
            .map_err(|source| YdError::ApiRequest {
                service: self.name(),
                source,
            })?;
        response
            .json::<T>()
            .await
            .map_err(|source| YdError::ApiRequest {
                service: self.name(),
                source,
            })
            .map_err(Into::into)
    }

    fn invalid_data(self, detail: impl Into<String>) -> YdError {
        YdError::ApiData {
            service: self.name(),
            detail: detail.into(),
        }
    }
}

#[async_trait]
trait PriceProvider: Send + Sync {
    async fn usd_quote(&self, asset: Asset) -> Result<f64>;
}

struct CoinGeckoPriceProvider {
    client: Client,
}

#[async_trait]
impl PriceProvider for CoinGeckoPriceProvider {
    async fn usd_quote(&self, asset: Asset) -> Result<f64> {
        let prices = ApiService::CoinGecko
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
                ApiService::CoinGecko
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
        let response = ApiService::Coinbase
            .json::<CoinbasePriceResponse>(self.client.get(format!(
                "https://api.coinbase.com/v2/prices/{}-USD/spot",
                asset.symbol()
            )))
            .await?;
        response.data.amount.parse::<f64>().map_err(|_| {
            ApiService::Coinbase
                .invalid_data(format!("invalid {} quote amount", asset.symbol()))
                .into()
        })
    }
}

#[derive(Clone)]
pub(crate) struct PriceService {
    primary: Arc<dyn PriceProvider>,
    fallback: Arc<dyn PriceProvider>,
}

impl PriceService {
    pub(crate) fn new() -> Self {
        Self {
            primary: Arc::new(CoinGeckoPriceProvider { client: client() }),
            fallback: Arc::new(CoinbasePriceProvider { client: client() }),
        }
    }

    async fn usd_quote(&self, asset: Asset) -> Option<f64> {
        let (primary, fallback) = tokio::join!(
            self.primary.usd_quote(asset),
            self.fallback.usd_quote(asset)
        );
        match (primary, fallback) {
            (Ok(price), _) => Some(price),
            (Err(primary_error), Ok(price)) => {
                tracing::debug!(%primary_error, asset = asset.symbol(), "using fallback USD quote");
                Some(price)
            }
            (Err(primary_error), Err(fallback_error)) => {
                tracing::debug!(%primary_error, %fallback_error, asset = asset.symbol(), "USD quote providers unavailable");
                None
            }
        }
    }
}

pub struct EthereumProvider {
    client: Client,
    prices: PriceService,
}
impl EthereumProvider {
    pub fn new(prices: PriceService) -> Self {
        Self {
            client: client(),
            prices,
        }
    }
}
#[async_trait]
impl NetworkProvider for EthereumProvider {
    fn kind(&self) -> NetworkKind {
        NetworkKind::Ethereum
    }
    fn name(&self) -> &'static str {
        "Ethereum"
    }
    async fn fetch(&self, address: String) -> Result<PortfolioEntry> {
        let rpc = ApiService::EthereumRpc
            .json::<RpcResponse>(self.client.post("https://eth.drpc.org").json(
                &serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "eth_getBalance",
                    "params": [address, "latest"],
                }),
            ))
            .await?;
        let wei = u128::from_str_radix(rpc.result.trim_start_matches("0x"), 16)
            .map_err(|_| ApiService::EthereumRpc.invalid_data("invalid hex balance"))?;
        let balance = wei as f64 / 1e18;
        let price = self.prices.usd_quote(Asset::Ethereum).await;
        Ok(PortfolioEntry {
            name: self.name(),
            symbol: "ETH",
            address,
            balance: format_amount(balance, 8),
            usd_value: price.map(|p| p * balance),
        })
    }
}

pub struct BitcoinProvider {
    client: Client,
    prices: PriceService,
}
impl BitcoinProvider {
    pub fn new(prices: PriceService) -> Self {
        Self {
            client: client(),
            prices,
        }
    }
}
#[async_trait]
impl NetworkProvider for BitcoinProvider {
    fn kind(&self) -> NetworkKind {
        NetworkKind::Bitcoin
    }
    fn name(&self) -> &'static str {
        "Bitcoin"
    }
    async fn fetch(&self, address: String) -> Result<PortfolioEntry> {
        let stats = ApiService::Blockstream
            .json::<AddressStats>(
                self.client
                    .get(format!("https://blockstream.info/api/address/{address}")),
            )
            .await?;
        let sats = stats
            .chain_stats
            .funded_txo_sum
            .saturating_sub(stats.chain_stats.spent_txo_sum);
        let balance = sats as f64 / 1e8;
        let price = self.prices.usd_quote(Asset::Bitcoin).await;
        Ok(PortfolioEntry {
            name: self.name(),
            symbol: "BTC",
            address,
            balance: format_amount(balance, 8),
            usd_value: price.map(|p| p * balance),
        })
    }
}

pub struct LitecoinProvider {
    client: Client,
    prices: PriceService,
}
impl LitecoinProvider {
    pub fn new(prices: PriceService) -> Self {
        Self {
            client: client(),
            prices,
        }
    }
}
#[async_trait]
impl NetworkProvider for LitecoinProvider {
    fn kind(&self) -> NetworkKind {
        NetworkKind::Litecoin
    }
    fn name(&self) -> &'static str {
        "Litecoin"
    }
    async fn fetch(&self, address: String) -> Result<PortfolioEntry> {
        let stats = ApiService::LitecoinSpace
            .json::<AddressStats>(
                self.client
                    .get(format!("https://litecoinspace.org/api/address/{address}")),
            )
            .await?;
        let sats = stats
            .chain_stats
            .funded_txo_sum
            .saturating_sub(stats.chain_stats.spent_txo_sum);
        let balance = sats as f64 / 1e8;
        let price = self.prices.usd_quote(Asset::Litecoin).await;
        Ok(PortfolioEntry {
            name: self.name(),
            symbol: "LTC",
            address,
            balance: format_amount(balance, 8),
            usd_value: price.map(|p| p * balance),
        })
    }
}

#[derive(Deserialize)]
struct RpcResponse {
    result: String,
}
#[derive(Deserialize)]
struct AddressStats {
    chain_stats: ChainStats,
}
#[derive(Deserialize)]
struct ChainStats {
    funded_txo_sum: u64,
    spent_txo_sum: u64,
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

fn format_amount(value: f64, decimals: usize) -> String {
    let raw = format!("{value:.decimals$}");
    raw.trim_end_matches('0').trim_end_matches('.').to_owned()
}
