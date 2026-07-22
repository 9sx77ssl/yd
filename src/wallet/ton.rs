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

/// Hash: sha256(d1(0x00) + d2(0xde) + 111 bytes code data) for v3R2 wallet contract.
const V3R2_CODE_HASH: [u8; 32] = [
    0x84, 0xda, 0xfa, 0x44, 0x9f, 0x98, 0xa6, 0x98, 0x77, 0x89, 0xba, 0x23, 0x23, 0x58, 0x07, 0x2b,
    0xc0, 0xf7, 0x6d, 0xc4, 0x52, 0x40, 0x02, 0xa5, 0xd0, 0x91, 0x8b, 0x9a, 0x75, 0xd2, 0xd5, 0x99,
];

fn hardened(parent: &[u8; 32], index: u32) -> Result<([u8; 32], [u8; 32])> {
    let mut mac = Hmac::<Sha512>::new_from_slice(parent).expect("HMAC key");
    mac.update(&[0x00]);
    mac.update(parent);
    mac.update(&(index | 0x80000000).to_be_bytes());
    let r = mac.finalize().into_bytes();
    Ok((r[..32].try_into().unwrap(), r[32..].try_into().unwrap()))
}

fn slip0010_master(seed: &[u8; 64]) -> [u8; 32] {
    let mut mac = Hmac::<Sha512>::new_from_slice(SLIP0010_KEY).expect("HMAC key");
    mac.update(seed);
    let master = mac.finalize().into_bytes();
    master[..32].try_into().unwrap()
}

fn derive_safepal(master: &[u8; 32], account: u32) -> Result<SigningKey> {
    let (s, _) = hardened(master, 44)?;
    let (s, _) = hardened(&s, TON_COIN)?;
    let (s, _) = hardened(&s, account)?;
    let (s, _) = hardened(&s, 0)?;
    Ok(SigningKey::from_bytes(&s))
}

fn derive_exodus(master: &[u8; 32], account: u32) -> Result<SigningKey> {
    let (s, _) = hardened(master, 44)?;
    let (s, _) = hardened(&s, TON_COIN)?;
    let (s, _) = hardened(&s, 0)?;
    let (s, _) = hardened(&s, 0)?;
    let (s, _) = hardened(&s, account)?;
    Ok(SigningKey::from_bytes(&s))
}

/// Build data cell repr: d1(0x00) + d2(0x50) + seqno(4) + wallet_id(4) + pubkey(32)
/// d2 = ceil(320/8) + floor(320/8) = 40 + 40 = 80 = 0x50 (tonsdk convention)
fn data_cell_repr(public_key: &[u8; 32], wallet_id: u32) -> [u8; 42] {
    let mut cell = [0u8; 42];
    cell[0] = 0x00;
    cell[1] = 0x50;
    cell[2..6].copy_from_slice(&0u32.to_be_bytes());
    cell[6..10].copy_from_slice(&wallet_id.to_be_bytes());
    cell[10..42].copy_from_slice(public_key);
    cell
}

fn cell_hash(repr: &[u8]) -> [u8; 32] {
    let h = Sha256::digest(repr);
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = h[i];
        i += 1;
    }
    out
}

/// StateInit hash: sha256(d1 + d2 + data + code_depth + code_hash + data_depth + data_hash)
fn state_init_hash(public_key: &[u8; 32], wallet_id: u32) -> [u8; 32] {
    let data_repr = data_cell_repr(public_key, wallet_id);
    let data_hash = cell_hash(&data_repr);

    let mut repr = Vec::with_capacity(71);
    repr.push(0x02); // d1: 2 refs (tonsdk convention: just refs count)
    repr.push(0x01); // d2: ceil(5/8) + floor(5/8) = 1
    repr.push(0x34); // data: 00110 padded with 100 (TL-B: 1 then zeros)
                     // ref depths: 2 bytes per ref, max_depth of each ref (leaf = 0)
    repr.extend_from_slice(&0u16.to_be_bytes()); // code cell depth = 0
    repr.extend_from_slice(&0u16.to_be_bytes()); // data cell depth = 0
    repr.extend_from_slice(&V3R2_CODE_HASH);
    repr.extend_from_slice(&data_hash);

    cell_hash(&repr)
}

fn encode_ton_address(hash: [u8; 32]) -> String {
    let mut addr_data = Vec::with_capacity(36);
    addr_data.push(0x51);
    addr_data.push(0x00);
    addr_data.extend_from_slice(&hash);

    let crc = crc16(&addr_data);
    addr_data.extend_from_slice(&crc);

    base64url_encode(&addr_data)
}

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
        let wallet_ids = [0, 698983191];

        for i in 0..5 {
            for derive_fn in [
                derive_safepal as fn(&[u8; 32], u32) -> Result<SigningKey>,
                derive_exodus,
            ] {
                let keypair = derive_fn(&master, i)?;
                let pub_bytes = keypair.verifying_key().to_bytes();

                for &wid in &wallet_ids {
                    let addr = compute_ton_address(&pub_bytes, wid);
                    if seen.insert(addr.clone()) {
                        let balance = self.fetch_balance(&addr).await.unwrap_or(0);
                        results.push((addr, balance));
                    }
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

fn compute_ton_address(public_key: &[u8; 32], wallet_id: u32) -> String {
    let hash = state_init_hash(public_key, wallet_id);
    encode_ton_address(hash)
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
                let addr = compute_ton_address(&pub_bytes, 0);
                assert_eq!(addr.len(), 48, "address must be 48 chars: {addr}");
                assert!(
                    addr.starts_with('U'),
                    "non-bounce must start with U: {addr}"
                );
            }
        }
    }

    #[test]
    fn different_wallet_ids_produce_different_addresses() {
        let seed = test_seed();
        let master = slip0010_master(&seed);
        let keypair = derive_safepal(&master, 0).unwrap();
        let pub_bytes = keypair.verifying_key().to_bytes();
        let a_wid0 = compute_ton_address(&pub_bytes, 0);
        let a_wid1 = compute_ton_address(&pub_bytes, 698983191);
        assert_ne!(a_wid0, a_wid1);
    }

    #[test]
    fn different_indices_produce_different_addresses() {
        let seed = test_seed();
        let master = slip0010_master(&seed);
        let k0 = derive_safepal(&master, 0).unwrap();
        let k1 = derive_safepal(&master, 1).unwrap();
        let a0 = compute_ton_address(&k0.verifying_key().to_bytes(), 0);
        let a1 = compute_ton_address(&k1.verifying_key().to_bytes(), 0);
        assert_ne!(a0, a1);
    }

    #[test]
    fn different_paths_produce_different_addresses() {
        let seed = test_seed();
        let master = slip0010_master(&seed);
        let sp = derive_safepal(&master, 0).unwrap();
        let ex = derive_exodus(&master, 0).unwrap();
        let a_sp = compute_ton_address(&sp.verifying_key().to_bytes(), 0);
        let a_ex = compute_ton_address(&ex.verifying_key().to_bytes(), 0);
        assert_ne!(a_sp, a_ex);
    }

    #[test]
    fn ton_config_exposes_expected_fields() {
        let config = TonNetworkConfig::mainnet();
        assert_eq!(config.name, "TON");
        assert_eq!(config.symbol, "GRAM");
        assert_eq!(config.asset, Asset::Gram);
    }

    #[test]
    fn address_matches_tonsdk_reference() {
        let seed = test_seed();
        let master = slip0010_master(&seed);
        let (s, _) = hardened(&master, 44).unwrap();
        let (s, _) = hardened(&s, 607).unwrap();
        let (s, _) = hardened(&s, 0).unwrap();
        let (k, _) = hardened(&s, 0).unwrap();
        let pk = SigningKey::from_bytes(&k).verifying_key().to_bytes();
        let addr = compute_ton_address(&pk, 0);
        assert_eq!(
            addr, "UQBLZj89ypxolPahcpZntJ_DrZK-pufffmMYpXGCMh4LBr3I",
            "must match tonsdk reference for test mnemonic"
        );
    }
}
