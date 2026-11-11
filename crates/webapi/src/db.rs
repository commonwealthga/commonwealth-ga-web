//! SQLite access.
//!
//! Two rules, both inherited from the previous site and both load-bearing:
//!
//! 1. The game database is opened **read-only**. Nothing here may write to it;
//!    website-owned writes only ever go to `site.db`.
//! 2. Every pool is small and has a short busy timeout. A stats query never
//!    gets to stall the process while the game server holds a write lock.

use std::path::Path;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

/// Open an existing database file for reading only.
///
/// `query_only` is belt-and-braces on top of `mode=ro`: even a mistakenly
/// writable mount cannot be written through this pool.
pub async fn open_read_only(path: &Path, max_connections: u32) -> sqlx::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(2))
        .pragma("query_only", "true");

    SqlitePoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(2))
        .connect_with(options)
        .await
}

/// Open (creating if needed) the website's own database.
pub async fn open_read_write(path: &Path, max_connections: u32) -> sqlx::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(5))
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
}
