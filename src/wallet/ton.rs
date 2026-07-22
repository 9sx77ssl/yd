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
/// Hash: feb5ff6820e2ff0d9483e7e0d62c817d846789fb4ae580c878866d959dabd5c0
const V4R2_CODE_HEX: &str = "0b5ee9c72410101010003000002dc0010a09fc4d3098164a4156076200b40917c680405035c302120101061f00620020030030201520003020114000402012c0005030050402015201040500e0300201320070201520009000b00010d08ed54f1e05c0e12b4c737527a54c616e42727f16f219f11fb5f1b150f819d3002b2aa8302015202020109000b000111040500e10302012c000600012cad000c020152010a000f0009000530c82f0903020114000402012c0007000a232b2bb290d8e9038d05a04e903bc15c0904880008e478b04012004130bd0";

/// v3R2 wallet code BoC (Bag of Cells) as hex.
/// Hash: 84dafa449f98a6987789ba232358072bc0f76dc4524002a5d0918b9a75d2d599
/// Data layout: seqno(32) + subwallet_id(32) + public_key(256)
const V3R2_CODE_HEX: &str = "b5ee9c724101010100710000deff0020dd2082014c97ba218201339cbab19f71b0ed44d0d31fd31f31d70bffe304e0a4f2608308d71820d31fd31fd31ff82313bbf263ed44d0d31fd31fd3ffd15132baf2a15144baf2a204f901541055f910f2a3f8009320d74a96d307d402fb00e8d101a4c8cb1fcb1fcbffc9ed5410bd6dad";

/// Hardened derivation: HMAC-SHA512(key=chain, 0x00 || secret || index)
fn hardened(parent: &[u8; 32], index: u32) -> Result<([u8; 32], [u8; 32])> {
    let mut mac = Hmac::<Sha512>::new_from_slice(parent).expect("HMAC key");
    mac.update(&[0x00]);
    mac.update(parent);
    mac.update(&(index | 0x80000000).to_be_bytes());
    let r = mac.finalize().into_bytes();
    Ok((r[..32].try_into().unwrap(), r[32..].try_into().unwrap()))
}

/// SLIP-0010 master key from seed.
fn slip0010_master(seed: &[u8; 64]) -> [u8; 32] {
    let mut mac = Hmac::<Sha512>::new_from_slice(SLIP0010_KEY).expect("HMAC key");
    mac.update(seed);
    let master = mac.finalize().into_bytes();
    master[..32].try_into().unwrap()
}

/// Derives keypair for path: m/44'/607'/i'/0'  (SafePal v3R2: 4 levels)
fn derive_safepal(master: &[u8; 32], account: u32) -> Result<SigningKey> {
    let (s, _) = hardened(master, 44)?;
    let (s, _) = hardened(&s, TON_COIN)?;
    let (s, _) = hardened(&s, account)?;
    let (s, _) = hardened(&s, 0)?;
    Ok(SigningKey::from_bytes(&s))
}

/// Derives keypair for path: m/44'/607'/0'/0'/i' (Exodus: 5 levels)
fn derive_exodus(master: &[u8; 32], account: u32) -> Result<SigningKey> {
    let (s, _) = hardened(master, 44)?;
    let (s, _) = hardened(&s, TON_COIN)?;
    let (s, _) = hardened(&s, 0)?;
    let (s, _) = hardened(&s, 0)?;
    let (s, _) = hardened(&s, account)?;
    Ok(SigningKey::from_bytes(&s))
}

/// TON address from public key using v3R2 wallet contract.
/// Data layout: seqno(0) + subwallet_id(0) + pubkey
fn compute_ton_address_v3r2(public_key: &[u8; 32], bounceable: bool) -> String {
    let code_bytes = hex::decode(V3R2_CODE_HEX).expect("valid hex");

    let mut data_bytes = Vec::with_capacity(40);
    data_bytes.extend_from_slice(&0u32.to_be_bytes()); // seqno = 0
    data_bytes.extend_from_slice(&0u32.to_be_bytes()); // subwallet_id = 0
    data_bytes.extend_from_slice(public_key);

    let mut hasher = Sha256::new();
    hasher.update(&code_bytes);
    hasher.update(&data_bytes);
    let hash: [u8; 32] = hasher.finalize().into();

    encode_ton_address(hash, bounceable)
}

/// TON address from public key using v4R2 wallet contract.
/// Data layout: wallet_id(0) + seqno(0) + pubkey + empty_dict(1 bit)
fn compute_ton_address_v4r2(public_key: &[u8; 32], bounceable: bool) -> String {
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

    encode_ton_address(hash, bounceable)
}

/// Encode raw address hash into TON user-friendly format.
fn encode_ton_address(hash: [u8; 32], bounceable: bool) -> String {
    let tag: u8 = if bounceable { 0x11 } else { 0x51 };

    let mut addr_data = Vec::with_capacity(36);
    addr_data.push(tag);
    addr_data.push(0x00);
    addr_data.extend_from_slice(&hash);

    let crc = crc16(&addr_data);
    addr_data.extend_from_slice(&crc);

    base64url_encode(&addr_data)
}

/// CRC-16/X.25 checksum.
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

        let master = slip0010_master(&self.seed);
        let mut seen = std::collections::HashSet::new();
        let mut results = Vec::new();

        for i in 0..5 {
            for derive_fn in [
                derive_safepal as fn(&[u8; 32], u32) -> Result<SigningKey>,
                derive_exodus,
            ] {
                let keypair = derive_fn(&master, i)?;
                let pub_bytes = keypair.verifying_key().to_bytes();

                let addr_v3r2 = compute_ton_address_v3r2(&pub_bytes, false);
                if seen.insert(addr_v3r2.clone()) {
                    let balance = self.fetch_balance(&addr_v3r2).await.unwrap_or(0);
                    results.push((addr_v3r2, balance));
                }

                let addr_v4r2 = compute_ton_address_v4r2(&pub_bytes, false);
                if seen.insert(addr_v4r2.clone()) {
                    let balance = self.fetch_balance(&addr_v4r2).await.unwrap_or(0);
                    results.push((addr_v4r2, balance));
                }
            }
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
        let master = slip0010_master(&seed);
        for i in 0..3 {
            for derive_fn in [
                derive_safepal as fn(&[u8; 32], u32) -> Result<SigningKey>,
                derive_exodus,
            ] {
                let keypair = derive_fn(&master, i).unwrap();
                let pub_bytes = keypair.verifying_key().to_bytes();
                let addr = compute_ton_address_v3r2(&pub_bytes, false);
                assert_eq!(addr.len(), 48, "v3r2 address must be 48 chars: {addr}");
                assert!(
                    addr.starts_with('U'),
                    "v3r2 non-bounce must start with U: {addr}"
                );
                let addr = compute_ton_address_v4r2(&pub_bytes, false);
                assert_eq!(addr.len(), 48, "v4r2 address must be 48 chars: {addr}");
                assert!(
                    addr.starts_with('U'),
                    "v4r2 non-bounce must start with U: {addr}"
                );
            }
        }
    }

    #[test]
    fn different_wallet_versions_produce_different_addresses() {
        let seed = test_seed();
        let master = slip0010_master(&seed);
        let keypair = derive_safepal(&master, 0).unwrap();
        let pub_bytes = keypair.verifying_key().to_bytes();
        let a_v3 = compute_ton_address_v3r2(&pub_bytes, false);
        let a_v4 = compute_ton_address_v4r2(&pub_bytes, false);
        assert_ne!(a_v3, a_v4);
    }

    #[test]
    fn different_indices_produce_different_addresses() {
        let seed = test_seed();
        let master = slip0010_master(&seed);
        let k0 = derive_safepal(&master, 0).unwrap();
        let k1 = derive_safepal(&master, 1).unwrap();
        let a0 = compute_ton_address_v3r2(&k0.verifying_key().to_bytes(), false);
        let a1 = compute_ton_address_v3r2(&k1.verifying_key().to_bytes(), false);
        assert_ne!(a0, a1);
    }

    #[test]
    fn different_paths_produce_different_addresses() {
        let seed = test_seed();
        let master = slip0010_master(&seed);
        let sp = derive_safepal(&master, 0).unwrap();
        let ex = derive_exodus(&master, 0).unwrap();
        let a_sp = compute_ton_address_v3r2(&sp.verifying_key().to_bytes(), false);
        let a_ex = compute_ton_address_v3r2(&ex.verifying_key().to_bytes(), false);
        assert_ne!(a_sp, a_ex);
    }

    #[test]
    fn ton_config_exposes_expected_fields() {
        let config = TonNetworkConfig::mainnet();
        assert_eq!(config.name, "TON");
        assert_eq!(config.symbol, "GRAM");
        assert_eq!(config.asset, Asset::Gram);
    }
}
