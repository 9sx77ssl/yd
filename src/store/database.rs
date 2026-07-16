use std::path::{Path, PathBuf};

use color_eyre::eyre::{eyre, Result, WrapErr};
use directories::ProjectDirs;
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};

/// A single SQLite database backing every persisted module.
///
/// Each domain registers its own migrations through [`Database::migrate`];
/// the shared `schema_migrations` ledger ensures every statement runs exactly
/// once, even when several modules share the same file.
#[derive(Clone)]
pub struct Database {
    database_path: PathBuf,
}

impl Database {
    /// Opens (creating if necessary) the shared `yd.sqlite` database.
    pub fn open() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "yd", "yd")
            .ok_or_else(|| eyre!("could not determine the application data directory"))?;
        std::fs::create_dir_all(dirs.data_local_dir())
            .wrap_err("could not create application data directory")?;
        Ok(Self {
            database_path: dirs.data_local_dir().join("yd.sqlite"),
        })
    }

    /// Opens a database at an explicit path (used by tests).
    #[allow(dead_code)]
    pub fn with_path(database_path: PathBuf) -> Self {
        Self { database_path }
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Connects with the standard PRAGMA profile and applies `migrations`.
    pub async fn connect(&self) -> Result<SqliteConnection> {
        let options = SqliteConnectOptions::new()
            .filename(&self.database_path)
            .create_if_missing(true);
        let mut connection = SqliteConnection::connect_with(&options).await?;
        configure_database(&mut connection).await?;
        Ok(connection)
    }

    /// Applies `migrations` inside the shared schema ledger.
    ///
    /// Each migration runs in a single transaction; failures roll back and
    /// leave the ledger untouched so the next run can retry.
    pub async fn migrate(&self, migrations: &[Migration]) -> Result<()> {
        let mut connection = self.connect().await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY)")
            .execute(&mut connection)
            .await?;

        for migration in migrations {
            let applied: Option<(i64,)> =
                sqlx::query_as("SELECT version FROM schema_migrations WHERE version = ?")
                    .bind(migration.version)
                    .fetch_optional(&mut connection)
                    .await?;
            if applied.is_some() {
                continue;
            }

            sqlx::query("BEGIN IMMEDIATE")
                .execute(&mut connection)
                .await?;
            for statement in migration.statements {
                if let Err(error) = sqlx::query(statement).execute(&mut connection).await {
                    let _ = sqlx::query("ROLLBACK").execute(&mut connection).await;
                    return Err(error.into());
                }
            }
            if let Err(error) = sqlx::query("INSERT INTO schema_migrations (version) VALUES (?)")
                .bind(migration.version)
                .execute(&mut connection)
                .await
            {
                let _ = sqlx::query("ROLLBACK").execute(&mut connection).await;
                return Err(error.into());
            }
            sqlx::query("COMMIT").execute(&mut connection).await?;
        }

        Ok(())
    }
}

/// A versioned migration applied through [`Database::migrate`].
#[derive(Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub statements: &'static [&'static str],
}

impl Migration {
    pub const fn new(version: i64, statements: &'static [&'static str]) -> Self {
        Self {
            version,
            statements,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn migrations_are_idempotent() {
        let path = temp_database_path("migrations-idempotent");
        let database = Database::with_path(path.clone());

        let migration = Migration::new(999, &["CREATE TABLE IF NOT EXISTS marker (x INTEGER)"]);
        database.migrate(&[migration]).await.unwrap();
        // Applying the same migration again must be a no-op.
        database.migrate(&[migration]).await.unwrap();

        let mut connection = database.connect().await.unwrap();
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM schema_migrations WHERE version = 999")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert_eq!(count.0, 1);

        cleanup_database(path);
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
