use std::path::PathBuf;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use color_eyre::eyre::{eyre, Result, WrapErr};
use directories::ProjectDirs;
use keyring::Entry;
use rand::{rngs::OsRng, RngCore};
use secrecy::{ExposeSecret, SecretString};
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};

use crate::error::YdError;

const KEYRING_SERVICE: &str = "yd";
const KEYRING_ACCOUNT: &str = "wallet-encryption-key-v1";

pub struct WalletStore {
    database_path: PathBuf,
}

impl WalletStore {
    pub fn open() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "yd", "yd")
            .ok_or_else(|| eyre!("could not determine the application data directory"))?;
        std::fs::create_dir_all(dirs.data_local_dir())
            .wrap_err("could not create application data directory")?;
        Ok(Self {
            database_path: dirs.data_local_dir().join("yd.sqlite"),
        })
    }

    pub async fn load_phrase(&self) -> Result<Option<String>> {
        let mut database = self.connect().await?;
        let record: Option<(Vec<u8>, Vec<u8>)> =
            sqlx::query_as("SELECT nonce, ciphertext FROM wallet_secrets WHERE id = 1")
                .fetch_optional(&mut database)
                .await?;
        let Some((nonce, ciphertext)) = record else {
            return Ok(None);
        };
        let key = self.encryption_key(false)?;
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| YdError::WalletCorrupted)?;
        let decrypted = cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|_| YdError::WalletCorrupted)?;
        String::from_utf8(decrypted)
            .map(Some)
            .map_err(|_| YdError::WalletCorrupted.into())
    }

    pub async fn save_phrase(&self, phrase: String) -> Result<()> {
        let key = self.encryption_key(true)?;
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| YdError::WalletCorrupted)?;
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let phrase = SecretString::from(phrase);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), phrase.expose_secret().as_bytes())
            .map_err(|_| YdError::WalletCorrupted)?;
        let mut database = self.connect().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO wallet_secrets (id, nonce, ciphertext) VALUES (1, ?, ?)",
        )
        .bind(nonce.to_vec())
        .bind(ciphertext)
        .execute(&mut database)
        .await?;
        Ok(())
    }

    pub async fn has_wallet(&self) -> Result<bool> {
        let mut database = self.connect().await?;
        let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM wallet_secrets WHERE id = 1")
            .fetch_optional(&mut database)
            .await?;
        Ok(exists.is_some())
    }

    pub async fn remove_wallet(&self) -> Result<()> {
        let mut database = self.connect().await?;
        sqlx::query("DELETE FROM wallet_secrets WHERE id = 1")
            .execute(&mut database)
            .await?;
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
            .map_err(|_| YdError::KeyringUnavailable)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(YdError::KeyringUnavailable.into()),
        }
    }

    async fn connect(&self) -> Result<SqliteConnection> {
        let options = SqliteConnectOptions::new()
            .filename(&self.database_path)
            .create_if_missing(true);
        let mut connection = SqliteConnection::connect_with(&options).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS wallet_secrets (id INTEGER PRIMARY KEY CHECK (id = 1), nonce BLOB NOT NULL, ciphertext BLOB NOT NULL)")
            .execute(&mut connection).await?;
        Ok(connection)
    }

    fn encryption_key(&self, create: bool) -> Result<[u8; 32]> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
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
            Err(_) => Err(YdError::KeyringUnavailable.into()),
        }
    }
}

fn decode_key(encoded: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(encoded).map_err(|_| YdError::WalletCorrupted)?;
    bytes
        .try_into()
        .map_err(|_| YdError::WalletCorrupted.into())
}
