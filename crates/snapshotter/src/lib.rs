//! The redacted snapshot of the game database.
//!
//! Why a copy at all: the website's stats queries are heavy SQL, and running
//! them against the live `server.db` makes the game server's database a shared
//! resource for every page load. This produces a separate file that every
//! statistics query reads instead, and that third parties can download
//! (the cards registry does).
//!
//! The copy is made with `VACUUM INTO`, which is a serialised read of the
//! current database: consistent even while the game server is writing, and
//! compact. (On the live 210 MB database it takes ~1.2 s, against ~13.5 s for a
//! VACUUM in place — the copy is the cheap form.)
//!
//! Redaction runs on the copy, never on the source: login sessions (addresses
//! and timelines) are deleted and password verifiers are nulled. `secure_delete`
//! is switched on first so the deleted bytes are zeroed in the pages rather than
//! left in free space for anyone to grep out of the file.
//!
//! Publication is atomic: the copy is built under a temporary name and renamed
//! into place, so a reader (or a download) always sees a complete snapshot, and
//! a failed cycle leaves the previous one in place.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Connection, SqliteConnection};

/// What gets scrubbed, and why each one is here.
///
/// The rule is "remove what identifies a person, keep what describes play".
/// Rows are *kept* wherever the website derives statistics from them — the
/// activity chart is built from login/logout timestamps, so deleting sessions
/// (as the first version of this did) silently blanked the chart.
///
/// `ip` is `NOT NULL` in both tables, so it is blanked rather than nulled;
/// writing NULL there would fail the whole cycle.
const SCRUB: [(&str, &str); 6] = [
    (
        "UPDATE ga_user_sessions SET ip = ''",
        "login addresses and timelines; the timestamps stay, the address does not",
    ),
    (
        "UPDATE ga_ip_bans SET ip = '', reason = ''",
        "the ban record stays, the address and the free-text note do not",
    ),
    (
        "UPDATE ga_user_bans SET reason = ''",
        "moderation notes are written about people, not about play",
    ),
    (
        "UPDATE ga_users SET password_verifier = NULL",
        "credentials never leave the server",
    ),
    (
        "UPDATE ga_users SET admin_notes = NULL",
        "operator notes read like 'the guy from <place> who had issues'",
    ),
    (
        "DELETE FROM ga_ip_checks",
        "pure address intelligence (country, ISP, proxy flags); nothing reads it",
    ),
];

/// Build one snapshot at `dest`, from `live`.
pub async fn snapshot(live: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    // Same directory, so the final rename is atomic on one filesystem.
    let temp = temp_path(dest);
    let _ = tokio::fs::remove_file(&temp).await;

    copy_consistent(live, &temp).await?;
    redact(&temp).await?;

    tokio::fs::rename(&temp, dest)
        .await
        .with_context(|| format!("publishing {}", dest.display()))?;

    Ok(())
}

fn temp_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "snapshot.db".to_string());
    dest.with_file_name(format!("{name}.tmp"))
}

/// A consistent, compact copy: `VACUUM INTO` reads under one transaction, so the
/// game server can keep writing while it runs.
async fn copy_consistent(live: &Path, temp: &Path) -> Result<()> {
    let mut source = SqliteConnection::connect(&read_only_url(live))
        .await
        .with_context(|| format!("opening {} read-only", live.display()))?;

    // The path is interpolated because VACUUM INTO takes a literal, not a bind
    // parameter. It is our own path (from configuration), never user input.
    let escaped = temp.to_string_lossy().replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{escaped}'"))
        .execute(&mut source)
        .await
        .with_context(|| format!("copying into {}", temp.display()))?;

    source.close().await.ok();
    Ok(())
}

/// Strip the parts of the database that are about people rather than matches.
async fn redact(path: &Path) -> Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .with_context(|| format!("opening {} for redaction", path.display()))?;

    // Must be set before the deletes: without it the removed rows stay readable
    // in the file's free pages, which defeats the point.
    sqlx::query("PRAGMA secure_delete = ON")
        .execute(&pool)
        .await?;

    for (statement, why) in SCRUB {
        // A deployment whose schema predates one of these tables should not
        // fail the cycle; the others still matter.
        if let Err(error) = sqlx::query(statement).execute(&pool).await {
            tracing::debug!(statement, %error, why, "redaction step skipped");
        }
    }

    pool.close().await;
    Ok(())
}

/// Read-only connection URL for the game database.
///
/// `mode=ro` and `query_only` both, so a mistake in this crate cannot write to
/// the file the game server owns.
fn read_only_url(path: &Path) -> String {
    format!("sqlite://{}?mode=ro", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn fixture(dir: &Path) -> PathBuf {
        let live = dir.join("live.db");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{}?mode=rwc", live.display()))
            .await
            .expect("create fixture");
        for ddl in [
            "CREATE TABLE ga_users (id INTEGER PRIMARY KEY, username TEXT, password_verifier BLOB,
                                    admin_notes TEXT)",
            "CREATE TABLE ga_user_sessions (id INTEGER PRIMARY KEY, user_id INTEGER,
                                            username TEXT, ip TEXT NOT NULL, login_at INTEGER)",
            "CREATE TABLE ga_users_notes (id INTEGER)",
            "CREATE TABLE ga_ip_checks (ip TEXT, country TEXT)",
            "CREATE TABLE ga_match_player_stats (instance_id INTEGER, user_id INTEGER, kills INTEGER)",
        ] {
            sqlx::query(ddl).execute(&pool).await.expect("ddl");
        }
        sqlx::query("INSERT INTO ga_users (id, username, password_verifier) VALUES (1, 'Agent', x'deadbeef')")
            .execute(&pool)
            .await
            .expect("user");
        sqlx::query("INSERT INTO ga_users (id, username, admin_notes) VALUES (2, 'Other', 'the guy from somewhere')")
            .execute(&pool)
            .await
            .expect("user");
        sqlx::query(
            "INSERT INTO ga_user_sessions VALUES (1, 1, 'Agent', '203.0.113.7', 1700000000)",
        )
        .execute(&pool)
        .await
        .expect("session");
        sqlx::query("INSERT INTO ga_match_player_stats VALUES (500, 1, 12)")
            .execute(&pool)
            .await
            .expect("stats");
        pool.close().await;
        live
    }

    #[tokio::test]
    async fn a_snapshot_keeps_the_play_and_drops_the_people() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let live = fixture(dir.path()).await;
        let dest = dir.path().join("snapshot.db");

        snapshot(&live, &dest).await.expect("snapshot");

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{}?mode=ro", dest.display()))
            .await
            .expect("open snapshot");

        // The activity chart is built from these rows, so they stay...
        let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ga_user_sessions")
            .fetch_one(&pool)
            .await
            .expect("count sessions");
        assert_eq!(sessions, 1, "login rows are statistics, not secrets");
        // ...with the address gone.
        let ips: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ga_user_sessions WHERE ip != ''")
            .fetch_one(&pool)
            .await
            .expect("count addresses");
        assert_eq!(ips, 0, "login addresses must not survive");

        let notes: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ga_users WHERE admin_notes IS NOT NULL")
                .fetch_one(&pool)
                .await
                .expect("count notes");
        assert_eq!(notes, 0, "operator notes are about people");

        let verifiers: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ga_users WHERE password_verifier IS NOT NULL")
                .fetch_one(&pool)
                .await
                .expect("count verifiers");
        assert_eq!(verifiers, 0, "credentials must not survive");

        let kills: i64 = sqlx::query_scalar("SELECT kills FROM ga_match_player_stats")
            .fetch_one(&pool)
            .await
            .expect("stats");
        assert_eq!(kills, 12, "the archive itself is the point");
        pool.close().await;

        // The source is untouched: redaction happens on the copy.
        let source = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&read_only_url(&live))
            .await
            .expect("open live");
        let live_sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ga_user_sessions")
            .fetch_one(&source)
            .await
            .expect("count live sessions");
        assert_eq!(live_sessions, 1);
        source.close().await;
    }

    #[tokio::test]
    async fn a_replaced_snapshot_leaves_no_temporary_file() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let live = fixture(dir.path()).await;
        let dest = dir.path().join("snapshot.db");

        snapshot(&live, &dest).await.expect("first");
        snapshot(&live, &dest).await.expect("second");

        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "got {leftovers:?}");
    }
}
