use std::str::FromStr;

use bip39::Mnemonic;
use bitcoin::{
    bip32::{DerivationPath, Xpriv},
    secp256k1::Secp256k1,
    Network,
};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use sha2::{Digest, Sha256};
use sha3::Keccak256;

use super::model::NetworkKind;
use crate::error::YdError;

#[allow(dead_code)]
const LITECOIN_P2PKH_VERSION: u8 = 0x30;
#[allow(dead_code)]
const BASE58CHECK_CHECKSUM_LEN: usize = 4;
#[allow(dead_code)]
const HASH160_LEN: usize = 20;
#[allow(dead_code)]
const ETHEREUM_ADDRESS_LEN: usize = 20;

/// HD-wallet key material derived once from a BIP-39 mnemonic.
///
/// Addresses are produced on demand via [`address_for`], and each derivation
/// is checked against [`AddressValidator`] in a debug assertion so a silent
/// regression in path handling surfaces during development.
#[allow(dead_code)]
pub struct WalletKeys {
    seed: [u8; 64],
}

#[allow(dead_code)]
impl WalletKeys {
    pub fn from_mnemonic(phrase: &str) -> Result<Self, YdError> {
        let mnemonic = phrase
            .parse::<Mnemonic>()
            .map_err(|error| YdError::InvalidMnemonic(error.to_string()))?;
        Ok(Self {
            seed: mnemonic.to_seed(""),
        })
    }

    pub fn address_for(&self, network: NetworkKind) -> String {
        let address = match network {
            NetworkKind::Ethereum | NetworkKind::BnbChain | NetworkKind::Polygon => {
                self.evm_address()
            }
            NetworkKind::Bitcoin => self.bitcoin_address(),
            NetworkKind::Litecoin => self.litecoin_address(),
            NetworkKind::Ton => String::new(), // TON uses its own derivation
        };

        debug_assert_eq!(
            AddressValidator::validate(network, &address),
            AddressValidation::Valid
        );

        address
    }

    fn derive_secret(&self, path: &str) -> bitcoin::secp256k1::SecretKey {
        let master = Xpriv::new_master(Network::Bitcoin, &self.seed).expect("valid seed length");
        let path = DerivationPath::from_str(path).expect("static derivation path");
        master
            .derive_priv(&Secp256k1::new(), &path)
            .expect("valid derivation path")
            .private_key
    }

    fn evm_address(&self) -> String {
        let secret = self.derive_secret("m/44'/60'/0'/0/0");
        let signing_key =
            k256::SecretKey::from_slice(&secret.secret_bytes()).expect("secp256k1 key");
        let encoded = signing_key.public_key().to_encoded_point(false);
        let hash = Keccak256::digest(&encoded.as_bytes()[1..]);
        format!("0x{}", hex::encode(&hash[12..]))
    }

    fn bitcoin_address(&self) -> String {
        let secret = self.derive_secret("m/84'/0'/0'/0/0");
        let public = bitcoin::secp256k1::PublicKey::from_secret_key(&Secp256k1::new(), &secret);
        bitcoin::Address::p2wpkh(&bitcoin::CompressedPublicKey(public), Network::Bitcoin)
            .to_string()
    }

    fn litecoin_address(&self) -> String {
        let secret = self.derive_secret("m/44'/2'/0'/0/0");
        let public =
            bitcoin::secp256k1::PublicKey::from_secret_key(&Secp256k1::new(), &secret).serialize();
        let sha = Sha256::digest(public);
        let ripe = ripemd::Ripemd160::digest(sha);
        let mut payload = Vec::with_capacity(25);
        payload.push(0x30);
        payload.extend_from_slice(&ripe);
        let checksum = Sha256::digest(Sha256::digest(&payload));
        payload.extend_from_slice(&checksum[..4]);
        bs58::encode(payload).into_string()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum AddressValidation {
    Valid,
    Invalid,
}

#[allow(dead_code)]
pub struct AddressValidator;

#[allow(dead_code)]
impl AddressValidator {
    pub fn validate(network: NetworkKind, address: &str) -> AddressValidation {
        let valid = match network {
            NetworkKind::Ethereum | NetworkKind::BnbChain | NetworkKind::Polygon => {
                Self::is_valid_evm_address(address)
            }
            NetworkKind::Bitcoin => Self::is_valid_bitcoin_address(address),
            NetworkKind::Litecoin => Self::is_valid_litecoin_address(address),
            NetworkKind::Ton => Self::is_valid_ton_address(address),
        };

        if valid {
            AddressValidation::Valid
        } else {
            AddressValidation::Invalid
        }
    }

    fn is_valid_evm_address(address: &str) -> bool {
        let Some(hex_address) = address.strip_prefix("0x") else {
            return false;
        };

        if hex_address.len() != ETHEREUM_ADDRESS_LEN * 2 || hex::decode(hex_address).is_err() {
            return false;
        }

        if hex_address
            .chars()
            .all(|character| !character.is_ascii_alphabetic() || character.is_ascii_lowercase())
            || hex_address
                .chars()
                .all(|character| !character.is_ascii_alphabetic() || character.is_ascii_uppercase())
        {
            return true;
        }

        Self::has_valid_eip55_checksum(hex_address)
    }

    fn has_valid_eip55_checksum(hex_address: &str) -> bool {
        let lowercase = hex_address.to_ascii_lowercase();
        let hash = hex::encode(Keccak256::digest(lowercase.as_bytes()));

        hex_address
            .chars()
            .zip(hash.chars())
            .all(|(address, hash)| {
                if !address.is_ascii_alphabetic() {
                    return true;
                }

                let hash_nibble = hash.to_digit(16).expect("keccak hash is hex");
                address.is_ascii_uppercase() == (hash_nibble >= 8)
            })
    }

    fn is_valid_bitcoin_address(address: &str) -> bool {
        address
            .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()
            .and_then(|address| address.require_network(Network::Bitcoin))
            .is_ok()
    }

    fn is_valid_litecoin_address(address: &str) -> bool {
        let Ok(payload) = bs58::decode(address).into_vec() else {
            return false;
        };

        let Some((body, checksum)) =
            payload.split_at_checked(payload.len().saturating_sub(BASE58CHECK_CHECKSUM_LEN))
        else {
            return false;
        };

        if body.len() != HASH160_LEN + 1 || checksum.len() != BASE58CHECK_CHECKSUM_LEN {
            return false;
        }

        if body[0] != LITECOIN_P2PKH_VERSION {
            return false;
        }

        let expected = Sha256::digest(Sha256::digest(body));
        checksum == &expected[..BASE58CHECK_CHECKSUM_LEN]
    }

    fn is_valid_ton_address(address: &str) -> bool {
        if address.len() != 48 {
            return false;
        }
        let allowed = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        if !address.chars().all(|c| allowed.contains(c)) {
            return false;
        }
        let Ok(decoded) = base64url_decode(address) else {
            return false;
        };
        decoded.len() == 36
    }
}

#[allow(dead_code)]
fn base64url_decode(input: &str) -> Result<Vec<u8>, ()> {
    let mut s = input.replace('-', "+").replace('_', "/");
    while !s.len().is_multiple_of(4) {
        s.push('=');
    }
    let mut output = Vec::new();
    for chunk in s.as_bytes().chunks(4) {
        let chunk_str = std::str::from_utf8(chunk).map_err(|_| ())?;
        let val = base64_char_to_val(chunk_str.as_bytes()[0]).ok_or(())? << 18
            | base64_char_to_val(chunk_str.as_bytes()[1]).ok_or(())? << 12
            | base64_char_to_val(chunk_str.as_bytes()[2]).ok_or(())? << 6
            | base64_char_to_val(chunk_str.as_bytes()[3]).ok_or(())?;
        output.push((val >> 16) as u8);
        if chunk_str.as_bytes()[2] != b'=' {
            output.push((val >> 8) as u8);
        }
        if chunk_str.as_bytes()[3] != b'=' {
            output.push(val as u8);
        }
    }
    Ok(output)
}

#[allow(dead_code)]
fn base64_char_to_val(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some((c - b'A') as u32),
        b'a'..=b'z' => Some((c - b'a' + 26) as u32),
        b'0'..=b'9' => Some((c - b'0' + 52) as u32),
        b'+' => Some(62),
        b'/' => Some(63),
        b'=' => Some(0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn derives_valid_default_network_addresses() {
        let keys = WalletKeys::from_mnemonic(TEST_MNEMONIC).expect("valid test mnemonic");
        let ethereum = keys.address_for(NetworkKind::Ethereum);
        let bnb_chain = keys.address_for(NetworkKind::BnbChain);
        let polygon = keys.address_for(NetworkKind::Polygon);
        let bitcoin = keys.address_for(NetworkKind::Bitcoin);
        let litecoin = keys.address_for(NetworkKind::Litecoin);

        assert_eq!(
            AddressValidator::validate(NetworkKind::Ethereum, &ethereum),
            AddressValidation::Valid
        );
        assert_eq!(
            AddressValidator::validate(NetworkKind::BnbChain, &bnb_chain),
            AddressValidation::Valid
        );
        assert_eq!(
            AddressValidator::validate(NetworkKind::Polygon, &polygon),
            AddressValidation::Valid
        );
        assert_eq!(ethereum, bnb_chain);
        assert_eq!(ethereum, polygon);
        assert_eq!(
            AddressValidator::validate(NetworkKind::Bitcoin, &bitcoin),
            AddressValidation::Valid
        );
        assert_eq!(
            AddressValidator::validate(NetworkKind::Litecoin, &litecoin),
            AddressValidation::Valid
        );
    }

    #[test]
    fn rejects_malformed_addresses() {
        assert_eq!(
            AddressValidator::validate(NetworkKind::Ethereum, "0xnot-hex"),
            AddressValidation::Invalid
        );
        assert_eq!(
            AddressValidator::validate(
                NetworkKind::Ethereum,
                "0x52908400098527886E0F7030069857D2E4169EE7"
            ),
            AddressValidation::Valid
        );
        assert_eq!(
            AddressValidator::validate(
                NetworkKind::Ethereum,
                "0x52908400098527886e0F7030069857D2E4169EE7"
            ),
            AddressValidation::Invalid
        );
        assert_eq!(
            AddressValidator::validate(
                NetworkKind::Bitcoin,
                "tb1qfm6s0quzjy8r7z5jy4w39rxfw0p27s3lgd4w0p"
            ),
            AddressValidation::Invalid
        );
        assert_eq!(
            AddressValidator::validate(NetworkKind::Litecoin, "Lh3PQZTcSxbDxPVTN6AgAQx3xYWwsbcWmn"),
            AddressValidation::Invalid
        );
    }

    #[test]
    fn rejects_invalid_mnemonics() {
        assert!(WalletKeys::from_mnemonic("not a seed phrase").is_err());
    }
}
