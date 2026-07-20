use async_trait::async_trait;
use color_eyre::eyre::Result;
use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::Deserialize;
use sha2::Sha512;
use std::sync::OnceLock;

use super::model::{NetworkKind, PortfolioEntry, SolanaNetworkConfig};
use super::provider::{format_amount, NetworkProvider};
use crate::net::{shared_client, ApiService, PriceService};

/// Number of account indices to scan per derivation method.
const ACCOUNT_SCAN_COUNT: u32 = 5;
const SLIP0010_ED25519_KEY: &[u8] = b"ed25519 seed";
const SOLANA_COIN_TYPE: u32 = 501;

/// Generates all candidate addresses from three derivation methods:
///
/// 1. **Exodus**: `m/44'/501'/i'/0/0` — BIP-32 Ed25519 (non-hardened tail)
/// 2. **Phantom/SafePal**: `m/44'/501'/i'/0'` — SLIP-0010 (all hardened)
/// 3. **SafePal optional**: `m/44'/501'/i'` — shorter SLIP-0010
///
/// Returns `(address, method_label)` pairs for batch balance checking.
pub(crate) fn derive_solana_candidates(seed: &[u8; 64]) -> Result<Vec<(String, &'static str)>> {
    let mut candidates = Vec::new();
    let master = slip0010_derive_master(seed)?;
    let coin = slip0010_derive_child(&master, 44 | 0x80000000)?;

    for i in 0..ACCOUNT_SCAN_COUNT {
        let hardened_account = slip0010_derive_child(&coin, SOLANA_COIN_TYPE | 0x80000000)?;
        let account = slip0010_derive_child(&hardened_account, i | 0x80000000)?;

        // Method 1: Exodus — m/44'/501'/i'/0/0
        let exodus_addr = {
            let change = bip32_derive_child(&account, 0)?;
            let key = bip32_derive_child(&change, 0)?;
            key_to_address(&key)?
        };
        candidates.push((exodus_addr, "exodus"));

        // Method 2: Phantom/SafePal — m/44'/501'/i'/0'
        let phantom_addr = {
            let change = slip0010_derive_child(&account, 0x80000000)?;
            key_to_address(&change)?
        };
        candidates.push((phantom_addr, "phantom"));

        // Method 3: SafePal optional — m/44'/501'/i'
        let safepal_addr = key_to_address(&account)?;
        candidates.push((safepal_addr, "safepal"));
    }

    Ok(candidates)
}

fn key_to_address(key: &[u8; 32]) -> Result<String> {
    let public = SigningKey::from_bytes(key).verifying_key();
    Ok(bs58::encode(public.as_bytes()).into_string())
}

fn slip0010_derive_master(seed: &[u8; 64]) -> Result<[u8; 32]> {
    let mut mac =
        Hmac::<Sha512>::new_from_slice(SLIP0010_ED25519_KEY).expect("HMAC accepts any key length");
    mac.update(seed);
    let result = mac.finalize().into_bytes();
    Ok(result[..32].try_into().expect("slice is 32 bytes"))
}

fn slip0010_derive_child(parent_key: &[u8; 32], index: u32) -> Result<[u8; 32]> {
    let mut mac = Hmac::<Sha512>::new_from_slice(parent_key).expect("HMAC accepts any key length");
    mac.update(&[0x00]);
    mac.update(parent_key);
    mac.update(&index.to_be_bytes());
    let result = mac.finalize().into_bytes();
    Ok(result[..32].try_into().expect("slice is 32 bytes"))
}

fn bip32_derive_child(parent_key: &[u8; 32], index: u32) -> Result<[u8; 32]> {
    let signing_key = SigningKey::from_bytes(parent_key);
    let verifying_key = signing_key.verifying_key();
    let public_bytes = verifying_key.as_bytes();
    let mut mac = Hmac::<Sha512>::new_from_slice(parent_key).expect("HMAC accepts any key length");
    mac.update(public_bytes);
    mac.update(&index.to_be_bytes());
    let result = mac.finalize().into_bytes();
    Ok(result[..32].try_into().expect("slice is 32 bytes"))
}

pub struct SolanaProvider {
    config: SolanaNetworkConfig,
    client: Client,
    prices: PriceService,
    seed: [u8; 64],
    cached_address: OnceLock<String>,
}

impl SolanaProvider {
    pub fn new(config: SolanaNetworkConfig, prices: PriceService, seed: [u8; 64]) -> Self {
        Self {
            config,
            client: shared_client(),
            prices,
            seed,
            cached_address: OnceLock::new(),
        }
    }

    async fn resolve_address(&self) -> Result<String> {
        if let Some(address) = self.cached_address.get() {
            return Ok(address.clone());
        }

        let candidates = derive_solana_candidates(&self.seed)?;
        let addresses: Vec<String> = candidates.iter().map(|(addr, _)| addr.clone()).collect();
        let balances = self.fetch_balances_batch(&addresses).await;

        let active = candidates
            .iter()
            .zip(balances.iter())
            .find(|(_, balance)| **balance > 0)
            .map(|((address, _), _)| address.clone())
            .unwrap_or_else(|| {
                candidates
                    .first()
                    .map(|(addr, _)| addr.clone())
                    .expect("at least one candidate")
            });

        Ok(self.cached_address.get_or_init(|| active).clone())
    }

    async fn fetch_balances_batch(&self, addresses: &[String]) -> Vec<u64> {
        let jobs = addresses.iter().map(|address| {
            let client = self.client.clone();
            let rpc_url = self.config.rpc_urls[0].to_owned();
            let address = address.clone();
            async move {
                Self::fetch_lamports_from(&client, &rpc_url, &address)
                    .await
                    .unwrap_or(0)
            }
        });
        futures::future::join_all(jobs).await
    }

    async fn fetch_lamports_from(client: &Client, rpc_url: &str, address: &str) -> Result<u64> {
        let rpc = ApiService::new("Solana")
            .json::<RpcResponse>(client.post(rpc_url).json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getBalance",
                "params": [address],
            })))
            .await?;
        Ok(rpc.result.value)
    }

    async fn fetch_balance_lamports(&self, address: &str) -> Result<u64> {
        let mut last_error = None;

        for rpc_url in self.config.rpc_urls {
            match Self::fetch_lamports_from(&self.client, rpc_url, address).await {
                Ok(lamports) => return Ok(lamports),
                Err(error) => {
                    tracing::debug!(
                        %error,
                        network = self.config.name,
                        rpc_url,
                        "Solana RPC endpoint unavailable"
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
}

#[async_trait]
impl NetworkProvider for SolanaProvider {
    fn kind(&self) -> NetworkKind {
        self.config.kind
    }

    fn name(&self) -> &'static str {
        self.config.name
    }

    async fn fetch(&self, _address: String) -> Result<PortfolioEntry> {
        let address = self.resolve_address().await?;
        let lamports = self.fetch_balance_lamports(&address).await?;
        let balance = lamports as f64 / 1e9;
        let price = self.prices.usd_quote(self.config.asset).await;
        Ok(PortfolioEntry {
            name: self.name(),
            symbol: self.config.symbol,
            address,
            balance: format_amount(balance, 9),
            usd_value: price.map(|p| p * balance),
        })
    }
}

#[derive(Deserialize)]
struct RpcResponse {
    result: RpcBalance,
}

#[derive(Deserialize)]
struct RpcBalance {
    value: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Asset;

    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn test_seed() -> [u8; 64] {
        let mnemonic = bip39::Mnemonic::parse(TEST_MNEMONIC).unwrap();
        let seed = mnemonic.to_seed("");
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&seed);
        bytes
    }

    #[test]
    fn all_derivation_methods_produce_unique_addresses() {
        let seed = test_seed();
        let candidates = derive_solana_candidates(&seed).unwrap();
        let mut seen = std::collections::HashSet::new();
        for (addr, method) in &candidates {
            assert!(
                seen.insert(addr.clone()),
                "duplicate address from {method}: {addr}"
            );
        }
        assert_eq!(candidates.len(), 15);
    }

    #[test]
    fn solana_addresses_are_valid_base58() {
        let seed = test_seed();
        let candidates = derive_solana_candidates(&seed).unwrap();
        for (address, _method) in &candidates {
            let decoded = bs58::decode(address).into_vec().unwrap();
            assert_eq!(decoded.len(), 32, "public key must be 32 bytes");
        }
    }

    #[test]
    fn solana_config_exposes_expected_fields() {
        let config = SolanaNetworkConfig::mainnet();
        assert_eq!(config.name, "Solana");
        assert_eq!(config.symbol, "SOL");
        assert_eq!(config.asset, Asset::Solana);
        assert!(!config.rpc_urls.is_empty());
    }
}
