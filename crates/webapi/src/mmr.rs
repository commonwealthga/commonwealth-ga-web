//! Ratings, in one place.
//!
//! The control server feeds one of two history tables depending on the engine
//! it is running (`cs_settings.active_mmr_engine`): the win/loss engine writes
//! `ga_wl_mmr_history`, the older performance engine writes `ga_mmr_history`.
//! They use different scales and the win/loss table has no `games_after`
//! column, so every read has to know which one is live — reading the wrong one
//! produces a plausible-looking but wrong number.
//!
//! Everything that shows a rating goes through here: match detail, a player's
//! career, an agent's profile, and the leaderboards, so they can never disagree.

use sqlx::Row;
use sqlx::sqlite::SqlitePool;

/// The class names the game writes into the history tables, in the order the
/// site displays them.
pub const CLASSES: [&str; 4] = ["Assault", "Medic", "Recon", "Robotic"];

/// Ratings a brand-new account starts on.
pub const STARTING_RATING: f64 = 1000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    WinLoss,
    Performance,
}

impl Engine {
    pub fn table(self) -> &'static str {
        match self {
            // The live table has no games_after; see `games_expr`.
            Engine::WinLoss => "ga_wl_mmr_history",
            Engine::Performance => "ga_mmr_history",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Engine::WinLoss => "wl",
            Engine::Performance => "perf",
        }
    }

    fn from_code(code: Option<&str>) -> Engine {
        match code {
            Some("perf") => Engine::Performance,
            _ => Engine::WinLoss,
        }
    }

    /// `games_after` exists only on the performance table. On the win/loss
    /// engine a class's game count is the number of rows it has.
    fn games_expr(self) -> &'static str {
        match self {
            Engine::WinLoss => "COUNT(*) OVER (PARTITION BY user_id, class_name)",
            Engine::Performance => "games_after",
        }
    }
}

/// Which engine is live, and therefore which table to read.
///
/// Unknown or unreadable settings fall back to the win/loss engine, which is
/// what the server runs today, and the caller keeps working if the table is
/// missing (a snapshot from an older build simply yields no ratings).
pub async fn engine(live: &SqlitePool) -> Engine {
    let code: Option<String> =
        sqlx::query("SELECT value FROM cs_settings WHERE key = 'active_mmr_engine'")
            .fetch_optional(live)
            .await
            .ok()
            .flatten()
            .and_then(|row| row.try_get::<String, _>("value").ok());

    Engine::from_code(code.as_deref())
}

/// One rating, as of the newest row in the history table.
#[derive(Debug, Clone)]
pub struct ClassRating {
    pub class_name: String,
    pub mmr: f64,
    pub games: i64,
    pub played_at: i64,
}

/// A player's current rating per class, newest row per class.
pub async fn latest_for_user(live: &SqlitePool, user_id: i64) -> Vec<ClassRating> {
    let engine = engine(live).await;
    latest_for_user_on(live, engine, user_id).await
}

async fn latest_for_user_on(live: &SqlitePool, engine: Engine, user_id: i64) -> Vec<ClassRating> {
    let sql = format!(
        "SELECT class_name, mmr_after, played_at, {} AS games
           FROM {table}
          WHERE user_id = ?
          ORDER BY played_at DESC, rowid DESC",
        engine.games_expr(),
        table = engine.table()
    );

    let rows = match sqlx::query(&sql).bind(user_id).fetch_all(live).await {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    let mut seen: Vec<String> = Vec::new();
    let mut ratings: Vec<ClassRating> = Vec::new();
    for row in &rows {
        let Some(class_name) = row
            .try_get::<Option<String>, _>("class_name")
            .ok()
            .flatten()
        else {
            continue;
        };
        if seen.contains(&class_name) {
            continue;
        }
        seen.push(class_name.clone());
        ratings.push(ClassRating {
            mmr: row
                .try_get::<Option<f64>, _>("mmr_after")
                .ok()
                .flatten()
                .unwrap_or(STARTING_RATING),
            games: row
                .try_get::<Option<i64>, _>("games")
                .ok()
                .flatten()
                .unwrap_or(0),
            played_at: row
                .try_get::<Option<i64>, _>("played_at")
                .ok()
                .flatten()
                .unwrap_or(0),
            class_name,
        });
    }

    // Display order: the game's class order, anything unexpected last.
    ratings.sort_by_key(|rating| {
        CLASSES
            .iter()
            .position(|class| *class == rating.class_name)
            .unwrap_or(CLASSES.len())
    });
    ratings
}

/// One row of a class leaderboard.
#[derive(Debug, Clone)]
pub struct Standing {
    pub user_id: i64,
    pub username: String,
    pub mmr: f64,
    pub games: i64,
    pub played_at: i64,
}

/// Current standings for one class, best first.
pub async fn standings(
    live: &SqlitePool,
    class_name: &str,
    limit: i64,
    offset: i64,
) -> Vec<Standing> {
    let engine = engine(live).await;
    standings_on(live, engine, class_name, limit, offset).await
}

async fn standings_on(
    live: &SqlitePool,
    engine: Engine,
    class_name: &str,
    limit: i64,
    offset: i64,
) -> Vec<Standing> {
    // Newest row per (user, class), then ordered by rating.
    let sql = format!(
        "WITH latest AS (
             SELECT user_id, mmr_after, played_at,
                    ROW_NUMBER() OVER (PARTITION BY user_id ORDER BY played_at DESC, rowid DESC) AS rn
               FROM {table}
              WHERE class_name = ?
         )
         SELECT latest.user_id, u.username, latest.mmr_after, latest.played_at
           FROM latest
           JOIN ga_users u ON u.id = latest.user_id
          WHERE latest.rn = 1
          ORDER BY latest.mmr_after DESC, latest.played_at DESC
          LIMIT ? OFFSET ?",
        table = engine.table()
    );

    let rows = match sqlx::query(&sql)
        .bind(class_name)
        .bind(limit)
        .bind(offset)
        .fetch_all(live)
        .await
    {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    rows.iter()
        .map(|row| Standing {
            user_id: row.try_get("user_id").unwrap_or(0),
            username: row
                .try_get::<Option<String>, _>("username")
                .ok()
                .flatten()
                .unwrap_or_default(),
            mmr: row
                .try_get::<Option<f64>, _>("mmr_after")
                .ok()
                .flatten()
                .unwrap_or(STARTING_RATING),
            games: 0,
            played_at: row
                .try_get::<Option<i64>, _>("played_at")
                .ok()
                .flatten()
                .unwrap_or(0),
        })
        .collect()
}

/// How many players are rated in a class.
pub async fn count_rated(live: &SqlitePool, class_name: &str) -> i64 {
    let engine = engine(live).await;
    let sql = format!(
        "SELECT COUNT(DISTINCT user_id) AS total FROM {} WHERE class_name = ?",
        engine.table()
    );
    sqlx::query_scalar(&sql)
        .bind(class_name)
        .fetch_one(live)
        .await
        .ok()
        .flatten()
        .unwrap_or(0)
}

/// How many players are rated in a class, and where this player sits.
pub async fn rank_for_user(live: &SqlitePool, user_id: i64) -> Vec<(String, i64, i64)> {
    let engine = engine(live).await;
    let mut out = Vec::new();

    for class_name in CLASSES {
        let board = standings_on(live, engine, class_name, 10_000, 0).await;
        if board.is_empty() {
            continue;
        }
        if let Some(index) = board.iter().position(|row| row.user_id == user_id) {
            out.push((class_name.to_string(), index as i64 + 1, board.len() as i64));
        }
    }

    out
}

/// A player's rating over time, oldest first, optionally for one class.
#[derive(Debug, Clone)]
pub struct SeriesPoint {
    pub played_at: i64,
    pub mmr: f64,
    pub instance_id: i64,
}

pub async fn series_for_user(
    live: &SqlitePool,
    user_id: i64,
    class_name: Option<&str>,
) -> Vec<SeriesPoint> {
    let engine = engine(live).await;
    let sql = match class_name {
        Some(_) => format!(
            "SELECT played_at, mmr_after, instance_id FROM {} 
              WHERE user_id = ? AND class_name = ?
              ORDER BY played_at ASC, rowid ASC",
            engine.table()
        ),
        None => format!(
            "SELECT played_at, mmr_after, instance_id FROM {} 
              WHERE user_id = ?
              ORDER BY played_at ASC, rowid ASC",
            engine.table()
        ),
    };

    let mut query = sqlx::query(&sql).bind(user_id);
    if let Some(class_name) = class_name {
        query = query.bind(class_name);
    }

    let rows = match query.fetch_all(live).await {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    rows.iter()
        .filter_map(|row| {
            Some(SeriesPoint {
                played_at: row.try_get::<Option<i64>, _>("played_at").ok().flatten()?,
                mmr: row.try_get::<Option<f64>, _>("mmr_after").ok().flatten()?,
                instance_id: row
                    .try_get::<Option<i64>, _>("instance_id")
                    .ok()
                    .flatten()
                    .unwrap_or(0),
            })
        })
        .collect()
}

/// Ratings for one match's participants, with the previous site's fallback
/// chain: this match's own `mmr_before`, else the most recent earlier rating
/// for the player's main class, else the engine's starting rating.
pub async fn pre_match(
    live: &SqlitePool,
    instance_id: i64,
    players: &[(i64, Option<String>)],
    started_at: i64,
) -> (
    Engine,
    std::collections::HashMap<i64, (f64, Option<f64>, String)>,
) {
    use std::collections::HashMap;

    let engine = engine(live).await;
    let table = engine.table();
    let mut out: HashMap<i64, (f64, Option<f64>, String)> = HashMap::new();

    let direct = sqlx::query(&format!(
        "SELECT user_id, class_name, mmr_before, mmr_after FROM {table} WHERE instance_id = ?"
    ))
    .bind(instance_id)
    .fetch_all(live)
    .await;

    if let Ok(rows) = &direct {
        for row in rows {
            let user_id: i64 = row.try_get("user_id").unwrap_or(0);
            let class: Option<String> = row.try_get("class_name").ok();
            let before: Option<f64> = row.try_get("mmr_before").ok();
            let after: Option<f64> = row.try_get("mmr_after").ok();
            if let (Some(before), Some(class)) = (before, class) {
                out.insert(user_id, (round1(before), after.map(round1), class));
            }
        }
    }

    for (user_id, class) in players {
        if out.contains_key(user_id) {
            continue;
        }
        let Some(class) = class.as_deref() else {
            continue;
        };
        let fallback = sqlx::query(&format!(
            "SELECT mmr_after FROM {table}
              WHERE user_id = ? AND class_name = ? AND played_at < ?
              ORDER BY played_at DESC, rowid DESC LIMIT 1"
        ))
        .bind(user_id)
        .bind(class)
        .bind(started_at)
        .fetch_optional(live)
        .await;

        if let Ok(Some(row)) = fallback
            && let Ok(Some(value)) = row.try_get::<Option<f64>, _>("mmr_after")
        {
            out.insert(*user_id, (round1(value), None, class.to_string()));
            continue;
        }
        out.insert(*user_id, (STARTING_RATING, None, class.to_string()));
    }

    (engine, out)
}

pub fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// Elo expectation for the left side, as a percentage rounded to 0.1.
pub fn win_probability(left: f64, right: f64) -> f64 {
    let expected = 1.0 / (1.0 + 10f64.powf((right - left) / 400.0));
    (expected * 1000.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_code_selects_the_matching_table() {
        assert_eq!(Engine::from_code(Some("wl")).table(), "ga_wl_mmr_history");
        assert_eq!(Engine::from_code(Some("perf")).table(), "ga_mmr_history");
        // Unknown or absent settings behave like the live deployment.
        assert_eq!(Engine::from_code(None).table(), "ga_wl_mmr_history");
        assert_eq!(Engine::from_code(Some("nonsense")).code(), "wl");
    }

    #[test]
    fn equal_ratings_are_a_coin_flip() {
        assert_eq!(win_probability(1000.0, 1000.0), 50.0);
        assert!(win_probability(1200.0, 1000.0) > 50.0);
        assert!(win_probability(900.0, 1100.0) < 50.0);
    }
}
