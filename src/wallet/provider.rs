use async_trait::async_trait;
use color_eyre::eyre::Result;
use reqwest::Client;
use serde::Deserialize;

use crate::error::YdError;

#[derive(Clone, Copy, Debug)]
pub enum NetworkKind {
    Ethereum,
    Bitcoin,
    Litecoin,
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
        .user_agent("yd/0.1")
        .build()
        .expect("valid client")
}

pub struct EthereumProvider {
    client: Client,
}
impl EthereumProvider {
    pub fn new() -> Self {
        Self { client: client() }
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
        let rpc = self.client.post("https://eth.drpc.org").json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"eth_getBalance","params":[address,"latest"]})).send().await
            .map_err(|_| YdError::NetworkUnavailable { network: self.name().into() })?.json::<RpcResponse>().await?;
        let wei = u128::from_str_radix(rpc.result.trim_start_matches("0x"), 16).map_err(|_| {
            YdError::NetworkUnavailable {
                network: self.name().into(),
            }
        })?;
        let balance = wei as f64 / 1e18;
        let price = price(&self.client, "ethereum").await?;
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
}
impl BitcoinProvider {
    pub fn new() -> Self {
        Self { client: client() }
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
        let stats = self
            .client
            .get(format!("https://blockstream.info/api/address/{address}"))
            .send()
            .await
            .map_err(|_| YdError::NetworkUnavailable {
                network: self.name().into(),
            })?
            .json::<AddressStats>()
            .await?;
        let sats = stats
            .chain_stats
            .funded_txo_sum
            .saturating_sub(stats.chain_stats.spent_txo_sum);
        let balance = sats as f64 / 1e8;
        let price = price(&self.client, "bitcoin").await?;
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
}
impl LitecoinProvider {
    pub fn new() -> Self {
        Self { client: client() }
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
        let stats = self
            .client
            .get(format!("https://litecoinspace.org/api/address/{address}"))
            .send()
            .await
            .map_err(|_| YdError::NetworkUnavailable {
                network: self.name().into(),
            })?
            .json::<AddressStats>()
            .await?;
        let sats = stats
            .chain_stats
            .funded_txo_sum
            .saturating_sub(stats.chain_stats.spent_txo_sum);
        let balance = sats as f64 / 1e8;
        let price = price(&self.client, "litecoin").await?;
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
struct PriceResponse {
    #[serde(rename = "usd")]
    usd: f64,
}

async fn price(client: &Client, coin: &str) -> Result<Option<f64>> {
    let response = client
        .get("https://api.coingecko.com/api/v3/simple/price")
        .query(&[("ids", coin), ("vs_currencies", "usd")])
        .send()
        .await;
    let Ok(response) = response else {
        return Ok(None);
    };
    let prices = response
        .json::<std::collections::HashMap<String, PriceResponse>>()
        .await
        .unwrap_or_default();
    Ok(prices.get(coin).map(|price| price.usd))
}

fn format_amount(value: f64, decimals: usize) -> String {
    let raw = format!("{value:.decimals$}");
    raw.trim_end_matches('0').trim_end_matches('.').to_owned()
}
