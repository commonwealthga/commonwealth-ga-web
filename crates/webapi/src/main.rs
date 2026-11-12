use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use webapi::config::Config;
use webapi::news;
use webapi::snapshot::Snapshot;
use webapi::status::StatusPayload;
use webapi::{AppState, DEFAULT_ACTIVITY_DAYS, app, auth, db, guides};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn")),
        )
        .init();

    let cfg = Config::from_env();
    tracing::info!(bind = %cfg.bind, "starting webapi");

    // A missing DB is not fatal: the site still serves, and the affected routes
    // report 503 instead of taking the process down.
    let live_pool = match cfg.live_db.as_deref() {
        Some(path) => report(
            "live",
            path,
            db::open_read_only(path, POOL_CONNECTIONS).await,
        ),
        None => None,
    };
    // Not opened here: the snapshotter publishes after this process starts, and
    // a pool opened at startup pins whichever file existed then (or none). The
    // handle re-opens as the file is replaced — see snapshot.rs.
    let snapshot = cfg.snapshot_db.as_deref().map(Snapshot::new);
    if let Some(handle) = &snapshot {
        // Opened once here, before any stats query runs, so a deployment with a
        // missing or unreadable snapshot says so at boot rather than on the
        // first page view.
        report(
            "snapshot",
            handle.path(),
            db::open_read_only(handle.path(), POOL_CONNECTIONS).await,
        );
    }
    let site_pool = match cfg.site_db.as_deref() {
        Some(path) => report(
            "site",
            path,
            db::open_read_write(path, POOL_CONNECTIONS).await,
        ),
        None => None,
    };
    if let Some(site) = &site_pool {
        // Website-owned schema only: sign-in sessions, the news tables and the
        // guides tables. The game DB is never written.
        if let Err(err) = auth::migrate(site).await {
            tracing::warn!(error = %err, "could not prepare the session table");
        }
        if let Err(err) = news::store::migrate(site).await {
            tracing::warn!(error = %err, "could not prepare the news tables");
        }
        if let Err(err) = guides::store::migrate(site).await {
            tracing::warn!(error = %err, "could not prepare the guides tables");
        }
        // Tiles arrived after the first builds were posted, so fill in the ones
        // that predate them. Bounded: a handful per start, and a no-op once done.
        match guides::store::image_hashes_missing_tiles(site, &cfg.images_dir, 50).await {
            Ok(hashes) if !hashes.is_empty() => {
                let dir = cfg.images_dir.clone();
                let made = tokio::task::spawn_blocking(move || {
                    hashes
                        .iter()
                        .filter(|hash| guides::images::ensure_tile(&dir, hash).unwrap_or(false))
                        .count()
                })
                .await
                .unwrap_or(0);
                tracing::info!(count = made, "generated missing build image tiles");
            }
            Ok(_) => {}
            Err(err) => tracing::warn!(error = %err, "could not check for missing image tiles"),
        }
    }

    if let Some(path) = &cfg.status_override {
        tracing::info!(file = %path.display(), "status override file (absent until created)");
    }

    let state = AppState::with_snapshot(cfg.clone(), live_pool, snapshot, site_pool);
    spawn_status_ticker(Arc::clone(&state));
    warm_caches(Arc::clone(&state));

    let listener = TcpListener::bind(cfg.bind)
        .await
        .with_context(|| format!("binding {}", cfg.bind))?;
    tracing::info!(addr = %listener.local_addr()?, "listening");

    axum::serve(listener, app(Arc::clone(&state)))
        .with_graceful_shutdown(shutdown_signal(Arc::clone(&state)))
        .await
        .context("server error")?;
    Ok(())
}

/// Connections per pool. Stats queries are few and indexed; the game server
/// needs the file more than we do.
const POOL_CONNECTIONS: u32 = 4;

/// Log the outcome of opening a database, and treat failure as "feature off"
/// rather than "process down".
fn report(
    name: &str,
    path: &Path,
    result: sqlx::Result<sqlx::SqlitePool>,
) -> Option<sqlx::SqlitePool> {
    match result {
        Ok(pool) => {
            tracing::info!(db = name, path = %path.display(), "database opened");
            Some(pool)
        }
        Err(err) => {
            tracing::warn!(db = name, path = %path.display(), error = %err, "database unavailable");
            None
        }
    }
}

/// One query per tick for the whole process, fanned out to every SSE client.
fn spawn_status_ticker(state: Arc<AppState>) {
    let tick = state.cfg.live_tick;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tick);
        loop {
            interval.tick().await;
            let payload = match state.status.refresh().await {
                Ok(status) => StatusPayload::from_status(&status),
                Err(err) => {
                    tracing::warn!(error = %err, "live status refresh failed");
                    StatusPayload::unavailable()
                }
            };
            // No subscribers is normal, not an error.
            if let Ok(json) = serde_json::to_string(&payload) {
                let _ = state.events.send(json);
            }
        }
    });
}

/// Fill the expensive caches before anyone asks for them.
///
/// The activity series walks every recorded session and the archive query hits
/// the game database; without this the first visitor after a deploy pays for
/// both. Warm-up is best effort — failures are logged and simply retried on the
/// next request.
fn warm_caches(state: Arc<AppState>) {
    tokio::spawn(async move {
        match state.history.series(crate::DEFAULT_ACTIVITY_DAYS).await {
            Ok(series) => tracing::info!(buckets = series.times.len(), "activity cache warmed"),
            Err(err) => tracing::warn!(error = %err, "activity warm-up failed"),
        }
        match state.matches.page(webapi::matches::Kind::All, 1, 0).await {
            Ok((_, rows)) => tracing::info!(rows = rows.len(), "archive cache warmed"),
            Err(err) => tracing::warn!(error = %err, "archive warm-up failed"),
        }
    });
}

/// How long connections get to drain before the process gives up and exits.
/// Short on purpose: this is a web server, not a database — nothing needs more.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

async fn shutdown_signal(state: Arc<AppState>) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("stopping — draining connections; Ctrl-C again exits immediately");

    // Server-sent events are told to hang up, or they would keep the process
    // alive: an SSE response never ends by itself.
    let _ = state.shutdown.send(());

    // A second interrupt is an unambiguous "now". Registered after the first
    // signal on purpose: tokio replaces the default SIGINT action, so without
    // this the terminal looks deaf to further Ctrl-C.
    tokio::spawn(async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::warn!("second interrupt — exiting now");
        std::process::exit(130);
    });

    // And a backstop, for whatever else refuses to drain.
    tokio::spawn(async {
        tokio::time::sleep(SHUTDOWN_GRACE).await;
        tracing::warn!(
            "connections did not drain within {}s — exiting anyway",
            SHUTDOWN_GRACE.as_secs()
        );
        std::process::exit(0);
    });
    tracing::info!("shutting down");
}
