use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::{rngs::OsRng, RngCore};

use crate::error::YdError;

/// 96-bit GCM nonce as required by AES-GCM.
pub const NONCE_LEN: usize = 12;
/// 256-bit AES key.
pub const KEY_LEN: usize = 32;

/// Encrypts `plaintext`, returning `(nonce, ciphertext)`.
pub fn encrypt(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), YdError> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| YdError::Corrupted { context: "secret" })?;
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|_| YdError::Corrupted { context: "secret" })?;
    Ok((nonce.to_vec(), ciphertext))
}

/// Decrypts `ciphertext` using `nonce`.
pub fn decrypt(key: &[u8; KEY_LEN], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, YdError> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| YdError::Corrupted { context: "secret" })?;
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| YdError::Corrupted { context: "secret" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trip() {
        let mut key = [0u8; KEY_LEN];
        OsRng.fill_bytes(&mut key);
        let plaintext = b"top secret wallet phrase";

        let (nonce, ciphertext) = encrypt(&key, plaintext).unwrap();
        assert_ne!(plaintext.as_slice(), ciphertext.as_slice());
        assert_ne!(nonce, Vec::<u8>::new());

        let recovered = decrypt(&key, &nonce, &ciphertext).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let (nonce, ciphertext) = encrypt(&[0u8; KEY_LEN], b"payload").unwrap();
        assert!(decrypt(&[1u8; KEY_LEN], &nonce, &ciphertext).is_err());
    }

    #[test]
    fn decrypt_with_tampered_ciphertext_fails() {
        let (nonce, mut ciphertext) = encrypt(&[0u8; KEY_LEN], b"payload").unwrap();
        ciphertext[0] ^= 0xff;
        assert!(decrypt(&[0u8; KEY_LEN], &nonce, &ciphertext).is_err());
    }
}
