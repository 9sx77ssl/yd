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

const SLIP0010_KEY: &[u8] = b"ed25519 seed";
const SOLANA_COIN: u32 = 501;

/// Hardened derivation: HMAC-SHA512(key=chain, 0x00 || secret || index)
fn hardened(parent: &[u8; 32], index: u32) -> Result<([u8; 32], [u8; 32])> {
    let mut mac = Hmac::<Sha512>::new_from_slice(parent).expect("HMAC key");
    mac.update(&[0x00]);
    mac.update(parent);
    mac.update(&(index | 0x80000000).to_be_bytes());
    let r = mac.finalize().into_bytes();
    Ok((r[..32].try_into().unwrap(), r[32..].try_into().unwrap()))
}

/// Non-hardened (Icarus/BIP32-Ed25519): HMAC-SHA512(key=chain, 0x02 || pubkey || index)
fn soft(
    parent_secret: &[u8; 32],
    parent_chain: &[u8; 32],
    index: u32,
) -> Result<([u8; 32], [u8; 32])> {
    let pub_key = SigningKey::from_bytes(parent_secret).verifying_key();
    let mut mac = Hmac::<Sha512>::new_from_slice(parent_chain).expect("HMAC key");
    mac.update(&[0x02]);
    mac.update(pub_key.as_bytes());
    mac.update(&index.to_be_bytes());
    let r = mac.finalize().into_bytes();
    Ok((r[..32].try_into().unwrap(), r[32..].try_into().unwrap()))
}

fn pubkey_b58(key: &[u8; 32]) -> String {
    let pub_key = SigningKey::from_bytes(key).verifying_key();
    bs58::encode(pub_key.as_bytes()).into_string()
}

/// Multi-strategy Solana address detector.
///
/// Scans every known derivation method across multiple account indices.
/// The first address with a non-zero SOL balance wins.
pub(crate) fn derive_solana_candidates(seed: &[u8; 64]) -> Result<Vec<String>> {
    let mut candidates = Vec::new();

    let mut mac = Hmac::<Sha512>::new_from_slice(SLIP0010_KEY).expect("HMAC key");
    mac.update(seed);
    let master_full = mac.finalize().into_bytes();
    let master_secret: [u8; 32] = master_full[..32].try_into().unwrap();
    let master_chain: [u8; 32] = master_full[32..].try_into().unwrap();

    // ── Exodus: m/0/0 (non-hardened, Icarus method) ────────────
    let (s0, c0) = soft(&master_secret, &master_chain, 0)?;
    candidates.push(pubkey_b58(&s0));
    let (s00, _c00) = soft(&s0, &c0, 0)?;
    candidates.push(pubkey_b58(&s00));

    // ── Exodus variants: m/0'/0', m/0'/i' (hardened) ───────────
    let (h0, _c_h0) = hardened(&master_secret, 0)?;
    candidates.push(pubkey_b58(&h0));
    for i in 1..5 {
        let (hi, _) = hardened(&h0, i)?;
        candidates.push(pubkey_b58(&hi));
    }

    // ── BIP-44: m/44'/501'/i'/0' (Phantom, SafePal, Solana CLI)
    let (purpose_s, _purpose_c) = hardened(&master_secret, 44)?;
    for i in 0..10 {
        let (acct_s, _acct_c) = hardened(&purpose_s, SOLANA_COIN)?;
        let (change_s, _) = hardened(&acct_s, i)?;
        let (key_s, _) = hardened(&change_s, 0)?;
        candidates.push(pubkey_b58(&key_s));
    }

    // ── BIP-44 short: m/44'/501'/i' (Trust Wallet) ─────────────
    for i in 0..10 {
        let (acct_s, _) = hardened(&purpose_s, SOLANA_COIN)?;
        let (key_s, _) = hardened(&acct_s, i)?;
        candidates.push(pubkey_b58(&key_s));
    }

    // ── BIP-44 deep: m/44'/501'/i'/0'/0' (Solana CLI legacy) ──
    for i in 0..5 {
        let (acct_s, _) = hardened(&purpose_s, SOLANA_COIN)?;
        let (change_s, _) = hardened(&acct_s, i)?;
        let (deep_s, _) = hardened(&change_s, 0)?;
        let (key_s, _) = hardened(&deep_s, 0)?;
        candidates.push(pubkey_b58(&key_s));
    }

    // ── BIP-44 change: m/44'/501'/i'/1' ─────────────────────────
    for i in 0..5 {
        let (acct_s, _) = hardened(&purpose_s, SOLANA_COIN)?;
        let (change_s, _) = hardened(&acct_s, i)?;
        let (key_s, _) = hardened(&change_s, 1)?;
        candidates.push(pubkey_b58(&key_s));
    }

    // ── Bare: m/44'/501/0 ───────────────────────────────────────
    let (bare, _) = hardened(&purpose_s, SOLANA_COIN)?;
    candidates.push(pubkey_b58(&bare));

    // ── Raw seed approaches ─────────────────────────────────────
    let raw0: [u8; 32] = seed[..32].try_into()?;
    candidates.push(pubkey_b58(&raw0));
    let raw1: [u8; 32] = seed[32..64].try_into()?;
    candidates.push(pubkey_b58(&raw1));
    candidates.push(pubkey_b58(&master_secret));

    Ok(candidates)
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
        let balances = self.fetch_balances_batch(&candidates).await;

        let active = candidates
            .iter()
            .zip(balances.iter())
            .find(|(_, bal)| **bal > 0)
            .map(|(addr, _)| addr.clone())
            .unwrap_or_else(|| candidates[0].clone());

        Ok(self.cached_address.get_or_init(|| active).clone())
    }

    async fn fetch_balances_batch(&self, addresses: &[String]) -> Vec<u64> {
        let jobs = addresses.iter().map(|address| {
            let client = self.client.clone();
            let rpc_urls = self.config.rpc_urls;
            let address = address.clone();
            async move {
                for rpc_url in rpc_urls {
                    if let Ok(lamports) =
                        Self::fetch_lamports_from(&client, rpc_url, &address).await
                    {
                        return lamports;
                    }
                }
                0
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
    fn all_candidates_are_unique_and_valid() {
        let seed = test_seed();
        let candidates = derive_solana_candidates(&seed).unwrap();
        let mut seen = std::collections::HashSet::new();
        for addr in &candidates {
            let decoded = bs58::decode(addr).into_vec().unwrap();
            assert_eq!(decoded.len(), 32, "public key must be 32 bytes: {addr}");
            assert!(seen.insert(addr.clone()), "duplicate: {addr}");
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
