//! Public agent profiles, directory, leaderboards and rating history.

use std::path::PathBuf;

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

    async fn build(&self) -> Router {
        let live_path = self.path("server.db");
        let live = db::open_read_write(&live_path, 2).await.expect("live db");

        for ddl in [
            "CREATE TABLE ga_users (id INTEGER PRIMARY KEY, username TEXT)",
            "CREATE TABLE ga_instances (id INTEGER PRIMARY KEY, instance_id INTEGER NOT NULL,
                                        map_name TEXT NOT NULL, game_mode TEXT NOT NULL,
                                        started_at INTEGER NOT NULL, sealed_at INTEGER,
                                        outcome TEXT, winning_task_force INTEGER,
                                        is_home_map INTEGER NOT NULL DEFAULT 0, queue_id INTEGER)",
            "CREATE TABLE ga_match_player_stats (
                 instance_id INTEGER NOT NULL, user_id INTEGER NOT NULL, character_id INTEGER NOT NULL,
                 task_force INTEGER, rep_points REAL, kills INTEGER, assists INTEGER,
                 damage_taken INTEGER, damage_dealt INTEGER, buff_value INTEGER, healing INTEGER,
                 defense INTEGER, deaths INTEGER, obj_points INTEGER, bot_kills INTEGER,
                 capture_seconds REAL, contest_seconds REAL, objective_captures INTEGER,
                 beacon_spawns_provided INTEGER, beacon_spawns_used INTEGER,
                 beacons_destroyed INTEGER, time_played_seconds REAL)",
            "CREATE TABLE ga_queues (queue_id INTEGER, name TEXT, name_msg_id INTEGER,
                                     desc_msg_id INTEGER, location_value_id INTEGER)",
            "CREATE TABLE map_game_info (map_name TEXT, friendly_name_msg_id INTEGER)",
            "CREATE TABLE asm_data_set_msg_translations (msg_id INTEGER, message TEXT)",
            "CREATE TABLE asm_data_set_valid_values (value_id INTEGER, text_msg_translated TEXT)",
            "CREATE TABLE cs_settings (key TEXT PRIMARY KEY, value TEXT)",
            "CREATE TABLE ga_wl_mmr_history (user_id INTEGER, class_name TEXT, instance_id INTEGER,
                                             played_at INTEGER, mmr_before REAL, mmr_after REAL)",
        ] {
            sqlx::query(ddl).execute(&live).await.expect("ddl");
        }

        sqlx::query("INSERT INTO cs_settings VALUES ('active_mmr_engine', 'wl')")
            .execute(&live)
            .await
            .expect("engine");

        // Three accounts: two active today, one long dormant, one with no games.
        // Columns are named: the real table has more of them than a fixture
        // cares about, and positional inserts break when the schema is corrected.
        for (id, name) in [
            (1_i64, "Older"),
            (2, "Zaxik"),
            (3, "Newcomer"),
            (4, "Ghost"),
        ] {
            sqlx::query("INSERT INTO ga_users (id, username) VALUES (?, ?)")
                .bind(id)
                .bind(name)
                .execute(&live)
                .await
                .expect("user");
        }

        sqlx::query("INSERT INTO ga_queues (queue_id, name) VALUES (1, 'umax')")
            .execute(&live)
            .await
            .expect("queue");
        sqlx::query("INSERT INTO ga_queues (queue_id, name) VALUES (2, 'merc')")
            .execute(&live)
            .await
            .expect("queue");

        for (id, instance, started, queue) in [
            (1_i64, 500_i64, 1_700_000_000_i64, 1_i64),
            (2, 501, 1_700_100_000, 1),
            // Mercenary: this is the one that counts as PvP.
            (3, 502, 1_700_200_000, 2),
        ] {
            // (A queue-less instance is added further down, on purpose.)
            sqlx::query(
                "INSERT INTO ga_instances VALUES (?, ?, 'Map_A', 'TgGame_Mission', ?, ?, 'ATTACKERS_WIN', 1, 0, ?)",
            )
            .bind(id)
            .bind(instance)
            .bind(started)
            .bind(started + 600)
            .bind(queue)
            .execute(&live)
            .await
            .expect("instance");
        }

        // Zaxik played the recent match, Older only the first one.
        for (instance, user, kills, damage) in [
            (500_i64, 1_i64, 5_i64, 1_000_i64),
            (500, 2, 9, 2_000),
            (501, 2, 12, 3_000),
            // The player's best game by kills, and the only PvP one.
            (502, 2, 30, 9_000),
        ] {
            sqlx::query(
                "INSERT INTO ga_match_player_stats
                 (instance_id, user_id, character_id, task_force, rep_points, kills, assists,
                  damage_taken, damage_dealt, buff_value, healing, defense, deaths, obj_points,
                  bot_kills, capture_seconds, contest_seconds, objective_captures,
                  beacon_spawns_provided, beacon_spawns_used, beacons_destroyed, time_played_seconds)
                 VALUES (?, ?, 1, 1, 0, ?, 1, 100, ?, 0, 0, 0, 2, 10, 3, 0, 0, 0, 0, 0, 1, 300.0)",
            )
            .bind(instance)
            .bind(user)
            .bind(kills)
            .bind(damage)
            .execute(&live)
            .await
            .expect("stats");
        }

        // Ratings: Zaxik is the better Medic, Older leads Assault.
        for (user, class, instance, at, before, after) in [
            (1_i64, "Assault", 500_i64, 1_700_000_000_i64, 1000.0, 1030.0),
            (2, "Assault", 501, 1_700_100_000, 1040.0, 1036.0),
            (2, "Medic", 500, 1_700_000_000, 1100.0, 1120.0),
            (2, "Medic", 501, 1_700_100_000, 1120.0, 1140.0),
        ] {
            sqlx::query("INSERT INTO ga_wl_mmr_history VALUES (?, ?, ?, ?, ?, ?)")
                .bind(user)
                .bind(class)
                .bind(instance)
                .bind(at)
                .bind(before)
                .bind(after)
                .execute(&live)
                .await
                .expect("mmr");
        }

        live.close().await;
        let pool = db::open_read_only(&live_path, 2).await.expect("reopen");

        app(AppState::new(
            Config {
                bind: "127.0.0.1:0".parse().expect("addr"),
                live_db: Some(live_path.clone()),
                snapshot_db: None,
                site_db: None,
                static_dir: self.path("static"),
                files_dir: self.path("files"),
                status_ttl: std::time::Duration::from_secs(0),
                live_tick: std::time::Duration::from_secs(15),
                match_ttl: std::time::Duration::from_secs(0),
                history_ttl: std::time::Duration::from_secs(0),
                status_override: None,
                control_api_url: None,
                control_api_token: None,
                db_export_token: None,
                news_posting_mode: webapi::news::PostingMode::Open,
            },
            Some(pool),
            None,
            None,
        ))
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
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn the_directory_is_sorted_by_last_active_and_searchable() {
    let harness = Harness::new();
    let router = harness.build().await;

    let (status, body) = get(&router, "/api/agents").await;
    assert_eq!(status, StatusCode::OK);
    // Accounts with no recorded match are not in the directory.
    assert_eq!(body["total"], 2);
    let agents = body["agents"].as_array().expect("agents");
    assert_eq!(agents[0]["username"], "Zaxik", "most recently active first");
    assert_eq!(agents[0]["matches"], 3);
    // Damage, not kills: PvE missions put their kills in bot_kills.
    assert_eq!(agents[0]["damage_dealt"], 14_000);
    assert_eq!(agents[1]["username"], "Older");
    assert!(
        agents[0]["last_active"].as_i64().unwrap() > agents[1]["last_active"].as_i64().unwrap()
    );

    let (status, body) = get(&router, "/api/agents?q=zax").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 1);
    assert_eq!(body["agents"][0]["username"], "Zaxik");

    // Searching for someone who never played still finds the account.
    let (status, body) = get(&router, "/api/agents?q=ghost").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 0);
}

#[tokio::test]
async fn a_profile_carries_career_totals_and_ranked_ratings() {
    let harness = Harness::new();
    let router = harness.build().await;

    let (status, body) = get(&router, "/api/agents/2").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["username"], "Zaxik");
    assert_eq!(
        body["totals"]["all"]["matches"], 3,
        "the queue-less instance is not a match"
    );
    assert_eq!(body["totals"]["all"]["kills"], 51);
    assert_eq!(body["totals"]["all"]["damage_dealt"], 14_000);

    // Blended totals hid the difference: the PvP and PvE halves are reported
    // separately so a PvE-heavy player's deaths do not skew their PvP record.
    assert_eq!(body["totals"]["pvp"]["matches"], 1);
    assert_eq!(body["totals"]["pvp"]["kills"], 30);
    assert_eq!(body["totals"]["pve"]["matches"], 2);
    assert_eq!(body["totals"]["pve"]["kills"], 21);
    // The map join must not fan out: Map_A has two map_game_info rows.
    assert_eq!(body["totals"]["all"]["seconds_played"], 900);
    // Wins are counted per match, so the UI can show a rate rather than a count.
    assert_eq!(body["totals"]["all"]["wins"], 3);
    assert_eq!(body["totals"]["pvp"]["wins"], 1);
    // Damage taken is reported separately: it is not a score, but players ask.
    assert_eq!(body["totals"]["all"]["damage_taken"], 300);

    // Best single match per statistic, pointing at the match itself.
    let best_kills = body["bests"]["pvp"]
        .as_array()
        .expect("bests")
        .iter()
        .find(|b| b["stat"] == "kills")
        .expect("best pvp kills");
    assert_eq!(best_kills["value"], 30);
    assert_eq!(best_kills["instance_id"], 502);
    assert_eq!(best_kills["queue_code"], "merc");

    let best_taken = body["bests"]["all"]
        .as_array()
        .expect("bests")
        .iter()
        .find(|b| b["stat"] == "damage_taken")
        .expect("best damage taken");
    assert_eq!(best_taken["value"], 100);

    let ratings = body["ratings"].as_array().expect("ratings");
    assert_eq!(ratings.len(), 2);
    // Class order, not alphabetical or by rating.
    assert_eq!(ratings[0]["class_name"], "Assault");
    let medic = ratings
        .iter()
        .find(|r| r["class_name"] == "Medic")
        .expect("medic");
    assert_eq!(medic["mmr"], 1140.0, "newest row wins");
    assert_eq!(medic["rank"], 1, "the only rated Medic is first");
    assert_eq!(medic["ranked_players"], 1);

    let assault = ratings
        .iter()
        .find(|r| r["class_name"] == "Assault")
        .expect("assault");
    // Zaxik's newest Assault row is 1036, Older's is 1030.
    assert_eq!(assault["rank"], 1);
    assert_eq!(assault["ranked_players"], 2);
}

#[tokio::test]
async fn an_instance_without_a_queue_has_no_match_page() {
    let harness = Harness::new();
    let router = harness.build().await;

    let (status, _) = get(&router, "/api/matches/503").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // And it is not in the archive.
    let (status, body) = get(&router, "/api/matches/recent?limit=50").await;
    assert_eq!(status, StatusCode::OK);
    let listed: Vec<i64> = body["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .filter_map(|m| m["instance_id"].as_i64())
        .collect();
    assert!(!listed.contains(&503), "got {listed:?}");
}

#[tokio::test]
async fn an_unknown_agent_is_a_404() {
    let harness = Harness::new();
    let router = harness.build().await;

    for uri in ["/api/agents/999", "/api/agents/none", "/api/agents/0"] {
        let (status, _) = get(&router, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn the_leaderboard_ranks_a_class() {
    let harness = Harness::new();
    let router = harness.build().await;

    let (status, body) = get(&router, "/api/leaderboard?class=Assault").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["class_name"], "Assault");
    assert_eq!(body["total"], 2);
    let entries = body["entries"].as_array().expect("entries");
    assert_eq!(entries[0]["username"], "Zaxik");
    assert_eq!(entries[0]["rank"], 1);
    assert_eq!(entries[1]["username"], "Older");
    assert_eq!(entries[1]["rank"], 2);

    // Paging keeps absolute ranks.
    let (_, body) = get(&router, "/api/leaderboard?class=Assault&limit=1&page=1").await;
    assert_eq!(body["entries"][0]["rank"], 2);
    assert_eq!(body["entries"][0]["username"], "Older");
}

#[tokio::test]
async fn rating_history_is_a_series_and_can_be_filtered_by_class() {
    let harness = Harness::new();
    let router = harness.build().await;

    let (status, body) = get(&router, "/api/agents/2/history").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["engine"], "wl");
    assert_eq!(body["times"].as_array().expect("times").len(), 3);
    // Oldest first, so the chart can draw left to right.
    let times: Vec<i64> = body["times"]
        .as_array()
        .expect("times")
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
    // Every class at once: the oldest point is the first Medic row.
    assert_eq!(body["mmr"][0], 1120.0);

    let (_, body) = get(&router, "/api/agents/2/history?class=Medic").await;
    assert_eq!(body["class_name"], "Medic");
    assert_eq!(body["times"].as_array().expect("times").len(), 2);
    assert_eq!(body["mmr"][1], 1140.0);

    // A window shorter than the history is anchored on the newest point, so it
    // keeps both rows from the final match and drops the older one.
    let (_, body) = get(&router, "/api/agents/2/history?days=1").await;
    assert_eq!(body["times"].as_array().expect("times").len(), 2);
}

#[tokio::test]
async fn an_agents_matches_are_public_and_filterable() {
    let harness = Harness::new();
    let router = harness.build().await;

    let (status, body) = get(&router, "/api/agents/2/matches").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3, "queue-less instances are excluded");
    assert_eq!(body["matches"][0]["instance_id"], 502, "newest first");

    let (_, body) = get(&router, "/api/agents/2/matches?kind=pvp").await;
    assert_eq!(body["total"], 1);
    let (_, body) = get(&router, "/api/agents/2/matches?kind=pve").await;
    assert_eq!(body["total"], 2);
}
