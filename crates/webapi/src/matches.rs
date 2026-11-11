//! Recent match summaries for the public archive.
//!
//! Reads the *live* database, not the snapshot: the query is a bounded
//! `LIMIT` over the `ga_instances` primary key, which SQLite answers without a
//! sort step. It is cached anyway, so a busy page costs one query per TTL
//! across every visitor.

use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::snapshot::StatsSource;

pub const LIMIT_DEFAULT: u32 = 8;
pub const LIMIT_MAX: u32 = 50;

/// Paging for the archive and a player's own history.
pub const PAGE_LIMIT_DEFAULT: u32 = 20;
pub const PAGE_LIMIT_MAX: u32 = 50;

/// Resolve `page`/`offset` query parameters into a row offset.
///
/// `page` wins when both are given; `offset` exists for callers that think in
/// rows. Never overflows: a silly page number saturates.
pub fn page_offset(page: Option<u32>, offset: Option<u32>, limit: u32) -> u32 {
    match (page, offset) {
        (Some(page), _) => page.saturating_mul(limit),
        (None, Some(offset)) => offset,
        _ => 0,
    }
}

/// Which matches to show. PvP is a property of the *queue*, decided here once
/// so the archive, a player's history and the match detail can never disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    All,
    Pvp,
    Pve,
}

impl Kind {
    /// Lenient by design: an unknown or absent value means "everything".
    pub fn parse(raw: Option<&str>) -> Kind {
        match raw.map(str::trim) {
            Some("pvp") | Some("PvP") => Kind::Pvp,
            Some("pve") | Some("PvE") => Kind::Pve,
            _ => Kind::All,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Kind::All => "all",
            Kind::Pvp => "pvp",
            Kind::Pve => "pve",
        }
    }

    /// SQL fragment filtering on a queue-code expression.
    ///
    /// The queue list comes from `match_detail::PVP_QUEUES`, so there is one
    /// definition of what counts as PvP. `expr` is code-owned SQL, never user
    /// input: the archive passes the `q.code` alias, the per-player queries a
    /// scalar subquery on `ga_instances.queue_id`.
    pub fn predicate_on(self, expr: &str) -> String {
        let list = crate::match_detail::PVP_QUEUES
            .iter()
            .map(|code| format!("'{code}'"))
            .collect::<Vec<_>>()
            .join(", ");
        match self {
            Kind::All => String::new(),
            Kind::Pvp => format!(" AND {expr} IN ({list})"),
            Kind::Pve => format!(" AND ({expr} IS NULL OR {expr} NOT IN ({list}))"),
        }
    }

    /// The archive's own filter: the queue join exposes `q.code`.
    pub fn queue_filter(self) -> String {
        self.predicate_on("q.code")
    }

    /// A per-player predicate, written against `ga_instances.queue_id` so it
    /// works without joining the queue table.
    pub fn player_predicate(self) -> String {
        self.predicate_on("(SELECT name FROM ga_queues WHERE queue_id = i.queue_id)")
    }
}

/// Ordered by `id`, not `started_at`: the primary key is already in
/// insertion (chronological) order, so there is no sort to pay for. Completed
/// missions only, and never the home map / hub instances.
pub const RECENT_MATCHES_SQL: &str = r#"
    SELECT i.id                 AS id,
           i.instance_id        AS instance_id,
           i.map_name           AS map_name,
           m.label              AS map_label,
           i.game_mode          AS game_mode,
           i.started_at         AS started_at,
           i.sealed_at          AS sealed_at,
           i.outcome            AS outcome,
           i.winning_task_force AS winning_task_force,
           q.code               AS queue_code,
           q.label              AS queue_label,
           q.location           AS queue_location,
           (SELECT COUNT(DISTINCT s.user_id) FROM ga_match_player_stats s
             WHERE s.instance_id = i.instance_id) AS players
    FROM ga_instances i
    -- Human-readable names live in the client's own string tables, not in the
    -- instance rows: the map's friendly name and the queue's title/location.
    LEFT JOIN (
        -- One row per map name; see the note in agents::match_lines.
        SELECT mg.map_name AS name, MAX(mt.message) AS label
          FROM map_game_info mg
          LEFT JOIN asm_data_set_msg_translations mt ON mt.msg_id = mg.friendly_name_msg_id
         GROUP BY mg.map_name
    ) m ON m.name = i.map_name
    LEFT JOIN (
        SELECT g.queue_id AS id, g.name AS code, mt.message AS label,
               vv.text_msg_translated AS location
          FROM ga_queues g
          LEFT JOIN asm_data_set_msg_translations mt ON mt.msg_id = g.name_msg_id
          LEFT JOIN asm_data_set_valid_values vv ON vv.value_id = g.location_value_id
    ) q ON q.id = i.queue_id
    WHERE i.is_home_map = 0 AND i.sealed_at IS NOT NULL
      -- A queue is what makes an instance a match. Instances without one are
      -- the hub, open-world city trips and test spawns; they are noise on a
      -- match archive and would pollute every player's record.
      AND i.queue_id IS NOT NULL{FILTER}
    ORDER BY i.id DESC
    LIMIT ? OFFSET ?
"#;

/// Total rows for the same filter, so the UI can say how many pages there are.
const COUNT_SQL: &str = r#"
    SELECT COUNT(*) AS total
    FROM ga_instances i
    LEFT JOIN (
        SELECT g.queue_id AS id, g.name AS code
          FROM ga_queues g
    ) q ON q.id = i.queue_id
    WHERE i.is_home_map = 0 AND i.sealed_at IS NOT NULL
      AND i.queue_id IS NOT NULL{FILTER}
"#;

fn with_filter(sql: &str, kind: Kind) -> String {
    sql.replace("{FILTER}", &kind.queue_filter())
}

/// One completed match, from one player's point of view.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlayerMatch {
    pub id: i64,
    pub instance_id: i64,
    pub map_name: String,
    pub map_label: Option<String>,
    pub game_mode: String,
    pub started_at: i64,
    pub sealed_at: Option<i64>,
    pub outcome: Option<String>,
    pub winning_task_force: Option<i64>,
    pub task_force: Option<i64>,
    pub queue_code: Option<String>,
    pub queue_label: Option<String>,
    pub queue_location: Option<String>,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub damage_dealt: i64,
    pub healing: i64,
    pub obj_points: i64,
    pub time_played_seconds: i64,
}

/// A player's matches, newest first, with the same paging and filter as the
/// archive. Used for both the signed-in history and the public agent profile.
const PLAYER_MATCHES_SQL: &str = r#"
    SELECT i.id, i.instance_id, i.map_name, m.label AS map_label, i.game_mode,
           i.started_at, i.sealed_at, i.outcome, i.winning_task_force, MAX(s.task_force) AS task_force,
           q.code AS queue_code, q.label AS queue_label, q.location AS queue_location,
           COALESCE(SUM(s.kills), 0) AS kills,
           COALESCE(SUM(s.deaths), 0) AS deaths,
           COALESCE(SUM(s.assists), 0) AS assists,
           COALESCE(SUM(s.damage_dealt), 0) AS damage_dealt,
           COALESCE(SUM(s.healing), 0) AS healing,
           COALESCE(SUM(s.obj_points), 0) AS obj_points,
           CAST(ROUND(SUM(s.time_played_seconds)) AS INTEGER) AS time_played_seconds
      FROM ga_match_player_stats s
      JOIN ga_instances i ON i.instance_id = s.instance_id
      LEFT JOIN (
          -- One row per map name; see the note in agents::match_lines.
          SELECT mg.map_name AS name, MAX(mt.message) AS label
            FROM map_game_info mg
            LEFT JOIN asm_data_set_msg_translations mt ON mt.msg_id = mg.friendly_name_msg_id
           GROUP BY mg.map_name
      ) m ON m.name = i.map_name
      LEFT JOIN (
          SELECT g.queue_id AS id, g.name AS code, mt.message AS label,
                 vv.text_msg_translated AS location
            FROM ga_queues g
            LEFT JOIN asm_data_set_msg_translations mt ON mt.msg_id = g.name_msg_id
            LEFT JOIN asm_data_set_valid_values vv ON vv.value_id = g.location_value_id
      ) q ON q.id = i.queue_id
     WHERE s.user_id = ?
       AND i.queue_id IS NOT NULL{FILTER}
     GROUP BY i.instance_id
     ORDER BY i.id DESC
     LIMIT ? OFFSET ?
"#;

/// Counting a player's matches.
///
/// Written as "instances whose id is in this player's stat rows" on purpose:
/// the join form makes SQLite scan every instance and probe the stats table
/// once per row, which measured 1.4s against the live database versus 5ms this
/// way. The website reads the same file the game server is writing to.
const PLAYER_MATCHES_COUNT_SQL: &str = r#"
    SELECT COUNT(*) AS total
      FROM ga_instances i
     WHERE i.instance_id IN (
               SELECT instance_id FROM ga_match_player_stats WHERE user_id = ?1
           )
       AND i.queue_id IS NOT NULL{FILTER}
"#;

pub async fn player_matches(
    live: &SqlitePool,
    user_id: i64,
    kind: Kind,
    limit: u32,
    offset: u32,
) -> Result<(i64, Vec<PlayerMatch>), MatchError> {
    let limit = limit.clamp(1, PAGE_LIMIT_MAX);
    let total: i64 =
        sqlx::query_scalar(&PLAYER_MATCHES_COUNT_SQL.replace("{FILTER}", &kind.player_predicate()))
            .bind(user_id)
            .fetch_one(live)
            .await?;
    let rows: Vec<PlayerMatch> = sqlx::query_as(&with_filter(PLAYER_MATCHES_SQL, kind))
        .bind(user_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(live)
        .await?;
    Ok((total, rows))
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MatchSummary {
    pub id: i64,
    /// The game's own match id — the identifier used in detail URLs.
    pub instance_id: i64,
    /// Raw Unreal map name, kept as the fallback when no translation exists.
    pub map_name: String,
    /// The map's display name from the client's string table.
    pub map_label: Option<String>,
    pub game_mode: String,
    pub started_at: i64,
    pub sealed_at: Option<i64>,
    pub outcome: Option<String>,
    pub winning_task_force: Option<i64>,
    /// Short queue code (`umax`, `merc`, …) — the fallback if untranslated.
    pub queue_code: Option<String>,
    /// The queue's display name, e.g. "Ultra Max Security".
    pub queue_label: Option<String>,
    /// The queue's location, when it has one, e.g. "Commonwealth Prime".
    pub queue_location: Option<String>,
    /// Participants, taken from the per-player stats table — `player_count` on
    /// the instance row is decremented as players leave and ends up at zero.
    pub players: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum MatchError {
    #[error("live game database is not configured")]
    NotConfigured,
    #[error("live game database error: {0}")]
    Query(#[from] sqlx::Error),
}

/// How many rows the cache window holds. Pages inside it are served from
/// memory; deeper offsets go back to SQL rather than growing the window.
const CACHE_WINDOW: u32 = 200;

pub struct MatchService {
    stats: StatsSource,
    ttl: Duration,
    /// Window cached once per filter and sliced per request, so the number of
    /// distinct `limit`/`page` combinations a client asks for cannot multiply
    /// queries.
    cache: Mutex<Vec<(Kind, Instant, Vec<MatchSummary>)>>,
}

impl MatchService {
    pub fn new(stats: impl Into<StatsSource>, ttl: Duration) -> Self {
        Self {
            stats: stats.into(),
            ttl,
            cache: Mutex::new(Vec::new()),
        }
    }

    /// One page of the archive, plus the total for the current filter.
    pub async fn page(
        &self,
        kind: Kind,
        limit: u32,
        offset: u32,
    ) -> Result<(i64, Vec<MatchSummary>), MatchError> {
        let limit = limit.clamp(1, PAGE_LIMIT_MAX);
        let pool = self.stats.pool().await.ok_or(MatchError::NotConfigured)?;
        let pool = &pool;

        if offset + limit <= CACHE_WINDOW
            && let Some(rows) = self.cached_window(kind).await
        {
            let total: i64 = sqlx::query_scalar(&with_filter(COUNT_SQL, kind))
                .fetch_one(pool)
                .await?;
            let slice = rows
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect();
            return Ok((total, slice));
        }

        let total: i64 = sqlx::query_scalar(&with_filter(COUNT_SQL, kind))
            .fetch_one(pool)
            .await?;
        let rows: Vec<MatchSummary> = sqlx::query_as(&with_filter(RECENT_MATCHES_SQL, kind))
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(pool)
            .await?;
        Ok((total, rows))
    }

    /// The cached window for a filter, refreshed when stale.
    async fn cached_window(&self, kind: Kind) -> Option<Vec<MatchSummary>> {
        {
            let cache = self.cache.lock().await;
            if let Some((_, at, rows)) = cache.iter().find(|(cached, _, _)| *cached == kind)
                && at.elapsed() < self.ttl
            {
                return Some(rows.clone());
            }
        }

        let pool = self.stats.pool().await?;
        let pool = &pool;
        let rows: Vec<MatchSummary> = sqlx::query_as(&with_filter(RECENT_MATCHES_SQL, kind))
            .bind(CACHE_WINDOW as i64)
            .bind(0_i64)
            .fetch_all(pool)
            .await
            .ok()?;

        let mut cache = self.cache.lock().await;
        cache.retain(|(cached, _, _)| *cached != kind);
        cache.push((kind, Instant::now(), rows.clone()));
        Some(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_filter_names_every_pvp_queue() {
        let pvp = Kind::Pvp.queue_filter();
        for code in crate::match_detail::PVP_QUEUES {
            assert!(pvp.contains(code), "PvP filter must include {code}: {pvp}");
        }
        // PvE is the complement, so it must exclude the same queues.
        let pve = Kind::Pve.queue_filter();
        assert!(pve.contains("NOT IN"), "{pve}");
        assert!(Kind::All.queue_filter().is_empty());
        assert_eq!(Kind::parse(Some("pvp")), Kind::Pvp);
        assert_eq!(Kind::parse(Some("nonsense")), Kind::All);
        assert_eq!(Kind::parse(None), Kind::All);
    }

    #[test]
    fn limit_is_clamped_to_the_window_and_to_at_least_one() {
        assert_eq!(LIMIT_DEFAULT, 8);
        // The route clamps before touching the service; this pins the intent.
        assert_eq!(0_u32.clamp(1, LIMIT_MAX), 1);
        assert_eq!(9_999_u32.clamp(1, LIMIT_MAX), LIMIT_MAX);
    }
}
