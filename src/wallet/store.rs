use color_eyre::eyre::Result;
use secrecy::SecretString;

use crate::secret::SecretStore;
use crate::store::{Database, Migration};

/// Wallet-specific persistence.
///
/// All encryption, keyring, and connection plumbing lives in [`SecretStore`]
/// and [`Database`]; this thin wrapper only names the wallet's migration and
/// forwards secret operations. New wallet data fields add a migration here,
/// not a new storage stack.
#[derive(Clone)]
pub struct WalletStore {
    secrets: SecretStore,
}

impl WalletStore {
    /// Runs every migration the wallet (and its dependencies) needs.
    pub const MIGRATIONS: &'static [Migration] = &[
        SecretStore::MIGRATION,
        // Historical v2 created the price_cache table; the TTL cache now uses
        // its own table (v3). Keep a no-op marker so the ledger version stays
        // monotonic and old databases do not re-run stray statements.
        Migration::new(2, &["SELECT 1"]),
        crate::store::TtlCache::MIGRATION,
    ];

    pub fn new(database: Database) -> Self {
        Self {
            secrets: SecretStore::new(database),
        }
    }

    pub fn database(&self) -> &Database {
        self.secrets.database()
    }

    pub async fn load_phrase(&self) -> Result<Option<SecretString>> {
        self.secrets.load().await
    }

    pub async fn save_phrase(&self, phrase: SecretString) -> Result<()> {
        self.secrets.save(phrase).await
    }

    pub async fn has_wallet(&self) -> Result<bool> {
        self.secrets.exists().await
    }

    pub async fn remove_wallet(&self) -> Result<()> {
        self.secrets.remove().await
    }
}
