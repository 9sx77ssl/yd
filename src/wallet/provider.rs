use async_trait::async_trait;
use color_eyre::eyre::Result;
use std::sync::Arc;

use super::chain::UtxoProvider;
use super::evm::EvmProvider;
use super::model::{EvmNetworkConfig, NetworkKind, PortfolioEntry, UtxoNetworkConfig};
use crate::net::PriceService;

/// One derivable address family the wallet can render.
///
/// Implementations live next to their chain logic ([`EvmProvider`],
/// [`UtxoProvider`]) and are assembled by [`wallet_providers`].
#[async_trait]
pub trait NetworkProvider: Send + Sync {
    fn kind(&self) -> NetworkKind;
    fn name(&self) -> &'static str;
    async fn fetch(&self, address: String) -> Result<PortfolioEntry>;
}

/// Builds the full provider set for a wallet.
///
/// Order is preserved in the rendered portfolio. Add a chain by appending an
/// instance here; no other call site changes.
pub fn wallet_providers(prices: PriceService) -> Vec<Arc<dyn NetworkProvider>> {
    vec![
        Arc::new(EvmProvider::new(
            EvmNetworkConfig::ethereum(),
            prices.clone(),
        )),
        Arc::new(EvmProvider::new(
            EvmNetworkConfig::bnb_chain(),
            prices.clone(),
        )),
        Arc::new(UtxoProvider::new(
            UtxoNetworkConfig::bitcoin(),
            prices.clone(),
        )),
        Arc::new(UtxoProvider::new(UtxoNetworkConfig::litecoin(), prices)),
    ]
}

/// Trims trailing zeros from a fixed-precision amount for display.
pub fn format_amount(value: f64, decimals: usize) -> String {
    let raw = format!("{value:.decimals$}");
    raw.trim_end_matches('0').trim_end_matches('.').to_owned()
}
