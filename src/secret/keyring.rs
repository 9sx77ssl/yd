use keyring::Entry;
use rand::{rngs::OsRng, RngCore};

use crate::error::YdError;

/// Loads (or, when `create` is set, provisions) the AES key guarding a secret.
///
/// All yd secrets share one keyring entry, identified by the long-stable
/// `KEYRING_ACCOUNT`. This keeps existing installations working: the wallet
/// key written by earlier versions is read back unchanged.
pub fn load_key(create: bool) -> Result<[u8; 32], YdError> {
    let entry = Entry::new(super::KEYRING_SERVICE, super::KEYRING_ACCOUNT)
        .map_err(|_| YdError::KeyringUnavailable)?;
    match entry.get_password() {
        Ok(encoded) => decode_key(&encoded),
        Err(keyring::Error::NoEntry) if create => {
            let mut key = [0u8; 32];
            OsRng.fill_bytes(&mut key);
            entry
                .set_password(&hex::encode(key))
                .map_err(|_| YdError::KeyringUnavailable)?;
            Ok(key)
        }
        Err(_) => Err(YdError::KeyringUnavailable),
    }
}

/// Removes the keyring entry (missing entries are treated as success).
pub fn delete_key() -> Result<(), YdError> {
    let entry = Entry::new(super::KEYRING_SERVICE, super::KEYRING_ACCOUNT)
        .map_err(|_| YdError::KeyringUnavailable)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(YdError::KeyringUnavailable),
    }
}

fn decode_key(encoded: &str) -> Result<[u8; 32], YdError> {
    let bytes = hex::decode(encoded).map_err(|_| YdError::Corrupted { context: "secret" })?;
    bytes
        .try_into()
        .map_err(|_| YdError::Corrupted { context: "secret" })
}
