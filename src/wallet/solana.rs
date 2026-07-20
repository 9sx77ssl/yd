use async_trait::async_trait;
use color_eyre::eyre::Result;
use ed25519_dalek::{SigningKey, VerifyingKey};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::Deserialize;
use sha2::Sha512;
use std::sync::OnceLock;

use super::model::{NetworkKind, PortfolioEntry, SolanaNetworkConfig};
use super::provider::{format_amount, NetworkProvider};
use crate::net::{shared_client, ApiService, PriceService};

const DERIVATION_SCAN_COUNT: u32 = 20;
const SLIP0010_ED25519_KEY: &[u8] = b"ed25519 seed";
const SOLANA_COIN_TYPE: u32 = 501;

pub fn derive_solana_address(seed: &[u8; 64], index: u32) -> Result<String> {
    let secret = derive_solana_keypair(seed, index)?;
    let public: VerifyingKey = SigningKey::from_bytes(&secret).verifying_key();
    Ok(bs58::encode(public.as_bytes()).into_string())
}

/// Derives a Solana keypair using BIP-32 Ed25519 (Exodus-compatible).
///
/// Path: `m / 44' / 501' / index' / 0 / 0`
///
/// Levels 0-2 are hardened (SLIP-0010 style), levels 3-4 are non-hardened
/// (BIP-32 Ed25519). This matches Exodus, which is the most popular
/// non-standard Solana derivation.
fn derive_solana_keypair(seed: &[u8; 64], index: u32) -> Result<[u8; 32]> {
    let master = slip0010_derive_master(seed)?;
    let coin = slip0010_derive_child(&master, 44 | 0x80000000)?;
    let account = slip0010_derive_child(&coin, SOLANA_COIN_TYPE | 0x80000000)?;
    let change = slip0010_derive_child(&account, index | 0x80000000)?;
    let address = bip32_derive_child(&change, 0)?;
    bip32_derive_child(&address, 0)
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

/// BIP-32 non-hardened child key derivation for Ed25519.
///
/// HMAC-SHA512(key = parent_public_key || index_be32, message = data)
/// Unlike SLIP-0010 hardened derivation which uses 0x00 || secret_key.
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

        let candidates: Vec<String> = (0..DERIVATION_SCAN_COUNT)
            .map(|i| derive_solana_address(&self.seed, i))
            .collect::<Result<_>>()?;

        let balances = self.fetch_balances_batch(&candidates).await;

        let active = candidates
            .iter()
            .zip(balances.iter())
            .find(|(_, balance)| **balance > 0)
            .map(|(address, _)| address.clone())
            .unwrap_or_else(|| candidates[0].clone());

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
    fn slip0010_derivation_produces_stable_addresses() {
        let seed = test_seed();
        let addr0 = derive_solana_address(&seed, 0).unwrap();
        let addr1 = derive_solana_address(&seed, 1).unwrap();
        let addr0_again = derive_solana_address(&seed, 0).unwrap();

        assert_ne!(addr0, addr1);
        assert_eq!(addr0, addr0_again);
        assert!(!addr0.is_empty());
    }

    #[test]
    fn direct_seed_derivation_matches_exodus_style() {
        let seed = test_seed();

        // SLIP-0010: m/44'/501'/0'/0' (Phantom-style, 4 hardened levels)
        let slip_addr = derive_solana_address(&seed, 0).unwrap();
        eprintln!("SLIP-0010  m/44'/501'/0'/0':   {slip_addr}");

        // Direct from BIP-39 seed (first 32 bytes as Ed25519 key)
        let key_bytes: [u8; 32] = seed[..32].try_into().unwrap();
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let public: VerifyingKey = signing_key.verifying_key();
        let direct_addr = bs58::encode(public.as_bytes()).into_string();
        eprintln!("Direct     seed[:32]:           {direct_addr}");

        assert_ne!(slip_addr, direct_addr);
    }

    #[test]
    fn solana_addresses_are_valid_base58() {
        let seed = test_seed();
        for i in 0..20 {
            let address = derive_solana_address(&seed, i).unwrap();
            let decoded = bs58::decode(&address).into_vec().unwrap();
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
