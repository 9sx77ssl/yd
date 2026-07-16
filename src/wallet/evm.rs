use async_trait::async_trait;
use color_eyre::eyre::Result;
use reqwest::Client;
use serde::Deserialize;

use super::model::{EvmNetworkConfig, NetworkKind, PortfolioEntry};
use super::provider::{format_amount, NetworkProvider};
use crate::net::{shared_client, ApiService, PriceService};

/// A single EVM provider that serves every EVM-compatible chain.
///
/// Add a chain by authoring an [`EvmNetworkConfig`] const and registering an
/// instance here; no per-chain provider code is needed because balance is the
/// standard `eth_getBalance` JSON-RPC call and the asset quote flows through
/// the shared [`PriceService`].
pub struct EvmProvider {
    config: EvmNetworkConfig,
    client: Client,
    prices: PriceService,
}

impl EvmProvider {
    pub fn new(config: EvmNetworkConfig, prices: PriceService) -> Self {
        Self {
            config,
            client: shared_client(),
            prices,
        }
    }
}

#[async_trait]
impl NetworkProvider for EvmProvider {
    fn kind(&self) -> NetworkKind {
        self.config.kind
    }
    fn name(&self) -> &'static str {
        self.config.name
    }
    async fn fetch(&self, address: String) -> Result<PortfolioEntry> {
        let wei = self.fetch_balance_wei(&address).await?;
        let balance = wei as f64 / 1e18;
        let price = self.prices.usd_quote(self.config.asset).await;
        Ok(PortfolioEntry {
            name: self.name(),
            symbol: self.config.symbol,
            address,
            balance: format_amount(balance, 8),
            usd_value: price.map(|p| p * balance),
        })
    }
}

impl EvmProvider {
    async fn fetch_balance_wei(&self, address: &str) -> Result<u128> {
        let mut last_error = None;

        for rpc_url in self.config.rpc_urls {
            match self.fetch_balance_wei_from(rpc_url, address).await {
                Ok(wei) => return Ok(wei),
                Err(error) => {
                    tracing::debug!(
                        %error,
                        network = self.config.name,
                        rpc_url,
                        "EVM RPC endpoint unavailable"
                    );
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            ApiService::new(self.config.name)
                .invalid_data("no RPC endpoints configured")
                .into()
        }))
    }

    async fn fetch_balance_wei_from(&self, rpc_url: &str, address: &str) -> Result<u128> {
        let rpc = ApiService::new(self.config.name)
            .json::<RpcResponse>(self.client.post(rpc_url).json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_getBalance",
                "params": [address, "latest"],
            })))
            .await?;
        u128::from_str_radix(rpc.result.trim_start_matches("0x"), 16).map_err(|_| {
            ApiService::new(self.config.name)
                .invalid_data("invalid hex balance")
                .into()
        })
    }
}

#[derive(Deserialize)]
struct RpcResponse {
    result: String,
}
