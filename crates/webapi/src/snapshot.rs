//! The published snapshot, and the stats queries that read it.
//!
//! The snapshotter publishes by writing `snapshot.db.tmp` and renaming it over
//! `snapshot.db`. A SQLite connection opened before that rename keeps reading
//! the inode it was opened on — the file it was pointed at, not the path. So a
//! pool created at startup serves the snapshot from boot forever, and if the
//! file did not exist yet at startup there is no pool at all.
//!
//! Both were happening in production: the web container started before the
//! snapshotter's first write, so `/api/health` reported the snapshot as absent
//! and every statistics query silently fell back to the live game database — the
//! exact load the snapshot exists to prevent.
//!
//! So the pool is not held, it is *checked*: on each use the file's identity is
//! compared with the one the pool was opened on, and the pool is re-opened when
//! it changed. One `stat` per query is cheap next to the aggregation SQL that
//! follows it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use sqlx::SqlitePool;

use crate::db;

/// How many connections a snapshot pool may open.
const SNAPSHOT_CONNECTIONS: u32 = 4;

pub struct Snapshot {
    path: PathBuf,
    open: tokio::sync::Mutex<Option<Open>>,
}

struct Open {
    stamp: Stamp,
    pool: SqlitePool,
}

/// What identifies the file a pool was opened on. The inode changes when the
/// snapshotter renames a new file into place, which is the signal we want;
/// mtime and size stand in on platforms without inodes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Stamp {
    ino: u64,
    modified_ns: u128,
    size: u64,
}

impl Snapshot {
    pub fn new(path: impl Into<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            path: path.into(),
            open: tokio::sync::Mutex::new(None),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A pool for the file as it is *now*, re-opening if it was replaced.
    ///
    /// `None` means the snapshot is not published (yet): callers fall back to
    /// the live database, as they did when the path was absent entirely.
    pub async fn pool(&self) -> Option<SqlitePool> {
        let stamp = match tokio::fs::metadata(&self.path).await {
            Ok(meta) => stamp(&meta),
            Err(_) => return None,
        };

        let mut open = self.open.lock().await;
        if let Some(current) = open.as_ref()
            && current.stamp == stamp
        {
            return Some(current.pool.clone());
        }

        match db::open_read_only(&self.path, SNAPSHOT_CONNECTIONS).await {
            Ok(pool) => {
                *open = Some(Open {
                    stamp,
                    pool: pool.clone(),
                });
                Some(pool)
            }
            Err(error) => {
                // Keep serving whatever we had; a stale snapshot beats none.
                tracing::warn!(
                    path = %self.path.display(),
                    %error,
                    "could not open the published snapshot"
                );
                None
            }
        }
    }

    /// Whether a snapshot is published, for the health endpoint.
    pub async fn available(&self) -> bool {
        tokio::fs::metadata(&self.path).await.is_ok()
    }
}

fn stamp(meta: &std::fs::Metadata) -> Stamp {
    #[cfg(unix)]
    let ino = {
        use std::os::unix::fs::MetadataExt;
        meta.ino()
    };
    #[cfg(not(unix))]
    let ino = 0;

    let modified_ns = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_nanos())
        .unwrap_or(0);

    Stamp {
        ino,
        modified_ns,
        size: meta.len(),
    }
}

/// Where statistics queries should read from.
///
/// A deployment with a snapshot path gets [`StatsSource::Snapshot`], which
/// follows the published file and falls back to the live database while there is
/// none. Tests and fixed deployments get a plain pool.
#[derive(Clone)]
pub enum StatsSource {
    Snapshot {
        snapshot: Arc<Snapshot>,
        fallback: Option<SqlitePool>,
    },
    Fixed(Option<SqlitePool>),
}

impl StatsSource {
    pub fn snapshot(snapshot: Arc<Snapshot>, fallback: Option<SqlitePool>) -> Self {
        Self::Snapshot { snapshot, fallback }
    }

    /// The pool to run a statistics query against, if there is one.
    pub async fn pool(&self) -> Option<SqlitePool> {
        match self {
            Self::Snapshot { snapshot, fallback } => {
                snapshot.pool().await.or_else(|| fallback.clone())
            }
            Self::Fixed(pool) => pool.clone(),
        }
    }
}

impl From<Option<SqlitePool>> for StatsSource {
    fn from(pool: Option<SqlitePool>) -> Self {
        Self::Fixed(pool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rewritten_file_gets_a_new_stamp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("snapshot.db");

        std::fs::write(&path, b"first").expect("write");
        let before = stamp(&std::fs::metadata(&path).expect("meta"));

        // The snapshotter's pattern: write a sibling, then rename over.
        let temp = dir.path().join("snapshot.db.tmp");
        std::fs::write(&temp, b"second version").expect("write temp");
        std::fs::rename(&temp, &path).expect("rename");

        let after = stamp(&std::fs::metadata(&path).expect("meta"));
        assert_ne!(before, after, "a replaced file must look different");
    }
}
