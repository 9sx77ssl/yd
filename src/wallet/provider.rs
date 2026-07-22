use async_trait::async_trait;
use color_eyre::eyre::Result;
use std::sync::Arc;

use super::chain::UtxoProvider;
use super::evm::EvmProvider;
use super::model::{
    EvmNetworkConfig, NetworkKind, PortfolioEntry, TonNetworkConfig, UtxoNetworkConfig,
};
use super::ton::TonProvider;
use crate::net::PriceService;

/// One derivable address family the wallet can render.
///
/// Implementations live next to their chain logic ([`EvmProvider`],
/// [`UtxoProvider`], [`TonProvider`]) and are assembled by
/// [`wallet_providers`].
#[async_trait]
pub trait NetworkProvider: Send + Sync {
    fn kind(&self) -> NetworkKind;
    fn name(&self) -> &'static str;
    async fn fetch(&self, address: String) -> Result<PortfolioEntry>;

    /// Returns all addresses with balances (for multi-wallet chains like TON).
    /// Default: single entry from fetch().
    async fn fetch_all(&self) -> Result<Vec<PortfolioEntry>> {
        let entry = self.fetch(String::new()).await?;
        Ok(vec![entry])
    }
}

/// Builds the full provider set for a wallet.
///
/// Order is preserved in the rendered portfolio. Add a chain by appending an
/// instance here; no other call site changes.
pub fn wallet_providers(prices: PriceService, seed: [u8; 64]) -> Vec<Arc<dyn NetworkProvider>> {
    vec![
        Arc::new(EvmProvider::new(
            EvmNetworkConfig::ethereum(),
            prices.clone(),
        )),
        Arc::new(EvmProvider::new(
            EvmNetworkConfig::bnb_chain(),
            prices.clone(),
        )),
        Arc::new(EvmProvider::new(
            EvmNetworkConfig::polygon(),
            prices.clone(),
        )),
        Arc::new(UtxoProvider::new(
            UtxoNetworkConfig::bitcoin(),
            prices.clone(),
        )),
        Arc::new(UtxoProvider::new(
            UtxoNetworkConfig::litecoin(),
            prices.clone(),
        )),
        Arc::new(TonProvider::new(TonNetworkConfig::mainnet(), prices, seed)),
    ]
}

/// Trims trailing zeros from a fixed-precision amount for display.
pub fn format_amount(value: f64, decimals: usize) -> String {
    let raw = format!("{value:.decimals$}");
    raw.trim_end_matches('0').trim_end_matches('.').to_owned()
}
