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

use super::model::Asset;
use crate::error::YdError;

const KEYRING_SERVICE: &str = "yd";
const KEYRING_ACCOUNT: &str = "wallet-encryption-key-v1";
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        statements: &[
            "CREATE TABLE IF NOT EXISTS wallet_secrets (id INTEGER PRIMARY KEY CHECK (id = 1), nonce BLOB NOT NULL, ciphertext BLOB NOT NULL)",
        ],
    },
    Migration {
        version: 2,
        statements: &[
            "CREATE TABLE IF NOT EXISTS price_cache (asset TEXT PRIMARY KEY, usd REAL NOT NULL, fetched_at INTEGER NOT NULL)",
        ],
    },
];

#[derive(Clone)]
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

    pub fn database_path(&self) -> &PathBuf {
        &self.database_path
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

    pub async fn cached_usd_quote(
        &self,
        asset: Asset,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<Option<f64>> {
        let mut database = self.connect().await?;
        let record: Option<(f64, i64)> =
            sqlx::query_as("SELECT usd, fetched_at FROM price_cache WHERE asset = ?")
                .bind(asset.cache_key())
                .fetch_optional(&mut database)
                .await?;
        let Some((usd, fetched_at)) = record else {
            return Ok(None);
        };

        if now.saturating_sub(fetched_at) <= ttl_seconds {
            Ok(Some(usd))
        } else {
            Ok(None)
        }
    }

    pub async fn save_usd_quote(&self, asset: Asset, usd: f64, fetched_at: i64) -> Result<()> {
        let mut database = self.connect().await?;
        sqlx::query("INSERT OR REPLACE INTO price_cache (asset, usd, fetched_at) VALUES (?, ?, ?)")
            .bind(asset.cache_key())
            .bind(usd)
            .bind(fetched_at)
            .execute(&mut database)
            .await?;
        Ok(())
    }

    async fn connect(&self) -> Result<SqliteConnection> {
        let options = SqliteConnectOptions::new()
            .filename(&self.database_path)
            .create_if_missing(true);
        let mut connection = SqliteConnection::connect_with(&options).await?;
        configure_database(&mut connection).await?;
        run_migrations(&mut connection).await?;
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

struct Migration {
    version: i64,
    statements: &'static [&'static str],
}

async fn configure_database(connection: &mut SqliteConnection) -> Result<()> {
    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&mut *connection)
        .await?;
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *connection)
        .await?;
    sqlx::query("PRAGMA busy_timeout = 5000")
        .execute(connection)
        .await?;
    Ok(())
}

async fn run_migrations(connection: &mut SqliteConnection) -> Result<()> {
    sqlx::query("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY)")
        .execute(&mut *connection)
        .await?;

    for migration in MIGRATIONS {
        let applied: Option<(i64,)> =
            sqlx::query_as("SELECT version FROM schema_migrations WHERE version = ?")
                .bind(migration.version)
                .fetch_optional(&mut *connection)
                .await?;
        if applied.is_some() {
            continue;
        }

        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await?;
        for statement in migration.statements {
            if let Err(error) = sqlx::query(statement).execute(&mut *connection).await {
                let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
                return Err(error.into());
            }
        }
        if let Err(error) = sqlx::query("INSERT INTO schema_migrations (version) VALUES (?)")
            .bind(migration.version)
            .execute(&mut *connection)
            .await
        {
            let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
            return Err(error.into());
        }
        sqlx::query("COMMIT").execute(&mut *connection).await?;
    }

    Ok(())
}

fn decode_key(encoded: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(encoded).map_err(|_| YdError::WalletCorrupted)?;
    bytes
        .try_into()
        .map_err(|_| YdError::WalletCorrupted.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn cached_usd_quote_expires_after_ttl() {
        let database_path = temp_database_path("price-cache");
        let store = WalletStore {
            database_path: database_path.clone(),
        };

        store
            .save_usd_quote(Asset::Ethereum, 123.45, 1_000)
            .await
            .unwrap();

        assert_eq!(
            store
                .cached_usd_quote(Asset::Ethereum, 1_024, 25)
                .await
                .unwrap(),
            Some(123.45)
        );
        assert_eq!(
            store
                .cached_usd_quote(Asset::Ethereum, 1_026, 25)
                .await
                .unwrap(),
            None
        );

        cleanup_database(database_path);
    }

    fn temp_database_path(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("yd-{name}-{}-{suffix}.sqlite", std::process::id()))
    }

    fn cleanup_database(path: PathBuf) {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    }
}
