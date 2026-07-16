use color_eyre::eyre::Result;
use sqlx::SqliteConnection;

use super::Migration;
use crate::error::YdError;

/// A timestamped, expiring cache stored in SQLite.
///
/// Backed by the shared `ttl_cache` table: `(key TEXT PRIMARY KEY, value TEXT,
/// fetched_at INTEGER)`. Callers own the (de)serialisation of `value`, so the
/// same table serves prices, quotes, or any short-lived public data.
pub struct TtlCache;

#[allow(dead_code)]
impl TtlCache {
    pub const MIGRATION: Migration = Migration::new(3, &[
        "CREATE TABLE IF NOT EXISTS ttl_cache (key TEXT PRIMARY KEY, value TEXT NOT NULL, fetched_at INTEGER NOT NULL)",
    ]);

    /// Returns the cached value if it exists and is younger than `ttl_seconds`.
    pub async fn get(
        connection: &mut SqliteConnection,
        key: &str,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<Option<String>> {
        let record: Option<(String, i64)> =
            sqlx::query_as("SELECT value, fetched_at FROM ttl_cache WHERE key = ?")
                .bind(key)
                .fetch_optional(connection)
                .await?;
        let Some((value, fetched_at)) = record else {
            return Ok(None);
        };

        if now.saturating_sub(fetched_at) <= ttl_seconds {
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    /// Stores `value` under `key`, stamping it with `now`.
    pub async fn set(
        connection: &mut SqliteConnection,
        key: &str,
        value: &str,
        now: i64,
    ) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO ttl_cache (key, value, fetched_at) VALUES (?, ?, ?)")
            .bind(key)
            .bind(value)
            .bind(now)
            .execute(connection)
            .await?;
        Ok(())
    }

    /// Removes a single cache entry.
    pub async fn remove(connection: &mut SqliteConnection, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM ttl_cache WHERE key = ?")
            .bind(key)
            .execute(connection)
            .await?;
        Ok(())
    }

    /// Coerces a decode failure into a typed corrupted error.
    pub fn corrupted(context: &'static str) -> YdError {
        YdError::Corrupted { context }
    }
}

#[cfg(test)]
mod tests {
    use super::super::database::Database;
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn ttl_cache_expires_after_ttl() {
        let database = temp_database();
        database.migrate(&[TtlCache::MIGRATION]).await.unwrap();

        let mut connection = database.connect().await.unwrap();
        TtlCache::set(&mut connection, "eth", "123.45", 1_000)
            .await
            .unwrap();

        assert_eq!(
            TtlCache::get(&mut connection, "eth", 1_024, 25)
                .await
                .unwrap(),
            Some("123.45".to_owned())
        );
        assert_eq!(
            TtlCache::get(&mut connection, "eth", 1_026, 25)
                .await
                .unwrap(),
            None
        );

        cleanup_database(database.database_path().to_path_buf());
    }

    #[tokio::test]
    async fn ttl_cache_remove_drops_entry() {
        let database = temp_database();
        database.migrate(&[TtlCache::MIGRATION]).await.unwrap();
        let mut connection = database.connect().await.unwrap();
        TtlCache::set(&mut connection, "eth", "1", 0).await.unwrap();
        TtlCache::remove(&mut connection, "eth").await.unwrap();
        assert_eq!(
            TtlCache::get(&mut connection, "eth", 0, 1).await.unwrap(),
            None
        );
        cleanup_database(database.database_path().to_path_buf());
    }

    fn temp_database() -> Database {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Database::with_path(
            std::env::temp_dir().join(format!("yd-ttl-{}-{suffix}.sqlite", std::process::id())),
        )
    }

    fn cleanup_database(path: std::path::PathBuf) {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    }
}
