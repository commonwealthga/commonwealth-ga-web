//! Player settings: reading the account's preferences and forwarding changes.
//!
//! The write path is the interesting part: the website owns no authority here,
//! it relays the change to the control server, which persists it and pushes the
//! live one. These tests stand a fake control server in front of it and assert
//! both what the player sees and what actually crossed the wire.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;
use webapi::config::Config;
use webapi::{AppState, app, auth, db};

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

    async fn live_db(&self) -> PathBuf {
        let live_path = self.path("server.db");
        let live = db::open_read_write(&live_path, 2).await.expect("live db");

        for ddl in [
            "CREATE TABLE ga_users (id INTEGER PRIMARY KEY, username TEXT)",
            "CREATE TABLE ga_user_preferences (
                 id INTEGER PRIMARY KEY AUTOINCREMENT, user_id INTEGER NOT NULL,
                 config_key TEXT NOT NULL, config_value TEXT NOT NULL,
                 UNIQUE(user_id, config_key))",
            "CREATE TABLE ga_dlc (dlc_id INTEGER PRIMARY KEY, identifier TEXT NOT NULL UNIQUE,
                                  name TEXT NOT NULL)",
            "CREATE TABLE ga_user_dlc (user_id INTEGER NOT NULL, dlc_id INTEGER NOT NULL,
                                       installed INTEGER NOT NULL DEFAULT 0)",
            // How a pack classifies as PvP or PvE: the maps it ships.
            "CREATE TABLE ga_map_pool_entries (map_pool_id INTEGER, map_name TEXT, game_mode TEXT,
                                               weight INTEGER, enabled INTEGER, dlc_id INTEGER)",
            "CREATE TABLE map_game_info (map_name TEXT, is_pvp INTEGER)",
        ] {
            sqlx::query(ddl).execute(&live).await.expect("ddl");
        }

        sqlx::query("INSERT INTO ga_users VALUES (7, 'Seven')")
            .execute(&live)
            .await
            .expect("user");
        // The account turned broken suits off and never touched solo mode.
        sqlx::query("INSERT INTO ga_user_preferences (user_id, config_key, config_value) VALUES (7, 'show_broken_suits', '0')")
            .execute(&live)
            .await
            .expect("pref");
        sqlx::query("INSERT INTO ga_dlc VALUES (1, 'carbon-capture', 'Scramble: Carbon Capture')")
            .execute(&live)
            .await
            .expect("dlc");
        sqlx::query("INSERT INTO ga_user_dlc VALUES (7, 1, 1)")
            .execute(&live)
            .await
            .expect("user dlc");
        // Pack 1 is a PvP pack; the catalog also carries a PvE pack with no
        // account flag, and a pack whose maps were never linked.
        sqlx::query("INSERT INTO ga_dlc VALUES (2, 'factory-maps', 'Central Industrial Complex & Recycling Plant 37')")
            .execute(&live)
            .await
            .expect("dlc");
        sqlx::query("INSERT INTO ga_map_pool_entries VALUES (1, 'Rot_10v10_Carbon_Capture1', 'TgGame.TgGame_Mission', 1, 1, 1)")
            .execute(&live)
            .await
            .expect("pool");
        sqlx::query("INSERT INTO map_game_info VALUES ('Rot_10v10_Carbon_Capture1', 1)")
            .execute(&live)
            .await
            .expect("map");
        sqlx::query("INSERT INTO ga_map_pool_entries VALUES (1, '1P_CPFactory03_P', 'TgGame.TgGame_Mission', 1, 1, 2)")
            .execute(&live)
            .await
            .expect("pool");
        sqlx::query("INSERT INTO map_game_info VALUES ('1P_CPFactory03_P', 0)")
            .execute(&live)
            .await
            .expect("map");

        live.close().await;
        live_path
    }

    fn config(&self, live_path: PathBuf, control_url: Option<String>) -> Config {
        Config {
            bind: "127.0.0.1:0".parse().expect("addr"),
            live_db: Some(live_path.clone()),
            snapshot_db: None,
            site_db: Some(self.path("site.db")),
            static_dir: self.path("static"),
            files_dir: self.path("files"),
            status_ttl: Duration::from_secs(0),
            live_tick: Duration::from_secs(15),
            match_ttl: Duration::from_secs(0),
            history_ttl: Duration::from_secs(0),
            status_override: None,
            control_api_url: control_url,
            control_api_token: Some("test-token".to_string()),
            db_export_token: None,
            news_posting_mode: webapi::news::PostingMode::Open,
            site_url: "https://example.test".to_string(),
        }
    }

    /// App with a session for user 7, and optionally a control server to talk to.
    async fn app(&self, live_path: PathBuf, control_url: Option<String>) -> (Router, String) {
        let site_path = self.path("site.db");
        let site = db::open_read_write(&site_path, 2).await.expect("site db");
        auth::migrate(&site).await.expect("migrate");
        let token = auth::create_session(&site, 7, "Seven")
            .await
            .expect("session");
        site.close().await;

        let live = db::open_read_only(&live_path, 2).await.expect("live ro");
        let site_ro = db::open_read_only(&site_path, 2).await.expect("site ro");

        let state = AppState::new(
            self.config(live_path, control_url),
            Some(live),
            None,
            Some(site_ro),
        );
        (app(state), token)
    }
}

async fn request(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    body: &str,
) -> (StatusCode, Value) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, format!("cw_session={token}"));
    let request = if body.is_empty() {
        builder.body(Body::empty()).expect("request")
    } else {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("request")
    };
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A stand-in for the control server: answers one request, records it.
async fn fake_control_server() -> (String, Arc<tokio::sync::Mutex<Option<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let seen = Arc::new(tokio::sync::Mutex::new(None));

    let recorder = seen.clone();
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            let mut raw = Vec::new();
            let mut buf = [0_u8; 4096];
            // Read until the declared body has arrived.
            while let Ok(read) = socket.read(&mut buf).await {
                if read == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..read]);
                let text = String::from_utf8_lossy(&raw).to_string();
                if let Some(split) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|line| {
                            let lower = line.to_lowercase();
                            lower
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if raw.len() >= split + 4 + length {
                        break;
                    }
                }
            }
            *recorder.lock().await = Some(String::from_utf8_lossy(&raw).to_string());
            let body = r#"{"ok":true,"applied_live":true}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });

    (format!("http://{addr}"), seen)
}

#[tokio::test]
async fn settings_show_stored_values_and_defaults() {
    let harness = Harness::new();
    let live_path = harness.live_db().await;
    let (router, token) = harness.app(live_path, None).await;

    let (status, body) = request(&router, "GET", "/api/me/settings", &token, "").await;
    assert_eq!(status, StatusCode::OK);

    let preferences = body["preferences"].as_array().expect("preferences");
    let broken = preferences
        .iter()
        .find(|p| p["key"] == "show_broken_suits")
        .expect("show_broken_suits");
    assert_eq!(broken["value"], "0", "the stored value wins");
    assert_eq!(broken["enabled"], false);

    // Never set: falls back to the game's own default, not to "off".
    let solo = preferences
        .iter()
        .find(|p| p["key"] == "solo_mode")
        .expect("solo_mode");
    assert_eq!(solo["value"], "0");
    let suits = preferences
        .iter()
        .find(|p| p["key"] == "show_all_suits")
        .expect("show_all_suits");
    assert_eq!(suits["value"], "1");

    let dlc = body["dlc"].as_array().expect("dlc");
    assert_eq!(dlc[0]["identifier"], "carbon-capture");
    assert_eq!(dlc[0]["installed"], true);
    // Mode comes from the pack's own maps, not a hand-kept list.
    assert_eq!(dlc[0]["mode"], "pvp");
    assert_eq!(dlc[1]["mode"], "pve");

    // No control server configured: readable, not writable.
    assert_eq!(body["writable"], false);
}

#[tokio::test]
async fn a_change_is_forwarded_to_the_control_server() {
    let harness = Harness::new();
    let live_path = harness.live_db().await;
    let (url, seen) = fake_control_server().await;
    let (router, token) = harness.app(live_path, Some(url)).await;

    // Reading now advertises the write path.
    let (_, body) = request(&router, "GET", "/api/me/settings", &token, "").await;
    assert_eq!(body["writable"], true);

    let (status, body) = request(
        &router,
        "POST",
        "/api/me/settings",
        &token,
        r#"{"key":"solo_mode","value":"1"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "got {body}");
    assert_eq!(body["applied_live"], true);

    let raw = seen
        .lock()
        .await
        .clone()
        .expect("control server saw a request");
    assert!(
        raw.starts_with("POST /player/preferences HTTP/1.1\r\n"),
        "{raw}"
    );
    assert!(raw.contains("X-Admin-Token: test-token"), "{raw}");
    // The user id comes from the session, never from the browser.
    assert!(raw.contains(r#""user_id":7"#), "{raw}");
    assert!(raw.contains(r#""key":"solo_mode""#), "{raw}");
    assert!(raw.contains(r#""value":"1""#), "{raw}");
}

#[tokio::test]
async fn dlc_flags_are_forwarded_separately() {
    let harness = Harness::new();
    let live_path = harness.live_db().await;
    let (url, seen) = fake_control_server().await;
    let (router, token) = harness.app(live_path, Some(url)).await;

    let (status, _) = request(
        &router,
        "POST",
        "/api/me/settings/dlc",
        &token,
        r#"{"identifier":"carbon-capture","installed":false}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let raw = seen.lock().await.clone().expect("request");
    assert!(raw.starts_with("POST /player/dlc HTTP/1.1\r\n"), "{raw}");
    assert!(raw.contains(r#""identifier":"carbon-capture""#), "{raw}");
    assert!(raw.contains(r#""installed":false"#), "{raw}");
}

#[tokio::test]
async fn writes_are_validated_before_anything_is_sent() {
    let harness = Harness::new();
    let live_path = harness.live_db().await;
    let (url, seen) = fake_control_server().await;
    let (router, token) = harness.app(live_path, Some(url)).await;

    // Unknown key: the control server's whitelist is mirrored here so an
    // invented key never reaches the game's configuration table.
    let (status, _) = request(
        &router,
        "POST",
        "/api/me/settings",
        &token,
        r#"{"key":"teleport","value":"1"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = request(
        &router,
        "POST",
        "/api/me/settings",
        &token,
        r#"{"key":"solo_mode","value":"maybe"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert!(
        seen.lock().await.is_none(),
        "nothing should have been forwarded"
    );
}

#[tokio::test]
async fn without_a_control_server_a_write_is_a_clean_503() {
    let harness = Harness::new();
    let live_path = harness.live_db().await;
    let (router, token) = harness.app(live_path, None).await;

    let (status, _) = request(
        &router,
        "POST",
        "/api/me/settings",
        &token,
        r#"{"key":"solo_mode","value":"1"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn settings_need_a_session() {
    let harness = Harness::new();
    let live_path = harness.live_db().await;
    let (router, _) = harness.app(live_path, None).await;

    let request = Request::builder()
        .uri("/api/me/settings")
        .body(Body::empty())
        .expect("request");
    let response = router.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
