use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use webapi::config::Config;
use webapi::snapshot::Snapshot;
use webapi::status::StatusPayload;
use webapi::{AppState, DEFAULT_ACTIVITY_DAYS, app, auth, db};

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
        // Website-owned schema only: sign-in sessions. The game DB is never written.
        if let Err(err) = auth::migrate(site).await {
            tracing::warn!(error = %err, "could not prepare the session table");
        }
    }

    if !cfg.static_dir.join("200.html").exists() {
        tracing::warn!(
            dir = %cfg.static_dir.display(),
            "frontend build not found — run `npm run build` in web/ (or set WEB_STATIC_DIR)"
        );
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

    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal())
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

async fn shutdown_signal() {
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
    tracing::info!("shutting down");
}
