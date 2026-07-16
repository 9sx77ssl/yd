use async_trait::async_trait;
use color_eyre::eyre::Result;
use reqwest::Client;
use serde::Deserialize;

use super::model::{NetworkKind, PortfolioEntry, UtxoNetworkConfig};
use super::provider::{format_amount, NetworkProvider};
use crate::net::{shared_client, ApiService, PriceService};

/// A single UTXO-chain provider serving Bitcoin and Litecoin.
///
/// Both networks expose the same Electrum-style `address/{address}` payload;
/// only the API host and asset differ. Collapsing the two former providers
/// into one means a parsing fix lands once, for every UTXO chain.
pub struct UtxoProvider {
    config: UtxoNetworkConfig,
    client: Client,
    prices: PriceService,
}

impl UtxoProvider {
    pub fn new(config: UtxoNetworkConfig, prices: PriceService) -> Self {
        Self {
            config,
            client: shared_client(),
            prices,
        }
    }

    fn address_url(&self, address: &str) -> String {
        self.config.api_url.replace("{address}", address)
    }
}

#[async_trait]
impl NetworkProvider for UtxoProvider {
    fn kind(&self) -> NetworkKind {
        self.config.kind
    }
    fn name(&self) -> &'static str {
        self.config.name
    }
    async fn fetch(&self, address: String) -> Result<PortfolioEntry> {
        let stats = ApiService::new(self.config.name)
            .json::<AddressStats>(self.client.get(self.address_url(&address)))
            .await?;
        let sats = stats
            .chain_stats
            .funded_txo_sum
            .saturating_sub(stats.chain_stats.spent_txo_sum);
        let balance = sats as f64 / 1e8;
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

#[derive(Deserialize)]
struct AddressStats {
    chain_stats: ChainStats,
}

#[derive(Deserialize)]
struct ChainStats {
    funded_txo_sum: u64,
    spent_txo_sum: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Asset;

    #[test]
    fn bitcoin_url_substitutes_address() {
        let provider = UtxoProvider::new(UtxoNetworkConfig::bitcoin(), stub_service());
        assert_eq!(
            provider.address_url("bc1qexample"),
            "https://blockstream.info/api/address/bc1qexample"
        );
    }

    #[test]
    fn litecoin_url_substitutes_address() {
        let provider = UtxoProvider::new(UtxoNetworkConfig::litecoin(), stub_service());
        assert_eq!(
            provider.address_url("Lexample"),
            "https://litecoinspace.org/api/address/Lexample"
        );
    }

    #[test]
    fn utxo_configs_map_to_expected_assets() {
        assert_eq!(UtxoNetworkConfig::bitcoin().asset, Asset::Bitcoin);
        assert_eq!(UtxoNetworkConfig::litecoin().asset, Asset::Litecoin);
        assert_eq!(UtxoNetworkConfig::bitcoin().symbol, "BTC");
        assert_eq!(UtxoNetworkConfig::litecoin().symbol, "LTC");
    }

    fn stub_service() -> PriceService {
        PriceService::new(crate::store::Database::with_path(
            std::env::temp_dir().join(format!("yd-stub-{}.sqlite", std::process::id())),
        ))
    }
}
