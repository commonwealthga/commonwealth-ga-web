//! One match in full: the archive's detail view.
//!
//! Two layouts come out of the same endpoint, so both are pinned here: a PvE
//! match keeps every statistic, and a PvP match adds pre-match ratings and the
//! winner/loser split. The viewer field is the difference between a public
//! scoreboard and "you won".

use std::path::PathBuf;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tempfile::TempDir;
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

    async fn build(&self) -> (Router, PathBuf) {
        let live_path = self.path("server.db");
        let live = db::open_read_write(&live_path, 2).await.expect("live db");

        for ddl in [
            "CREATE TABLE ga_instances (
                 id INTEGER PRIMARY KEY, instance_id INTEGER NOT NULL, map_name TEXT NOT NULL,
                 game_mode TEXT NOT NULL, started_at INTEGER NOT NULL, sealed_at INTEGER,
                 outcome TEXT, winning_task_force INTEGER, is_home_map INTEGER NOT NULL DEFAULT 0,
                 queue_id INTEGER)",
            "CREATE TABLE ga_match_player_stats (
                 instance_id INTEGER NOT NULL, user_id INTEGER NOT NULL, character_id INTEGER NOT NULL,
                 task_force INTEGER, rep_points REAL, kills INTEGER, assists INTEGER,
                 damage_taken INTEGER, damage_dealt INTEGER, buff_value INTEGER, healing INTEGER,
                 defense INTEGER, deaths INTEGER, obj_points INTEGER, bot_kills INTEGER,
                 capture_seconds REAL, contest_seconds REAL, objective_captures INTEGER,
                 beacon_spawns_provided INTEGER, beacon_spawns_used INTEGER,
                 beacons_destroyed INTEGER, time_played_seconds REAL)",
            "CREATE TABLE ga_users (id INTEGER PRIMARY KEY, username TEXT)",
            "CREATE TABLE ga_characters (id INTEGER PRIMARY KEY, user_id INTEGER, profile_id INTEGER)",
            "CREATE TABLE ga_queues (queue_id INTEGER, name TEXT, name_msg_id INTEGER,
                                      desc_msg_id INTEGER, location_value_id INTEGER)",
            "CREATE TABLE map_game_info (map_name TEXT, friendly_name_msg_id INTEGER)",
            "CREATE TABLE asm_data_set_msg_translations (msg_id INTEGER, message TEXT)",
            "CREATE TABLE asm_data_set_valid_values (value_id INTEGER, text_msg_translated TEXT)",
            "CREATE TABLE ga_wl_mmr_history (user_id INTEGER, class_name TEXT, instance_id INTEGER,
                                              played_at INTEGER, mmr_before REAL, mmr_after REAL)",
            "CREATE TABLE cs_settings (key TEXT PRIMARY KEY, value TEXT)",
        ] {
            sqlx::query(ddl).execute(&live).await.expect("ddl");
        }

        // Two queues: one PvE, one PvP.
        sqlx::query("INSERT INTO ga_queues VALUES (1, 'umax', 100, 101, 200)")
            .execute(&live)
            .await
            .expect("queue");
        sqlx::query("INSERT INTO ga_queues VALUES (2, 'merc', 110, 111, 0)")
            .execute(&live)
            .await
            .expect("queue");
        for (msg_id, message) in [
            (100_i64, "Ultra Max Security"),
            (110, "Mercenary"),
            (300, "Sector 20 Agent Inception Center"),
        ] {
            sqlx::query("INSERT INTO asm_data_set_msg_translations VALUES (?, ?)")
                .bind(msg_id)
                .bind(message)
                .execute(&live)
                .await
                .expect("translation");
        }
        sqlx::query("INSERT INTO asm_data_set_valid_values VALUES (200, 'Commonwealth Prime')")
            .execute(&live)
            .await
            .expect("location");
        sqlx::query("INSERT INTO map_game_info VALUES ('1P_CPLab01_P', 300)")
            .execute(&live)
            .await
            .expect("map");
        sqlx::query("INSERT INTO cs_settings VALUES ('active_mmr_engine', 'wl')")
            .execute(&live)
            .await
            .expect("engine");

        // 1001: PvE, two players. 1002: PvP, one winner and one loser.
        for (id, instance_id, queue, started, sealed, winner) in [
            (
                1_i64,
                1001_i64,
                1_i64,
                1_700_000_000_i64,
                Some(1_700_000_600_i64),
                Some(1_i64),
            ),
            (2, 1002, 2, 1_700_001_000, Some(1_700_001_600), Some(1)),
        ] {
            sqlx::query(
                "INSERT INTO ga_instances
                 (id, instance_id, map_name, game_mode, started_at, sealed_at, outcome,
                  winning_task_force, is_home_map, queue_id)
                 VALUES (?, ?, '1P_CPLab01_P', 'TgGame.TgGame_Mission', ?, ?, 'ATTACKERS_WIN', ?, 0, ?)",
            )
            .bind(id)
            .bind(instance_id)
            .bind(started)
            .bind(sealed)
            .bind(winner)
            .bind(queue)
            .execute(&live)
            .await
            .expect("instance");
        }

        for (id, username) in [(7_i64, "Seven"), (8, "Eight")] {
            sqlx::query("INSERT INTO ga_users VALUES (?, ?)")
                .bind(id)
                .bind(username)
                .execute(&live)
                .await
                .expect("user");
        }
        for (character, user, profile) in [(70_i64, 7_i64, 680_i64), (80, 8, 567)] {
            sqlx::query("INSERT INTO ga_characters VALUES (?, ?, ?)")
                .bind(character)
                .bind(user)
                .bind(profile)
                .execute(&live)
                .await
                .expect("character");
        }

        // PvE match: same side, real numbers (INTEGER columns, as the game stores them).
        for (user, character, side, kills, damage, bots) in [
            (7_i64, 70_i64, 1_i64, 12_i64, 480_000_i64, 34_i64),
            (8, 80, 1, 3, 120_500, 9),
        ] {
            sqlx::query(
                "INSERT INTO ga_match_player_stats
                 (instance_id, user_id, character_id, task_force, rep_points, kills, assists,
                  damage_taken, damage_dealt, buff_value, healing, defense, deaths, obj_points,
                  bot_kills, capture_seconds, contest_seconds, objective_captures,
                  beacon_spawns_provided, beacon_spawns_used, beacons_destroyed, time_played_seconds)
                 VALUES (1001, ?, ?, ?, 100, ?, 4, 5000, ?, 0, 2500, 0, 2, 900, ?, 30, 10, 2, 1, 1, 3, 480.0)",
            )
            .bind(user)
            .bind(character)
            .bind(side)
            .bind(kills)
            .bind(damage)
            .bind(bots)
            .execute(&live)
            .await
            .expect("pve row");
        }

        // PvP match: user 7 on the winning side, user 8 on the losing side.
        for (user, character, side, kills, deaths, damage) in [
            (7_i64, 70_i64, 1_i64, 20_i64, 4_i64, 300_000_i64),
            (8, 80, 2, 9, 18, 180_000),
        ] {
            sqlx::query(
                "INSERT INTO ga_match_player_stats
                 (instance_id, user_id, character_id, task_force, rep_points, kills, assists,
                  damage_taken, damage_dealt, buff_value, healing, defense, deaths, obj_points,
                  bot_kills, capture_seconds, contest_seconds, objective_captures,
                  beacon_spawns_provided, beacon_spawns_used, beacons_destroyed, time_played_seconds)
                 VALUES (1002, ?, ?, ?, 0, ?, 5, 90000, ?, 0, 0, 0, ?, 15000, 2, 0, 0, 0, 0, 0, 0, 900.0)",
            )
            .bind(user)
            .bind(character)
            .bind(side)
            .bind(kills)
            .bind(damage)
            .bind(deaths)
            .execute(&live)
            .await
            .expect("pvp row");
        }

        // Ratings: user 7 is the underdog who won, user 8 the favourite who lost.
        sqlx::query(
            "INSERT INTO ga_wl_mmr_history VALUES (7, 'Assault', 1002, 1700001000, 1040.0, 1060.0)",
        )
        .execute(&live)
        .await
        .expect("mmr");
        sqlx::query(
            "INSERT INTO ga_wl_mmr_history VALUES (8, 'Medic', 1002, 1700001000, 1120.0, 1100.0)",
        )
        .execute(&live)
        .await
        .expect("mmr");

        live.close().await;
        let pool = db::open_read_only(&live_path, 2).await.expect("reopen");

        let cfg = Config {
            bind: "127.0.0.1:0".parse().expect("addr"),
            live_db: Some(live_path.clone()),
            snapshot_db: None,
            site_db: None,
            files_dir: self.path("files"),
            status_ttl: std::time::Duration::from_secs(60),
            live_tick: std::time::Duration::from_secs(15),
            match_ttl: std::time::Duration::from_secs(30),
            history_ttl: std::time::Duration::from_secs(3600),
            status_override: None,
            control_api_url: None,
            control_api_token: None,
            db_export_token: None,
            news_posting_mode: webapi::news::PostingMode::Open,
            site_url: "https://example.test".to_string(),
        };
        (app(AppState::new(cfg, Some(pool), None, None)), live_path)
    }
}

async fn get(app: &Router, uri: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request");
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4 << 20)
        .await
        .expect("body");
    let json = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, json)
}

#[tokio::test]
async fn a_pve_match_keeps_every_statistic() {
    let harness = Harness::new();
    let (router, _) = harness.build().await;

    let (status, body) = get(&router, "/api/matches/1001").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "pve");
    assert_eq!(body["map_label"], "Sector 20 Agent Inception Center");
    assert_eq!(body["queue_label"], "Ultra Max Security");
    assert_eq!(body["queue_location"], "Commonwealth Prime");
    assert_eq!(body["duration_seconds"], 600);
    assert_eq!(body["balance"], Value::Null, "PvE has no rating context");

    let players = body["participants"].as_array().expect("participants");
    assert_eq!(players.len(), 2);

    let seven = players
        .iter()
        .find(|p| p["player"] == "Seven")
        .expect("Seven");
    // INTEGER columns must decode; a silent zero here is the bug this guards.
    assert_eq!(seven["kills"], 12);
    assert_eq!(seven["damage_dealt"], 480_000);
    assert_eq!(seven["bot_kills"], 34);
    assert_eq!(seven["damage_taken"], 5_000);
    assert_eq!(seven["healing"], 2_500);
    assert_eq!(seven["classes"][0], "Assault");
    assert_eq!(seven["task_force"], 1);
    assert_eq!(seven["won"], true);
    assert_eq!(seven["mmr"], Value::Null, "PvE carries no ratings");
}

#[tokio::test]
async fn a_pvp_match_splits_sides_and_carries_the_ratings() {
    let harness = Harness::new();
    let (router, _) = harness.build().await;

    let (status, body) = get(&router, "/api/matches/1002").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "pvp");
    assert_eq!(body["mmr_engine"], "wl");

    let players = body["participants"].as_array().expect("participants");
    let winner = players
        .iter()
        .find(|p| p["player"] == "Seven")
        .expect("Seven");
    let loser = players
        .iter()
        .find(|p| p["player"] == "Eight")
        .expect("Eight");
    assert_eq!(winner["won"], true);
    assert_eq!(winner["mmr"], 1040.0);
    assert_eq!(winner["mmr_class"], "Assault");
    // What the match did to the rating: the engine has both sides of the change.
    assert_eq!(winner["mmr_delta"], 20.0);
    assert_eq!(loser["won"], false);
    assert_eq!(loser["mmr"], 1120.0);
    assert_eq!(loser["mmr_delta"], -20.0);

    // The balance uses pre-match ratings, so an upset shows a negative delta.
    let balance = &body["balance"];
    assert_eq!(balance["wt_avg"], 1040.0);
    assert_eq!(balance["lt_avg"], 1120.0);
    assert_eq!(balance["delta"], -80.0);
    assert!(balance["wt_prob"].as_f64().expect("prob") < 50.0);
}

#[tokio::test]
async fn an_unknown_or_unsealed_match_is_a_404() {
    let harness = Harness::new();
    let (router, _) = harness.build().await;

    for uri in [
        "/api/matches/9999",
        "/api/matches/0",
        "/api/matches/not-a-number",
    ] {
        let (status, _) = get(&router, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn the_archive_lists_match_ids_for_the_detail_page() {
    let harness = Harness::new();
    let (router, _) = harness.build().await;

    let (status, body) = get(&router, "/api/matches/recent?limit=5").await;
    assert_eq!(status, StatusCode::OK);
    let matches = body["matches"].as_array().expect("matches");
    assert!(!matches.is_empty());
    for entry in matches {
        assert!(
            entry["instance_id"].as_i64().unwrap_or(0) > 0,
            "every archive row needs the id its detail page lives at"
        );
        assert_eq!(
            entry["viewer"],
            Value::Null,
            "signed out: the list carries no personal result"
        );
    }
}

#[tokio::test]
async fn signing_in_marks_your_result_in_the_lists() {
    let harness = Harness::new();
    let (router, live_path) = harness.build().await;

    // A session for user 7, written straight into a site DB — the point here is
    // the result column, not the credential exchange (covered in tests/auth.rs).
    let site_path = harness.path("site.db");
    let site = db::open_read_write(&site_path, 2).await.expect("site db");
    auth::migrate(&site).await.expect("migrate");
    let token = auth::create_session(&site, 7, "Seven")
        .await
        .expect("session");
    site.close().await;

    let state = {
        let pool = db::open_read_only(&live_path, 2).await.expect("live");
        let site_pool = db::open_read_only(&site_path, 2).await.expect("site");
        AppState::new(
            Config {
                bind: "127.0.0.1:0".parse().expect("addr"),
                live_db: Some(live_path.clone()),
                snapshot_db: None,
                site_db: Some(site_path.clone()),
                files_dir: harness.path("files"),
                status_ttl: std::time::Duration::from_secs(0),
                live_tick: std::time::Duration::from_secs(15),
                match_ttl: std::time::Duration::from_secs(0),
                history_ttl: std::time::Duration::from_secs(0),
                status_override: None,
                control_api_url: None,
                control_api_token: None,
                db_export_token: None,
                news_posting_mode: webapi::news::PostingMode::Open,
                site_url: "https://example.test".to_string(),
            },
            Some(pool),
            None,
            Some(site_pool),
        )
    };
    let signed_in = app(state);
    let _ = router;

    let request = Request::builder()
        .uri("/api/matches/recent?limit=5")
        .header(header::COOKIE, format!("cw_session={token}"))
        .body(Body::empty())
        .expect("request");
    let response = signed_in.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4 << 20)
        .await
        .expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");

    let pvp = json["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .find(|m| m["instance_id"] == 1002)
        .expect("the PvP match");
    assert_eq!(pvp["viewer"]["user_id"], 7);
    assert_eq!(pvp["viewer"]["won"], true);

    let pve = json["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .find(|m| m["instance_id"] == 1001)
        .expect("the PvE match");
    assert_eq!(pve["viewer"]["won"], true);

    // And the detail page marks the viewer's own row.
    let request = Request::builder()
        .uri("/api/matches/1002")
        .header(header::COOKIE, format!("cw_session={token}"))
        .body(Body::empty())
        .expect("request");
    let response = signed_in.oneshot(request).await.expect("response");
    let body = axum::body::to_bytes(response.into_body(), 4 << 20)
        .await
        .expect("body");
    let detail: Value = serde_json::from_slice(&body).expect("json");

    assert_eq!(detail["viewer"]["user_id"], 7);
    assert_eq!(detail["viewer"]["won"], true);
    let flagged: Vec<&Value> = detail["participants"]
        .as_array()
        .expect("participants")
        .iter()
        .filter(|p| p["is_viewer"] == true)
        .collect();
    assert_eq!(flagged.len(), 1, "exactly one row is yours");
    assert_eq!(flagged[0]["player"], "Seven");
}
