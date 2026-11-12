//! Commonwealth GA website — public site, JSON API and live feed.
//!
//! The crate is split lib/bin so the router can be built in tests without a
//! listening socket: [`app`] takes the assembled state and returns a `Router`.

pub mod activity;
pub mod agents;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod export;
pub mod files;
pub mod history;
pub mod match_detail;
pub mod matches;
pub mod mmr;
pub mod news;
pub mod routes;
pub mod settings;
pub mod sitemap;
pub mod snapshot;
pub mod status;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{StatusCode, Uri, header};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::trace::{DefaultOnFailure, TraceLayer};

use crate::activity::ActivityService;
use crate::auth::AttemptLimiter;
use crate::config::Config;
use crate::history::HistoryService;
use crate::matches::MatchService;
use crate::snapshot::StatsSource;
use crate::status::StatusService;

/// Number of buffered live events a slow SSE client may fall behind before it
/// starts missing ticks. Missing a tick is fine — the next one is 15s away.
const EVENT_BUFFER: usize = 64;

/// Default window for the player-activity series (also what warm-up loads).
pub const DEFAULT_ACTIVITY_DAYS: u32 = crate::history::DAYS_DEFAULT;

pub struct AppState {
    pub cfg: Config,
    pub status: StatusService,
    pub matches: MatchService,
    /// Recent play, for the homepage hero.
    pub activity: ActivityService,
    pub history: HistoryService,
    /// Failed sign-in attempts, per address.
    pub limiter: AttemptLimiter,
    /// Live game DB — read-only, and only for the player count.
    pub live_pool: Option<SqlitePool>,
    /// Where statistics queries read from: the published snapshot, re-opened
    /// as the snapshotter replaces it, falling back to the live database when
    /// there is none.
    pub stats: StatsSource,
    /// The snapshot itself, for the health endpoint's availability report.
    pub snapshot: Option<Arc<crate::snapshot::Snapshot>>,
    /// Website-owned read-write DB.
    pub site_pool: Option<SqlitePool>,
    /// Fan-out for `/api/live`.
    pub events: broadcast::Sender<String>,
    /// Fired once, on shutdown. Server-sent events end on it instead of holding
    /// the process open: a stream that never finishes is why the server ignored
    /// Ctrl-C.
    pub shutdown: broadcast::Sender<()>,
    /// Stylesheets from the frontend build, for the server-rendered news pages.
    /// Resolved once at startup: a new build arrives with a new container.
    pub stylesheets: Vec<String>,
}

impl AppState {
    /// The database statistics queries read: the published snapshot when there
    /// is one, the live database otherwise.
    ///
    /// This is the whole point of the snapshot: heavy SQL (career aggregation,
    /// rating series, leaderboards) must not run against the file the game
    /// server is writing. Callers that need *current* state — the player count,
    /// a player's own settings — keep using the live pool directly.
    pub async fn stats_pool(&self) -> Option<SqlitePool> {
        self.stats.pool().await
    }

    /// Whether the snapshot is published right now. Asked of the filesystem
    /// rather than remembered from startup, because the snapshotter publishes
    /// after this process is already serving.
    pub async fn snapshot_available(&self) -> bool {
        match &self.snapshot {
            Some(snapshot) => snapshot.available().await,
            None => false,
        }
    }

    /// A state whose statistics read one fixed pool — what tests want, and what
    /// a deployment with no snapshot path gets.
    pub fn new(
        cfg: Config,
        live_pool: Option<SqlitePool>,
        snapshot_pool: Option<SqlitePool>,
        site_pool: Option<SqlitePool>,
    ) -> Arc<Self> {
        let stats = StatsSource::from(snapshot_pool.clone().or_else(|| live_pool.clone()));
        Self::build(cfg, live_pool, stats, None, site_pool)
    }

    /// Production: statistics follow the published snapshot file.
    pub fn with_snapshot(
        cfg: Config,
        live_pool: Option<SqlitePool>,
        snapshot: Option<Arc<crate::snapshot::Snapshot>>,
        site_pool: Option<SqlitePool>,
    ) -> Arc<Self> {
        let stats = match &snapshot {
            Some(handle) => StatsSource::snapshot(Arc::clone(handle), live_pool.clone()),
            None => StatsSource::from(live_pool.clone()),
        };
        Self::build(cfg, live_pool, stats, snapshot, site_pool)
    }

    fn build(
        cfg: Config,
        live_pool: Option<SqlitePool>,
        stats: StatsSource,
        snapshot: Option<Arc<crate::snapshot::Snapshot>>,
        site_pool: Option<SqlitePool>,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let (shutdown, _) = broadcast::channel(1);
        let stylesheets = find_stylesheets(&cfg.static_dir);
        let control_addr = cfg.control_addr();
        let status = StatusService::new(
            live_pool.clone(),
            cfg.status_ttl,
            cfg.status_override.clone(),
            control_addr,
        );
        // Changes no faster than the snapshot itself, so a minute of caching is
        // invisible to a reader and keeps the hero cheap under load.
        let activity = ActivityService::new(stats.clone(), Duration::from_secs(60));
        let matches = MatchService::new(stats.clone(), cfg.match_ttl);
        let history = HistoryService::new(stats.clone(), cfg.history_ttl);
        Arc::new(Self {
            cfg,
            status,
            matches,
            activity,
            history,
            limiter: AttemptLimiter::default(),
            live_pool,
            stats,
            snapshot,
            site_pool,
            events,
            shutdown,
            stylesheets,
        })
    }
}

/// Every CSS file the frontend build emitted, as URL paths.
///
/// SvelteKit content-hashes asset names, so the news pages cannot hard-code one;
/// a long cache is safe precisely because the name changes with the content.
fn find_stylesheets(static_dir: &Path) -> Vec<String> {
    let dir = static_dir.join("_app/immutable/assets");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "css"))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .map(|name| format!("/_app/immutable/assets/{name}"))
        .collect();
    found.sort();
    found
}

/// Assemble the full router: API routes first, then the static frontend with
/// an SPA fallback so client-side routes survive a hard refresh.
pub fn app(state: Arc<AppState>) -> Router {
    let static_dir = state.cfg.static_dir.clone();
    // SvelteKit's `adapter-static` emits prerendered pages as real files plus a
    // `200.html` app shell. Falling back to the shell (not `index.html`) keeps
    // the prerendered landing page intact while still letting client-side
    // routes survive a hard refresh.
    //
    // Compression is scoped to the static assets on purpose: a compression layer
    // in front of the SSE stream would buffer it and defeat the whole point of
    // server-sent events.
    let assets = ServiceBuilder::new()
        .layer(CompressionLayer::new())
        .service(ServeDir::new(&static_dir));

    Router::new()
        // Nested so the API keeps its own JSON 404 and `/api/*` never reaches
        // the file server.
        .nest("/api", routes::api())
        // Download directory: the launcher depends on these paths, so they are
        // served verbatim, straight from disk, with no restart needed when
        // files are added or replaced.
        .nest_service("/files", files::service(state.cfg.files_dir.clone()))
        // The redacted database, at the root because that is where third
        // parties already look for it (the previous site used the same URL).
        // A wrong token must never fall through to the SPA shell.
        .route("/db-export/{token}", axum::routing::get(export::download))
        // The bare path must not fall through to the app shell: a stranger
        // poking at it should get "no such thing", not a web page.
        .route("/db-export", axum::routing::get(export::no_token))
        // Server-rendered pages, so a post written this morning is crawlable
        // this morning (the SvelteKit build is static).
        .route("/news", axum::routing::get(routes::pages::feed))
        // The next batch of entries, as HTML: the feed's endless scroll.
        .route("/news/cards", axum::routing::get(routes::pages::cards))
        .route("/news/{*rest}", axum::routing::get(routes::pages::post))
        .route(
            "/sitemap.xml",
            axum::routing::get(routes::pages::sitemap_xml),
        )
        .route("/db-export/", axum::routing::get(export::no_token))
        .fallback_service(assets)
        .layer(from_fn_with_state(Arc::clone(&state), html_suffix))
        // A 503 from a degraded route is an expected outcome, not a fault; the
        // error handler already logs genuine failures at ERROR with detail.
        .layer(
            TraceLayer::new_for_http()
                .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
        )
        .with_state(state)
}

/// Serve `<path>.html` for extensionless paths.
///
/// `adapter-static` writes prerendered pages as `server.html`, `matches.html`,
/// and so on. Without this, `/server` misses the file and falls through to the
/// `200.html` shell — which only renders with JavaScript, is invisible to
/// crawlers, and leaves a blank first paint. Assets (anything with an
/// extension) and the API are untouched.
async fn html_suffix(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let is_read = matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    );
    let path = request.uri().path().to_string();
    let wants_html = is_read
        && path.starts_with('/')
        && !path.starts_with("/api/")
        && !path.starts_with("/files/")
        && !path.starts_with("/db-export")
        && path != "/"
        && !path.rsplit('/').next().unwrap_or_default().contains('.');

    if wants_html {
        // `adapter-static` writes prerendered pages as `server.html`, so an
        // extensionless path has to be pointed at its real file. Without this,
        // `/server` misses the file and is treated as unrouted.
        let candidate = state.cfg.static_dir.join(format!(
            "{}.html",
            path.trim_end_matches('/').trim_start_matches('/')
        ));
        if tokio::fs::metadata(&candidate).await.is_ok() {
            let mut parts = request.uri().clone().into_parts();
            parts.path_and_query = Some(
                format!("{}.html", path.trim_end_matches('/'))
                    .parse()
                    .expect("static html path is valid"),
            );
            if let Ok(uri) = Uri::from_parts(parts) {
                *request.uri_mut() = uri;
            }
        }
    }

    let response = next.run(request).await;

    // A miss on a page-shaped GET is not always a 404: routes that render on the
    // client (agent profiles, missions, the leaderboard) exist only as the app
    // shell. Serve that shell — but keep the status honest, so an unknown path
    // ends up a real 404 for a crawler while the app still draws its error page.
    if wants_html && response.status() == StatusCode::NOT_FOUND {
        let status = if is_client_route(&path) {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        };
        return shell_response(&state.cfg.static_dir, status).await;
    }

    response
}

/// Routes that exist only in the browser bundle.
///
/// Everything else is either a prerendered file or a 404, and telling the two
/// apart is what stops this site from answering 200 for every typo — the
/// difference between a crawler indexing nine pages and indexing infinite
/// garbage.
fn is_client_route(path: &str) -> bool {
    // Depth-zero routes plus the two detail routes. Prefixes include the
    // trailing slash on purpose: matching "/agents" as a prefix made
    // "/agents-are-not-real" a route, and a bare `starts_with` on a path is how
    // that happens.
    const EXACT: [&str; 4] = ["/agents", "/leaderboard", "/settings", "/me"];
    const PREFIXES: [&str; 2] = ["/agents/", "/missions/"];
    let path = path.trim_end_matches('/');
    let path = if path.is_empty() { "/" } else { path };
    EXACT.contains(&path) || PREFIXES.iter().any(|prefix| path.starts_with(prefix))
}

/// The app shell, at whatever status the caller decided.
async fn shell_response(static_dir: &Path, status: StatusCode) -> Response {
    match tokio::fs::read(static_dir.join("200.html")).await {
        Ok(body) => (
            status,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            body,
        )
            .into_response(),
        Err(error) => {
            // No shell on disk is a deployment fault, not a routing one.
            tracing::error!(%error, "app shell missing");
            (status, "not found").into_response()
        }
    }
}
