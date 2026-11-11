//! Sign-in and player-activity tests.
//!
//! These exercise the wiring around the credential check: what the browser
//! posts, what the cookie does, and what happens when any of it is wrong. The
//! derivation itself lives in the browser; the server only ever compares a
//! SHA-256.

use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tempfile::TempDir;
use tower::ServiceExt;
use webapi::config::Config;
use webapi::{AppState, app, auth, db};

struct Harness {
    dir: TempDir,
}

struct Fixtures {
    app: Router,
    username: String,
    keystream: Vec<u8>,
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

    /// Live DB with one registered account, plus the website DB for sessions.
    async fn fixtures(&self, password_like_keystream: &[u8], registered: bool) -> Fixtures {
        let username = "Agent01".to_string();
        // The keystream length the client would produce for this account.
        let mut keystream = password_like_keystream.to_vec();
        keystream.resize(auth::expected_keystream_len(&username), 0x5a);

        let live_path = self.path("server.db");
        let live = db::open_read_write(&live_path, 2).await.expect("live db");
        sqlx::query(
            "CREATE TABLE ga_users (
                 id INTEGER PRIMARY KEY,
                 username TEXT,
                 password_verifier BLOB)",
        )
        .execute(&live)
        .await
        .expect("ddl");
        sqlx::query(
            "CREATE TABLE ga_match_player_stats (
                 instance_id INTEGER, user_id INTEGER, character_id INTEGER, task_force INTEGER,
                 kills INTEGER, deaths INTEGER, assists INTEGER, damage_dealt INTEGER,
                 healing INTEGER, obj_points INTEGER, time_played_seconds REAL)",
        )
        .execute(&live)
        .await
        .expect("ddl");
        sqlx::query(
            "CREATE TABLE ga_instances (
                 id INTEGER PRIMARY KEY, instance_id INTEGER, map_name TEXT, game_mode TEXT,
                 started_at INTEGER, sealed_at INTEGER, outcome TEXT, winning_task_force INTEGER,
                 is_home_map INTEGER DEFAULT 0, queue_id INTEGER)",
        )
        .execute(&live)
        .await
        .expect("ddl");
        for ddl in [
            "CREATE TABLE ga_queues (queue_id INTEGER, name TEXT, name_msg_id INTEGER, location_value_id INTEGER)",
            "CREATE TABLE map_game_info (map_name TEXT, friendly_name_msg_id INTEGER)",
            "CREATE TABLE asm_data_set_msg_translations (msg_id INTEGER, message TEXT)",
            "CREATE TABLE asm_data_set_valid_values (value_id INTEGER, text_msg_translated TEXT)",
        ] {
            sqlx::query(ddl).execute(&live).await.expect("lookup ddl");
        }
        for (msg_id, message) in [
            (100_i64, "Ultra Max Security"),
            (200, "Commonwealth Prime"),
            (300, "Sector 20 Agent Inception Center"),
        ] {
            sqlx::query("INSERT INTO asm_data_set_msg_translations VALUES (?, ?)")
                .bind(msg_id)
                .bind(message)
                .execute(&live)
                .await
                .expect("translation");
        }
        sqlx::query("INSERT INTO ga_queues VALUES (1, 'umax', 100, 200)")
            .execute(&live)
            .await
            .expect("queue");
        sqlx::query("INSERT INTO asm_data_set_valid_values VALUES (200, 'Commonwealth Prime')")
            .execute(&live)
            .await
            .expect("location");
        for map in ["1P_CPLab01_P", "Ticket_HimLab_4v4", "Push_Toxicity"] {
            sqlx::query("INSERT INTO map_game_info VALUES (?, 300)")
                .bind(map)
                .execute(&live)
                .await
                .expect("map");
        }
        sqlx::query(
            "CREATE TABLE ga_mmr_history (
                 user_id INTEGER, class_name TEXT, instance_id INTEGER, played_at INTEGER,
                 mmr_before REAL, mmr_after REAL, games_after INTEGER)",
        )
        .execute(&live)
        .await
        .expect("ddl");

        let verifier: Option<Vec<u8>> =
            registered.then(|| auth::verifier_from_keystream(&keystream).to_vec());
        sqlx::query("INSERT INTO ga_users (id, username, password_verifier) VALUES (?, ?, ?)")
            .bind(7_i64)
            .bind(&username)
            .bind(verifier)
            .execute(&live)
            .await
            .expect("row");

        // One match for this player, so /api/me/matches has something to return.
        sqlx::query("INSERT INTO ga_instances VALUES (1, 500, '1P_CPLab01_P', 'TgGame_Mission', 1000, 1600, 'ATTACKERS_WIN', 1, 0, 1)")
            .execute(&live)
            .await
            .expect("row");
        sqlx::query(
            "INSERT INTO ga_match_player_stats VALUES (500, 7, 1, 1, 12, 3, 4, 9000, 0, 55, 600)",
        )
        .execute(&live)
        .await
        .expect("row");
        // The fixture stores a performance-engine history, so it must say so:
        // the site reads whichever table `active_mmr_engine` names.
        for ddl in [
            "INSERT INTO ga_mmr_history VALUES (7, 'Assault', 500, 1600, 1000.0, 1024.5, 1)",
            "CREATE TABLE cs_settings (key TEXT PRIMARY KEY, value TEXT)",
            "INSERT INTO cs_settings VALUES ('active_mmr_engine', 'perf')",
        ] {
            sqlx::query(ddl).execute(&live).await.expect("row");
        }

        live.close().await;

        let site_path = self.path("site.db");
        let site = db::open_read_write(&site_path, 2).await.expect("site db");
        auth::migrate(&site).await.expect("migrate");
        site.close().await;

        let live_pool = db::open_read_only(&live_path, 2).await.expect("open live");
        let site_pool = db::open_read_write(&site_path, 2).await.expect("open site");

        let cfg = Config {
            bind: "127.0.0.1:0".parse().expect("addr"),
            live_db: Some(live_path),
            snapshot_db: None,
            site_db: Some(site_path),
            static_dir: self.path("static"),
            files_dir: self.path("static"),
            status_ttl: Duration::from_secs(60),
            live_tick: Duration::from_secs(15),
            match_ttl: Duration::from_secs(30),
            history_ttl: Duration::from_secs(3600),
            status_override: None,
            control_api_url: None,
            control_api_token: None,
            db_export_token: None,
        };

        Fixtures {
            app: app(AppState::new(cfg, Some(live_pool), None, Some(site_pool))),
            username,
            keystream,
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Option<String>, String) {
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or_default().to_string());
    let body = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body");
    (status, cookie, String::from_utf8_lossy(&body).to_string())
}

fn login_request(username: &str, keystream_hex: &str) -> Request<Body> {
    let body = serde_json::json!({ "username": username, "keystream": keystream_hex }).to_string();
    Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .expect("request")
}

fn get_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .expect("request")
}

#[tokio::test]
async fn login_succeeds_and_the_cookie_opens_the_player_routes() {
    let harness = Harness::new();
    let fixtures = harness.fixtures(b"a-realish-keystream", true).await;

    let (status, cookie, body) = send(
        &fixtures.app,
        login_request(&fixtures.username, &hex(&fixtures.keystream)),
    )
    .await;

    assert!(status == StatusCode::OK, "login failed ({status}): {body}");
    let cookie = cookie.expect("session cookie");
    assert!(cookie.starts_with("cw_session="), "got {cookie}");

    let (status, _, me) = send(&fixtures.app, get_with_cookie("/api/auth/me", &cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(me.contains("\"authenticated\":true"), "got {me}");
    assert!(me.contains("Agent01"), "username should come back: {me}");

    let (status, _, matches) =
        send(&fixtures.app, get_with_cookie("/api/me/matches", &cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(matches.contains("1P_CPLab01_P"), "got {matches}");
    assert!(
        matches.contains("\"kills\":12"),
        "per-match stats included: {matches}"
    );

    let (status, _, summary) =
        send(&fixtures.app, get_with_cookie("/api/me/summary", &cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(summary.contains("\"kills\":12"), "career totals: {summary}");
    assert!(summary.contains("Assault"), "rating per class: {summary}");
}

#[tokio::test]
async fn wrong_keystream_is_rejected() {
    let harness = Harness::new();
    let fixtures = harness.fixtures(b"the-right-one", true).await;

    let mut wrong = fixtures.keystream.clone();
    wrong[0] ^= 0xff;

    let (status, cookie, _) = send(
        &fixtures.app,
        login_request(&fixtures.username, &hex(&wrong)),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(cookie.is_none(), "no session on failure");
}

#[tokio::test]
async fn account_without_a_registered_credential_cannot_sign_in() {
    let harness = Harness::new();
    let fixtures = harness.fixtures(b"never-used", false).await;

    let (status, _, body) = send(
        &fixtures.app,
        login_request(&fixtures.username, &hex(&fixtures.keystream)),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "got {body}");
    assert!(body.contains("password"), "explains why: {body}");
}

#[tokio::test]
async fn malformed_input_is_rejected_before_it_reaches_the_database() {
    let harness = Harness::new();
    let fixtures = harness.fixtures(b"whatever", true).await;

    // Short keystream: cannot be the client's output for this account.
    let (status, _, _) = send(&fixtures.app, login_request(&fixtures.username, "0011")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Non-hex.
    let (status, _, _) = send(&fixtures.app, login_request(&fixtures.username, "zzzz")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Usernames we would never accept.
    let (status, _, _) = send(&fixtures.app, login_request("bad name!", "00")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn player_routes_are_closed_without_a_session() {
    let harness = Harness::new();
    let fixtures = harness.fixtures(b"whatever", true).await;

    for uri in ["/api/me/matches", "/api/me/summary"] {
        let request = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("request");
        let (status, _, _) = send(&fixtures.app, request).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{uri} must require a session"
        );
    }

    // A made-up token is not a session either.
    let request = get_with_cookie("/api/me/summary", "cw_session=deadbeef");
    let (status, _, _) = send(&fixtures.app, request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_clears_the_session() {
    let harness = Harness::new();
    let fixtures = harness.fixtures(b"keystream", true).await;

    let (_, cookie, _) = send(
        &fixtures.app,
        login_request(&fixtures.username, &hex(&fixtures.keystream)),
    )
    .await;
    let cookie = cookie.expect("session cookie");

    let logout = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .expect("request");
    let (status, cleared, _) = send(&fixtures.app, logout).await;
    assert_eq!(status, StatusCode::OK);
    assert!(cleared.is_some(), "the cookie is cleared client-side too");

    let (_, _, me) = send(&fixtures.app, get_with_cookie("/api/auth/me", &cookie)).await;
    assert!(
        me.contains("\"authenticated\":false"),
        "session is gone: {me}"
    );
}

#[tokio::test]
async fn online_history_returns_parallel_arrays() {
    let harness = Harness::new();
    let live_path = harness.path("history.db");
    let live = db::open_read_write(&live_path, 1).await.expect("db");
    sqlx::query(
        "CREATE TABLE ga_user_sessions (
             user_id INTEGER, login_at INTEGER, logout_at INTEGER, outcome TEXT)",
    )
    .execute(&live)
    .await
    .expect("ddl");
    sqlx::query(
        "CREATE TABLE ga_match_events (actor_user_id INTEGER, target_user_id INTEGER, ts INTEGER)",
    )
    .execute(&live)
    .await
    .expect("ddl");

    let hour = 3600_i64;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64;
    // Anchored to the present: the endpoint windows the series by days.
    let base = (now / hour) * hour - 5 * hour;
    for (user, login, logout) in [
        (1_i64, base + 60, Some(base + 2 * hour)),
        (2, base + 120, Some(base + hour)),
    ] {
        sqlx::query("INSERT INTO ga_user_sessions VALUES (?, ?, ?, 'ok')")
            .bind(user)
            .bind(login)
            .bind(logout)
            .execute(&live)
            .await
            .expect("row");
    }
    live.close().await;

    let pool = db::open_read_only(&live_path, 1).await.expect("open");
    let cfg = Config {
        bind: "127.0.0.1:0".parse().expect("addr"),
        live_db: Some(live_path),
        snapshot_db: None,
        site_db: None,
        static_dir: harness.path("static"),
        files_dir: harness.path("static"),
        status_ttl: Duration::from_secs(60),
        live_tick: Duration::from_secs(15),
        match_ttl: Duration::from_secs(30),
        history_ttl: Duration::from_secs(3600),
        status_override: None,
        control_api_url: None,
        control_api_token: None,
        db_export_token: None,
    };
    let app = app(AppState::new(cfg, Some(pool), None, None));

    let request = Request::builder()
        .uri("/api/online-history?days=365")
        .body(Body::empty())
        .expect("request");
    let (status, _, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK, "got {body}");

    let json: serde_json::Value = serde_json::from_str(&body).expect("json");
    let times = json["times"].as_array().expect("times");
    let players = json["players"].as_array().expect("players");
    assert_eq!(times.len(), players.len(), "the two series must line up");
    assert!(!times.is_empty(), "the fixture should produce buckets");
    assert!(
        players.iter().any(|p| p.as_i64().unwrap_or(0) == 2),
        "two overlapping players should register a peak of 2: {body}"
    );
}

#[tokio::test]
async fn activity_window_zero_returns_everything() {
    let harness = Harness::new();
    let live_path = harness.path("history-all.db");
    let live = db::open_read_write(&live_path, 1).await.expect("db");
    sqlx::query(
        "CREATE TABLE ga_user_sessions (
             user_id INTEGER, login_at INTEGER, logout_at INTEGER, outcome TEXT)",
    )
    .execute(&live)
    .await
    .expect("ddl");

    let hour = 3600_i64;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64;
    // One session today, one two years ago: a day window must hide the old one.
    for login in [now - hour, now - 730 * 24 * hour] {
        sqlx::query("INSERT INTO ga_user_sessions VALUES (1, ?, ?, 'ok')")
            .bind(login)
            .bind(login + 1800)
            .execute(&live)
            .await
            .expect("row");
    }
    live.close().await;

    let pool = db::open_read_only(&live_path, 1).await.expect("open");
    let cfg = Config {
        bind: "127.0.0.1:0".parse().expect("addr"),
        live_db: Some(live_path),
        snapshot_db: None,
        site_db: None,
        static_dir: harness.path("static"),
        files_dir: harness.path("static"),
        status_ttl: Duration::from_secs(60),
        live_tick: Duration::from_secs(15),
        match_ttl: Duration::from_secs(30),
        history_ttl: Duration::from_secs(3600),
        status_override: None,
        control_api_url: None,
        control_api_token: None,
        db_export_token: None,
    };
    let app = app(AppState::new(cfg, Some(pool), None, None));

    let windowed = Request::builder()
        .uri("/api/online-history?days=7")
        .body(Body::empty())
        .expect("request");
    let (_, _, body) = send(&app, windowed).await;
    let recent: serde_json::Value = serde_json::from_str(&body).expect("json");

    let everything = Request::builder()
        .uri("/api/online-history?days=0")
        .body(Body::empty())
        .expect("request");
    let (status, _, all_body) = send(&app, everything).await;
    assert_eq!(status, StatusCode::OK);
    let all: serde_json::Value = serde_json::from_str(&all_body).expect("json");

    assert!(
        all["times"].as_array().expect("times").len()
            > recent["times"].as_array().expect("times").len(),
        "days=0 must reach further back than a 7-day window"
    );
}
