//! The snapshot loop.
//!
//! Runs beside the website and rewrites `snapshot.db` every interval. Port 0 of
//! the operation: everything here degrades to "log it and try again next cycle",
//! because a missed snapshot means stale statistics, not a broken site.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use snapshotter::snapshot;

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let live = env_path("WEB_DB_LIVE").context("WEB_DB_LIVE must point at the game database")?;
    let dest =
        env_path("WEB_DB_SNAPSHOT").context("WEB_DB_SNAPSHOT must point at the file to publish")?;
    let interval = std::env::var("WEB_SNAPSHOT_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(300);
    let once = std::env::args().any(|arg| arg == "--once");

    tracing::info!(live = %live.display(), dest = %dest.display(), interval, once, "snapshotter starting");

    loop {
        match snapshot(&live, &dest).await {
            Ok(()) => {
                let size = tokio::fs::metadata(&dest)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                tracing::info!(bytes = size, "snapshot published");
            }
            Err(error) => {
                // Keep the previous snapshot: readers keep working, just on
                // older data, which is the least bad failure here.
                tracing::error!(%error, "snapshot failed; keeping the previous one");
                if once {
                    return Err(error);
                }
            }
        }

        if once {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}
