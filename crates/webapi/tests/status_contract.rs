//! `/api/status` is a public contract: the Discord bot polls it and prints the
//! body verbatim. These tests pin the contract down — wording, content type,
//! and the fact that a broken database degrades instead of erroring.

use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
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

    /// Build the live-DB fixture: two instances (one stopped), four players
    /// rows of which one has left and one sits in the stopped instance — so the
    /// expected count is 2.
    async fn live_db(&self, stopped_instance_players: i64) -> PathBuf {
        let path = self.path("server.db");
        let pool = db::open_read_write(&path, 1).await.expect("create fixture");
        sqlx::query(
            "CREATE TABLE ga_instances (instance_id INTEGER NOT NULL, state TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("ddl");
        sqlx::query(
            "CREATE TABLE ga_instance_players (
                 instance_id INTEGER NOT NULL,
                 user_id INTEGER NOT NULL,
                 left_at INTEGER
             )",
        )
        .execute(&pool)
        .await
        .expect("ddl");

        for (id, state) in [(1_i64, "READY"), (2, "STOPPED")] {
            sqlx::query("INSERT INTO ga_instances (instance_id, state) VALUES (?, ?)")
                .bind(id)
                .bind(state)
                .execute(&pool)
                .await
                .expect("row");
        }

        // Active players in the running instance.
        for user in [10_i64, 11] {
            sqlx::query(
                "INSERT INTO ga_instance_players (instance_id, user_id, left_at) VALUES (1, ?, NULL)",
            )
            .bind(user)
            .execute(&pool)
            .await
            .expect("row");
        }
        // A player who already left: not counted.
        sqlx::query("INSERT INTO ga_instance_players (instance_id, user_id, left_at) VALUES (1, 12, 1700000000)")
            .execute(&pool)
            .await
            .expect("row");
        // A player in a stopped instance: not counted.
        for i in 0..stopped_instance_players {
            sqlx::query(
                "INSERT INTO ga_instance_players (instance_id, user_id, left_at) VALUES (2, ?, NULL)",
            )
            .bind(20 + i)
            .execute(&pool)
            .await
            .expect("row");
        }

        pool.close().await;
        path
    }

    fn config(&self, live_db: Option<PathBuf>) -> Config {
        Config {
            bind: "127.0.0.1:0".parse().expect("addr"),
            live_db,
            snapshot_db: None,
            site_db: None,
            files_dir: self.path("static"),
            images_dir: self.path("static"),
            status_ttl: Duration::from_secs(60),
            live_tick: Duration::from_secs(15),
            match_ttl: Duration::from_secs(30),
            history_ttl: Duration::from_secs(3600),
            status_override: Some(self.path("status-override.txt")),
            control_api_url: None,
            control_api_token: None,
            db_export_token: None,
            news_posting_mode: webapi::news::PostingMode::Open,
            site_url: "https://example.test".to_string(),
        }
    }

    async fn app_with_live_db(&self) -> (Router, PathBuf) {
        let live_db = self.live_db(1).await;
        let pool = db::open_read_only(&live_db, 2)
            .await
            .expect("open read-only");
        let state = AppState::new(self.config(Some(live_db.clone())), Some(pool), None, None);
        (app(state), live_db)
    }

    fn app_without_live_db(&self) -> Router {
        let state = AppState::new(self.config(None), None, None, None);
        app(state)
    }
}

async fn get(app: &Router, uri: &str) -> (StatusCode, String, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("body");
    (
        status,
        content_type,
        String::from_utf8(body.to_vec()).expect("utf8"),
    )
}

#[tokio::test]
async fn status_is_plain_text_with_the_discord_bot_wording() {
    let harness = Harness::new();
    let (app, _) = harness.app_with_live_db().await;

    let (status, content_type, body) = get(&app, "/api/status").await;

    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/plain"), "got {content_type}");
    assert_eq!(body, "2 players in-game");
}

#[tokio::test]
async fn status_uses_the_singular_for_one_player() {
    let harness = Harness::new();
    let path = harness.path("server.db");
    let pool = db::open_read_write(&path, 1).await.expect("create");
    sqlx::query("CREATE TABLE ga_instances (instance_id INTEGER NOT NULL, state TEXT NOT NULL)")
        .execute(&pool)
        .await
        .expect("ddl");
    sqlx::query(
        "CREATE TABLE ga_instance_players (instance_id INTEGER NOT NULL, user_id INTEGER NOT NULL, left_at INTEGER)",
    )
    .execute(&pool)
    .await
    .expect("ddl");
    sqlx::query("INSERT INTO ga_instances VALUES (1, 'READY')")
        .execute(&pool)
        .await
        .expect("row");
    sqlx::query("INSERT INTO ga_instance_players VALUES (1, 7, NULL)")
        .execute(&pool)
        .await
        .expect("row");
    pool.close().await;

    let pool = db::open_read_only(&path, 2).await.expect("open");
    let state = AppState::new(harness.config(Some(path)), Some(pool), None, None);

    let (status, _, body) = get(&app(state), "/api/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "1 player in-game");
}

#[tokio::test]
async fn override_file_wins_and_takes_effect_immediately() {
    let harness = Harness::new();
    let (app, _) = harness.app_with_live_db().await;

    let (_, _, before) = get(&app, "/api/status").await;
    assert_eq!(before, "2 players in-game");

    let override_path = harness.path("status-override.txt");
    std::fs::write(&override_path, "Server maintenance\n").expect("write override");

    let (status, _, during) = get(&app, "/api/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(during, "Server maintenance");

    // Emptying the file reverts to the live count without a restart.
    std::fs::write(&override_path, "   \n").expect("clear override");
    let (_, _, after) = get(&app, "/api/status").await;
    assert_eq!(after, "2 players in-game");
}

#[tokio::test]
async fn missing_database_degrades_to_503_not_a_traceback() {
    let harness = Harness::new();
    let app = harness.app_without_live_db();

    let (status, content_type, body) = get(&app, "/api/status").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(content_type.starts_with("text/plain"));
    assert_eq!(body, "server status unavailable");
}

#[tokio::test]
async fn health_reports_the_live_db_and_degrades_without_it() {
    let harness = Harness::new();
    let (healthy, _) = harness.app_with_live_db().await;
    let (status, _, body) = get(&healthy, "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"live_db\":true"), "got {body}");

    let broken = harness.app_without_live_db();
    let (status, _, body) = get(&broken, "/api/health").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body.contains("\"live_db\":false"), "got {body}");
}

// --- recent matches ---------------------------------------------------------

async fn matches_fixture(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("matches.db");
    let pool = db::open_read_write(&path, 1).await.expect("create fixture");
    sqlx::query(
        "CREATE TABLE ga_instances (
             id INTEGER PRIMARY KEY, instance_id INTEGER NOT NULL, map_name TEXT NOT NULL,
             game_mode TEXT NOT NULL, started_at INTEGER NOT NULL, sealed_at INTEGER,
             outcome TEXT, winning_task_force INTEGER, is_home_map INTEGER NOT NULL DEFAULT 0,
             queue_id INTEGER)",
    )
    .execute(&pool)
    .await
    .expect("ddl");
    for ddl in [
        "CREATE TABLE ga_queues (queue_id INTEGER, name TEXT, name_msg_id INTEGER, location_value_id INTEGER)",
        "CREATE TABLE map_game_info (map_name TEXT, friendly_name_msg_id INTEGER)",
        "CREATE TABLE asm_data_set_msg_translations (msg_id INTEGER, message TEXT)",
        "CREATE TABLE asm_data_set_valid_values (value_id INTEGER, text_msg_translated TEXT)",
    ] {
        sqlx::query(ddl).execute(&pool).await.expect("lookup ddl");
    }
    for (msg_id, message) in [
        (100_i64, "Ultra Max Security"),
        (300, "Sector 20 Agent Inception Center"),
    ] {
        sqlx::query("INSERT INTO asm_data_set_msg_translations VALUES (?, ?)")
            .bind(msg_id)
            .bind(message)
            .execute(&pool)
            .await
            .expect("translation");
    }
    sqlx::query("INSERT INTO ga_queues VALUES (1, 'umax', 100, 200)")
        .execute(&pool)
        .await
        .expect("queue");
    sqlx::query("INSERT INTO asm_data_set_valid_values VALUES (200, 'Commonwealth Prime')")
        .execute(&pool)
        .await
        .expect("location");
    sqlx::query("INSERT INTO map_game_info VALUES ('1P_CPLab01_P', 300)")
        .execute(&pool)
        .await
        .expect("map");
    sqlx::query(
        "CREATE TABLE ga_match_player_stats (instance_id INTEGER NOT NULL,
                                             user_id INTEGER,
                                             character_id INTEGER NOT NULL)",
    )
    .execute(&pool)
    .await
    .expect("ddl");

    // id 1: completed mission with 3 participants.
    // id 2: the hub (home map) — must never appear in the archive.
    // id 3: still running (no sealed_at) — not a completed match yet.
    // id 4: completed escort with 1 participant.
    // (id, instance_id, map, mode, started_at, sealed_at, outcome, winner, is_home, queue)
    #[allow(clippy::type_complexity)]
    let rows: [(
        i64,
        i64,
        &str,
        &str,
        i64,
        Option<i64>,
        &str,
        Option<i64>,
        i64,
        i64,
    ); 4] = [
        (
            1,
            11,
            "1P_CPLab01_P",
            "TgGame.TgGame_Mission",
            1_700_000_000,
            Some(1_700_000_600),
            "ATTACKERS_WIN",
            Some(1),
            0,
            1,
        ),
        (
            2,
            12,
            "Dome3_VR_Arena_P",
            "TgGame.TgGame_City",
            1_700_000_100,
            Some(1_700_000_700),
            "ATTACKERS_WIN",
            Some(1),
            1,
            1,
        ),
        (
            3,
            13,
            "Push_Toxicity",
            "TgGame.TgGame_Escort",
            1_700_000_200,
            None,
            "",
            None,
            0,
            1,
        ),
        (
            4,
            14,
            "Ticket_HimLab_4v4",
            "TgGame.TgGame_PointRotation",
            1_700_000_300,
            Some(1_700_001_000),
            "DEFENDERS_WIN",
            Some(2),
            0,
            1,
        ),
    ];
    for (id, instance_id, map, mode, started, sealed, outcome, winner, home, queue) in rows {
        sqlx::query(
            "INSERT INTO ga_instances
             (id, instance_id, map_name, game_mode, started_at, sealed_at, outcome,
              winning_task_force, is_home_map, queue_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(instance_id)
        .bind(map)
        .bind(mode)
        .bind(started)
        .bind(sealed)
        .bind(outcome)
        .bind(winner)
        .bind(home)
        .bind(queue)
        .execute(&pool)
        .await
        .expect("row");
    }
    // Three people in match 11, one in 14 — and one of them swapped class, so
    // their two rows must still count as one participant.
    for (instance_id, user_id, character_id) in [
        (11, 101, 1),
        (11, 102, 2),
        (11, 103, 3),
        (11, 103, 4),
        (14, 104, 9),
    ] {
        sqlx::query(
            "INSERT INTO ga_match_player_stats (instance_id, user_id, character_id) VALUES (?, ?, ?)",
        )
        .bind(instance_id)
        .bind(user_id)
        .bind(character_id)
        .execute(&pool)
        .await
        .expect("row");
    }
    pool.close().await;
    path
}

async fn app_with_matches(harness: &Harness) -> Router {
    let path = matches_fixture(harness.dir.path()).await;
    let pool = db::open_read_only(&path, 2).await.expect("open read-only");
    let state = AppState::new(harness.config(Some(path)), Some(pool), None, None);
    app(state)
}

#[tokio::test]
async fn recent_matches_are_newest_first_completed_only() {
    let harness = Harness::new();
    let app = app_with_matches(&harness).await;

    let (status, content_type, body) = get(&app, "/api/matches/recent").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type.starts_with("application/json"),
        "got {content_type}"
    );

    let json: serde_json::Value = serde_json::from_str(&body).expect("json");
    let matches = json["matches"].as_array().expect("matches array");
    assert_eq!(
        matches.len(),
        2,
        "expected the two completed missions: {body}"
    );

    // Newest id first, home map excluded, running instance excluded.
    assert_eq!(matches[0]["id"], 4);
    assert_eq!(matches[0]["players"], 1);
    assert_eq!(matches[1]["id"], 1);
    assert_eq!(matches[1]["players"], 3);
    assert_eq!(matches[1]["outcome"], "ATTACKERS_WIN");
    assert_eq!(matches[1]["map_label"], "Sector 20 Agent Inception Center");
    assert_eq!(matches[1]["queue_label"], "Ultra Max Security");
    assert_eq!(matches[1]["queue_location"], "Commonwealth Prime");
    assert_eq!(json["limit"], 8);
}

#[tokio::test]
async fn recent_matches_limit_is_clamped() {
    let harness = Harness::new();
    let app = app_with_matches(&harness).await;

    let (_, _, body) = get(&app, "/api/matches/recent?limit=1").await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(json["matches"].as_array().expect("array").len(), 1);
    assert_eq!(json["limit"], 1);

    // Over-large and nonsense limits are clamped, never forwarded raw.
    let (_, _, body) = get(&app, "/api/matches/recent?limit=9999").await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(json["limit"], 50);
}

#[tokio::test]
async fn recent_matches_degrade_to_503_without_the_database() {
    let harness = Harness::new();
    let app = harness.app_without_live_db();

    let (status, content_type, body) = get(&app, "/api/matches/recent").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(content_type.starts_with("application/json"));
    assert!(body.contains("error"), "got {body}");
}
