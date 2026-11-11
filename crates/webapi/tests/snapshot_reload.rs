//! The published snapshot is replaced while the process runs.
//!
//! The snapshotter writes `snapshot.db.tmp` and renames it over `snapshot.db`
//! every five minutes. A pool opened once at startup keeps the inode it was
//! opened on, so statistics served from it silently froze at boot — and a pool
//! opened before the first publish failed outright and fell back to the live
//! game database, which is the load the snapshot exists to keep off it.

use std::path::Path;

use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use webapi::snapshot::Snapshot;

/// A database with `rows` rows in a one-column table.
async fn write_db(path: &Path, rows: i64) {
    let _ = std::fs::remove_file(path);
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(options).await.expect("open");
    sqlx::query("CREATE TABLE IF NOT EXISTS t (n INTEGER)")
        .execute(&pool)
        .await
        .expect("schema");
    for _ in 0..rows {
        sqlx::query("INSERT INTO t (n) VALUES (1)")
            .execute(&pool)
            .await
            .expect("row");
    }
    pool.close().await;
}

async fn count(pool: SqlitePool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM t")
        .fetch_one(&pool)
        .await
        .expect("count")
}

#[tokio::test]
async fn a_replaced_snapshot_is_picked_up() {
    let dir = tempfile::tempdir().expect("tempdir");
    let published = dir.path().join("snapshot.db");
    write_db(&published, 1).await;

    let snapshot = Snapshot::new(&published);
    assert_eq!(count(snapshot.pool().await.expect("pool")).await, 1);

    // Exactly what the snapshotter does: a sibling file, then a rename.
    let temp = dir.path().join("snapshot.db.tmp");
    write_db(&temp, 3).await;
    std::fs::rename(&temp, &published).expect("rename");

    assert_eq!(
        count(snapshot.pool().await.expect("pool")).await,
        3,
        "the next query must read the file that was published, not the one open at startup"
    );
}

#[tokio::test]
async fn nothing_is_published_yet_is_not_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let snapshot = Snapshot::new(dir.path().join("snapshot.db"));

    assert!(snapshot.pool().await.is_none(), "no file, no pool");
    assert!(!snapshot.available().await);

    // The snapshotter's first write arrives later; the handle must pick it up
    // without a restart, which is what makes the health flag honest.
    write_db(&dir.path().join("snapshot.db"), 2).await;

    assert!(snapshot.available().await);
    assert_eq!(count(snapshot.pool().await.expect("pool")).await, 2);
}
