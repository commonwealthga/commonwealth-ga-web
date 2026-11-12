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

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use sqlx::SqlitePool;
use tokio::sync::broadcast;
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
        })
    }
}

/// Assemble the full router: API routes first, then the static frontend with
/// an SPA fallback so client-side routes survive a hard refresh.
/// Assemble the router.
///
/// This process is the API: JSON, the download directory, the redacted
/// database, the sitemap. The pages belong to SvelteKit's Node server, which the
/// edge routes to, so there is no static file serving here and no app shell to
/// fall back to — an unknown path is a 404, which is what a stranger poking at
/// it should get.
pub fn app(state: Arc<AppState>) -> Router {
    Router::new()
        // Nested so the API keeps its own JSON 404 and `/api/*` never wanders.
        .nest("/api", routes::api())
        // Download directory: the launcher depends on these paths, so they are
        // served verbatim, straight from disk, with no restart needed when files
        // are added or replaced.
        .nest_service("/files", files::service(state.cfg.files_dir.clone()))
        // The redacted database, at the root because that is where third parties
        // already look for it (the previous site used the same URL).
        .route("/db-export/{token}", axum::routing::get(export::download))
        // The bare path gets "no such thing" rather than a web page.
        .route("/db-export", axum::routing::get(export::no_token))
        .route("/db-export/", axum::routing::get(export::no_token))
        // Generated here so the page list travels with the release that knows
        // it. The edge sends this path to the API rather than to the site.
        .route(
            "/sitemap.xml",
            axum::routing::get(routes::pages::sitemap_xml),
        )
        .fallback(not_found)
        // A 503 from a degraded route is an expected outcome, not a fault; the
        // error handler already logs genuine failures at ERROR with detail.
        .layer(
            TraceLayer::new_for_http()
                .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
        )
        .with_state(state)
}

/// Anything unrouted: JSON, so a wrong URL reads as an API that does not know
/// the path rather than as a web page gone missing.
async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({
            "error": "not found",
            "hint": "pages are served by another process; this one is the API",
        })),
    )
}
