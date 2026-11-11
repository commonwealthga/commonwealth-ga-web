//! `/api/activity` — the homepage hero's numbers.
//!
//! The behaviour worth pinning is the window ladder: a quiet server must not
//! report zeroes because nobody played *today*, and the label the page prints
//! comes from the same response, so the two cannot disagree.

use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;
use webapi::config::Config;
use webapi::{AppState, app, db};

struct Harness {
    dir: TempDir,
}

impl Harness {
    fn new() -> Self {
        Self {
            dir: TempDir::new().expect("temp dir"),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// One match, `hours_ago` before now, with two agents in it.
    async fn router(&self, hours_ago: Option<i64>) -> Router {
        let live_path = self.path("live.db");
        let live = db::open_read_write(&live_path, 1).await.expect("live db");

        for ddl in [
            "CREATE TABLE ga_instances (id INTEGER PRIMARY KEY, instance_id INTEGER NOT NULL,
                                        map_name TEXT NOT NULL, game_mode TEXT NOT NULL,
                                        started_at INTEGER NOT NULL, sealed_at INTEGER,
                                        outcome TEXT, winning_task_force INTEGER,
                                        is_home_map INTEGER NOT NULL DEFAULT 0, queue_id INTEGER)",
            "CREATE TABLE ga_match_player_stats (instance_id INTEGER NOT NULL, user_id INTEGER NOT NULL,
                                                 character_id INTEGER NOT NULL, task_force INTEGER)",
        ] {
            sqlx::query(ddl).execute(&live).await.expect("ddl");
        }

        if let Some(hours) = hours_ago {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_secs() as i64;
            let started = now - hours * 3600;
            sqlx::query(
                "INSERT INTO ga_instances VALUES (1, 700, 'Map_A', 'TgGame_Mission', ?, ?, 'ATTACKERS_WIN', 1, 0, 1)",
            )
            .bind(started)
            .bind(started + 600)
            .execute(&live)
            .await
            .expect("instance");
            for user in [11_i64, 12] {
                sqlx::query("INSERT INTO ga_match_player_stats VALUES (700, ?, 1, 1)")
                    .bind(user)
                    .execute(&live)
                    .await
                    .expect("stats");
            }
        }

        live.close().await;
        let pool = db::open_read_only(&live_path, 1).await.expect("open");

        let cfg = Config {
            bind: "127.0.0.1:0".parse().expect("addr"),
            live_db: Some(live_path),
            snapshot_db: None,
            site_db: None,
            static_dir: self.path("static"),
            files_dir: self.path("files"),
            status_ttl: Duration::from_secs(0),
            live_tick: Duration::from_secs(15),
            match_ttl: Duration::from_secs(0),
            history_ttl: Duration::from_secs(0),
            status_override: None,
            control_api_url: None,
            control_api_token: None,
            db_export_token: None,
        };
        app(AppState::new(cfg, Some(pool), None, None))
    }
}

async fn get(router: &Router) -> (StatusCode, Value) {
    let request = Request::builder()
        .uri("/api/activity")
        .body(Body::empty())
        .expect("request");
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn play_today_is_reported_over_twenty_four_hours() {
    let harness = Harness::new();
    let router = harness.router(Some(2)).await;

    let (status, body) = get(&router).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["window_hours"], 24);
    assert_eq!(body["agents"], 2);
    assert_eq!(body["missions"], 1);
}

#[tokio::test]
async fn a_quiet_day_widens_the_window_instead_of_reporting_zero() {
    let harness = Harness::new();
    // Last played 30 hours ago: nothing in 24h, so the answer covers 48h.
    let router = harness.router(Some(30)).await;

    let (status, body) = get(&router).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["window_hours"], 48,
        "the window widens to find activity"
    );
    assert_eq!(body["agents"], 2);
    assert_eq!(body["missions"], 1);
}

#[tokio::test]
async fn an_empty_archive_reports_all_time_and_zeroes() {
    let harness = Harness::new();
    let router = harness.router(None).await;

    let (status, body) = get(&router).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["window_hours"], 0, "all time is the last resort");
    assert_eq!(body["agents"], 0);
    assert_eq!(body["missions"], 0);
}
