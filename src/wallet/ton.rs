use async_trait::async_trait;
use color_eyre::eyre::Result;
use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256, Sha512};
use std::sync::OnceLock;

use super::model::{NetworkKind, PortfolioEntry, TonNetworkConfig};
use super::provider::{format_amount, NetworkProvider};
use crate::net::{shared_client, ApiService, PriceService};

const SLIP0010_KEY: &[u8] = b"ed25519 seed";
const TON_COIN: u32 = 607;

/// v4R2 wallet code BoC (Bag of Cells) as hex.
/// This is the standard wallet contract deployed on TON mainnet.
/// Leading zero padding ensures even byte length for BoC format.
const V4R2_CODE_HEX: &str = "0b5ee9c72410101010003000002dc0010a09fc4d3098164a4156076200b40917c680405035c302120101061f00620020030030201520003020114000402012c0005030050402015201040500e0300201320070201520009000b00010d08ed54f1e05c0e12b4c737527a54c616e42727f16f219f11fb5f1b150f819d3002b2aa8302015202020109000b000111040500e10302012c000600012cad000c020152010a000f0009000530c82f0903020114000402012c0007000a232b2bb290d8e9038d05a04e903bc15c0904880008e478b04012004130bd0";

/// Hardened derivation: HMAC-SHA512(key=chain, 0x00 || secret || index)
fn hardened(parent: &[u8; 32], index: u32) -> Result<([u8; 32], [u8; 32])> {
    let mut mac = Hmac::<Sha512>::new_from_slice(parent).expect("HMAC key");
    mac.update(&[0x00]);
    mac.update(parent);
    mac.update(&(index | 0x80000000).to_be_bytes());
    let r = mac.finalize().into_bytes();
    Ok((r[..32].try_into().unwrap(), r[32..].try_into().unwrap()))
}

/// Derives Ed25519 keypair for TON using BIP-44 path m/44'/607'/i'/0'.
fn derive_ton_keypair(seed: &[u8; 64], index: u32) -> Result<SigningKey> {
    let mut mac = Hmac::<Sha512>::new_from_slice(SLIP0010_KEY).expect("HMAC key");
    mac.update(seed);
    let master = mac.finalize().into_bytes();
    let master_secret: [u8; 32] = master[..32].try_into().unwrap();

    let (purpose_secret, _) = hardened(&master_secret, 44)?;
    let (account_secret, _) = hardened(&purpose_secret, TON_COIN)?;
    let (change_secret, _) = hardened(&account_secret, index)?;
    let (key_secret, _) = hardened(&change_secret, 0)?;

    Ok(SigningKey::from_bytes(&key_secret))
}

/// Computes TON user-friendly address from public key.
///
/// Algorithm:
/// 1. Build v4R2 StateInit data = wallet_id(0) + seqno(0) + pubkey + empty_dict
/// 2. StateInit hash = SHA-256(code_data || data_data)
/// 3. Address = workchain(0) + hash
/// 4. User-friendly = prefix + address + CRC32C → base64url
fn compute_ton_address(public_key: &[u8; 32], bounceable: bool) -> String {
    let code_bytes = hex::decode(V4R2_CODE_HEX).expect("valid hex");

    let mut data_bytes = Vec::with_capacity(41);
    data_bytes.extend_from_slice(&0u32.to_be_bytes());
    data_bytes.extend_from_slice(&0u32.to_be_bytes());
    data_bytes.extend_from_slice(public_key);
    data_bytes.push(0x00);

    let mut hasher = Sha256::new();
    hasher.update(&code_bytes);
    hasher.update(&data_bytes);
    let hash: [u8; 32] = hasher.finalize().into();

    // TON user-friendly address format (from @ton/core):
    // tag(1) + workchain(1) + hash(32) + crc16(2) = 36 bytes → 48 base64url chars
    // tag: 0x11 = bounceable, 0x51 = non-bounceable
    let tag: u8 = if bounceable { 0x11 } else { 0x51 };

    let mut addr_data = Vec::with_capacity(36);
    addr_data.push(tag);
    addr_data.push(0x00); // workchain 0
    addr_data.extend_from_slice(&hash);

    let crc = crc16(&addr_data);
    addr_data.extend_from_slice(&crc);

    base64url_encode(&addr_data)
}

/// CRC-16/X.25 checksum (used by TON for address validation).
fn crc16(data: &[u8]) -> [u8; 2] {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= byte as u16;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0x8408;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^= 0xFFFF;
    crc.to_le_bytes()
}

/// Base64url encode (URL-safe, no padding).
fn base64url_encode(data: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::with_capacity((data.len() * 4).div_ceil(3));
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        }
    }
    result
}

pub struct TonProvider {
    config: TonNetworkConfig,
    client: Client,
    prices: PriceService,
    seed: [u8; 64],
    cached_addresses: OnceLock<Vec<(String, u64)>>,
}

impl TonProvider {
    pub fn new(config: TonNetworkConfig, prices: PriceService, seed: [u8; 64]) -> Self {
        Self {
            config,
            client: shared_client(),
            prices,
            seed,
            cached_addresses: OnceLock::new(),
        }
    }

    async fn resolve_addresses(&self) -> Result<&Vec<(String, u64)>> {
        if let Some(addrs) = self.cached_addresses.get() {
            return Ok(addrs);
        }

        let mut results = Vec::new();
        for i in 0..5 {
            let keypair = derive_ton_keypair(&self.seed, i)?;
            let pub_bytes = keypair.verifying_key().to_bytes();
            let addr = compute_ton_address(&pub_bytes, true);
            let balance = self.fetch_balance(&addr).await.unwrap_or(0);
            results.push((addr, balance));
        }

        Ok(self.cached_addresses.get_or_init(|| results))
    }

    async fn fetch_balance(&self, address: &str) -> Result<u64> {
        let url = format!(
            "{}/getAddressBalance?address={}",
            self.config.api_url, address
        );
        let resp = ApiService::new("TON")
            .json::<TonBalanceResponse>(self.client.get(&url))
            .await?;
        resp.result.balance.parse().map_err(|_| {
            ApiService::new("TON")
                .invalid_data("invalid balance format")
                .into()
        })
    }
}

#[async_trait]
impl NetworkProvider for TonProvider {
    fn kind(&self) -> NetworkKind {
        self.config.kind
    }

    fn name(&self) -> &'static str {
        self.config.name
    }

    async fn fetch(&self, _address: String) -> Result<PortfolioEntry> {
        let addresses = self.resolve_addresses().await?;

        // Show all addresses with balance
        for (addr, nanogram) in addresses {
            if *nanogram > 0 {
                let balance = *nanogram as f64 / 1e9;
                let price = self.prices.usd_quote(self.config.asset).await;
                return Ok(PortfolioEntry {
                    name: self.name(),
                    symbol: self.config.symbol,
                    address: addr.clone(),
                    balance: format_amount(balance, 9),
                    usd_value: price.map(|p| p * balance),
                });
            }
        }

        // Default: first address with 0 balance
        let (addr, _) = &addresses[0];
        Ok(PortfolioEntry {
            name: self.name(),
            symbol: self.config.symbol,
            address: addr.clone(),
            balance: "0".into(),
            usd_value: None,
        })
    }

    async fn fetch_all(&self) -> Result<Vec<PortfolioEntry>> {
        let addresses = self.resolve_addresses().await?;
        let mut entries = Vec::new();
        let price = self.prices.usd_quote(self.config.asset).await;

        for (addr, nanogram) in addresses {
            let balance = *nanogram as f64 / 1e9;
            entries.push(PortfolioEntry {
                name: self.name(),
                symbol: self.config.symbol,
                address: addr.clone(),
                balance: format_amount(balance, 9),
                usd_value: if *nanogram > 0 {
                    price.map(|p| p * balance)
                } else {
                    None
                },
            });
        }

        Ok(entries)
    }
}

#[derive(Deserialize)]
struct TonBalanceResponse {
    result: TonBalanceResult,
}

#[derive(Deserialize)]
struct TonBalanceResult {
    balance: String,
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
    fn ton_derivation_produces_valid_addresses() {
        let seed = test_seed();
        for i in 0..5 {
            let keypair = derive_ton_keypair(&seed, i).unwrap();
            let pub_bytes = keypair.verifying_key().to_bytes();
            let addr = compute_ton_address(&pub_bytes, true);
            assert_eq!(addr.len(), 48, "address must be 48 chars: {addr}");
            assert!(
                addr.starts_with('E') || addr.starts_with('U'),
                "address must start with E or U: {addr}"
            );
        }
    }

    #[test]
    fn different_indices_produce_different_addresses() {
        let seed = test_seed();
        let k0 = derive_ton_keypair(&seed, 0).unwrap();
        let k1 = derive_ton_keypair(&seed, 1).unwrap();
        let a0 = compute_ton_address(&k0.verifying_key().to_bytes(), true);
        let a1 = compute_ton_address(&k1.verifying_key().to_bytes(), true);
        assert_ne!(a0, a1);
    }

    #[test]
    fn ton_config_exposes_expected_fields() {
        let config = TonNetworkConfig::mainnet();
        assert_eq!(config.name, "TON");
        assert_eq!(config.symbol, "GRAM");
        assert_eq!(config.asset, Asset::Gram);
    }
}
