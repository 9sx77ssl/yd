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

use super::provider::NetworkKind;
use crate::error::YdError;

pub struct WalletKeys {
    seed: [u8; 64],
}

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
        match network {
            NetworkKind::Ethereum => self.ethereum_address(),
            NetworkKind::Bitcoin => self.bitcoin_address(),
            NetworkKind::Litecoin => self.litecoin_address(),
        }
    }

    fn derive_secret(&self, path: &str) -> bitcoin::secp256k1::SecretKey {
        let master = Xpriv::new_master(Network::Bitcoin, &self.seed).expect("valid seed length");
        let path = DerivationPath::from_str(path).expect("static derivation path");
        master
            .derive_priv(&Secp256k1::new(), &path)
            .expect("valid derivation path")
            .private_key
    }

    fn ethereum_address(&self) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn derives_recognisable_default_network_addresses() {
        let keys = WalletKeys::from_mnemonic(TEST_MNEMONIC).expect("valid test mnemonic");
        let ethereum = keys.address_for(NetworkKind::Ethereum);
        let bitcoin = keys.address_for(NetworkKind::Bitcoin);
        let litecoin = keys.address_for(NetworkKind::Litecoin);

        assert!(ethereum.starts_with("0x") && ethereum.len() == 42);
        assert!(bitcoin.starts_with("bc1q"));
        assert!(bitcoin
            .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()
            .is_ok());
        assert!(litecoin.starts_with('L'));
        assert_eq!(
            bs58::decode(litecoin)
                .into_vec()
                .expect("valid Base58Check payload")
                .len(),
            25
        );
    }

    #[test]
    fn rejects_invalid_mnemonics() {
        assert!(WalletKeys::from_mnemonic("not a seed phrase").is_err());
    }
}
