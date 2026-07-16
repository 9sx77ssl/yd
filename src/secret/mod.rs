//! Shared secret storage.
//!
//! Any module that must persist private data (seed phrases, private notes)
//! routes through [`SecretStore`]. Secrets are AES-256-GCM encrypted in
//! SQLite; the symmetric key lives in the system keyring and never touches
//! disk in plaintext. The on-disk format is identical to earlier yd releases,
//! so an existing wallet keeps working after upgrade.

mod crypto;
mod keyring;

use color_eyre::eyre::Result;
use secrecy::{ExposeSecret, SecretString};

use crate::store::{Database, Migration};

pub(super) const KEYRING_SERVICE: &str = "yd";
pub(super) const KEYRING_ACCOUNT: &str = "wallet-encryption-key-v1";

/// Persists encrypted secrets into the shared database.
#[derive(Clone)]
pub struct SecretStore {
    database: Database,
}

impl SecretStore {
    /// `wallet_secrets` predates this module; keep the exact schema so the
    /// first yd release that stored a wallet still reads it back.
    pub const MIGRATION: Migration = Migration::new(1, &[
        "CREATE TABLE IF NOT EXISTS wallet_secrets (id INTEGER PRIMARY KEY CHECK (id = 1), nonce BLOB NOT NULL, ciphertext BLOB NOT NULL)",
    ]);

    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    /// Loads the stored secret, or `None` when nothing is registered yet.
    pub async fn load(&self) -> Result<Option<SecretString>> {
        let mut connection = self.database.connect().await?;
        let record: Option<(Vec<u8>, Vec<u8>)> =
            sqlx::query_as("SELECT nonce, ciphertext FROM wallet_secrets WHERE id = 1")
                .fetch_optional(&mut connection)
                .await?;
        let Some((nonce, ciphertext)) = record else {
            return Ok(None);
        };

        let key = keyring::load_key(false)?;
        let plaintext = crypto::decrypt(&key, &nonce, &ciphertext)?;
        let text = String::from_utf8(plaintext)
            .map_err(|_| crate::error::YdError::Corrupted { context: "secret" })?;
        Ok(Some(SecretString::from(text)))
    }

    /// Encrypts and stores `secret`, replacing any prior value.
    pub async fn save(&self, secret: SecretString) -> Result<()> {
        let key = keyring::load_key(true)?;
        let (nonce, ciphertext) = crypto::encrypt(&key, secret.expose_secret().as_bytes())?;
        let mut connection = self.database.connect().await?;
        sqlx::query(
            "INSERT OR REPLACE INTO wallet_secrets (id, nonce, ciphertext) VALUES (1, ?, ?)",
        )
        .bind(nonce)
        .bind(ciphertext)
        .execute(&mut connection)
        .await?;
        Ok(())
    }

    /// Returns whether a secret is currently stored.
    pub async fn exists(&self) -> Result<bool> {
        let mut connection = self.database.connect().await?;
        let record: Option<(i64,)> = sqlx::query_as("SELECT id FROM wallet_secrets WHERE id = 1")
            .fetch_optional(&mut connection)
            .await?;
        Ok(record.is_some())
    }

    /// Removes the stored secret and the keyring key.
    pub async fn remove(&self) -> Result<()> {
        let mut connection = self.database.connect().await?;
        sqlx::query("DELETE FROM wallet_secrets WHERE id = 1")
            .execute(&mut connection)
            .await?;
        keyring::delete_key()?;
        Ok(())
    }
}
