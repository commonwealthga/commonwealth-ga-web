//! Public agent profiles.
//!
//! The same data behind `/me`, minus the session: a directory of everyone with
//! recorded activity, one agent's career and rating history, and the class
//! leaderboards. All of it reads the read-only game database; nothing here
//! touches `site.db`.
//!
//! Helpers stay top-level and small, matching the rest of the crate, and every
//! one of them degrades to empty rather than erroring when the game database
//! predates a table.

use serde::Serialize;
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::mmr;

/// Rows per directory page.
pub const PAGE_LIMIT_DEFAULT: u32 = 24;
pub const PAGE_LIMIT_MAX: u32 = 60;

/// One line in the directory.
#[derive(Debug, Clone, Serialize)]
pub struct AgentRow {
    pub user_id: i64,
    pub username: String,
    /// Completed matches this account appears in.
    pub matches: i64,
    /// Unix seconds of the newest match; `None` for accounts with no archive.
    pub last_active: Option<i64>,
    /// Total damage dealt. Used instead of kills because a PvE-heavy player's
    /// kills sit in `bot_kills`, which would make the column read as zero.
    pub damage_dealt: i64,
}

/// A rating plus how it compares to everyone else in that class.
#[derive(Debug, Clone, Serialize)]
pub struct RankedRating {
    pub class_name: String,
    pub mmr: f64,
    pub games: i64,
    pub played_at: i64,
    /// 1-based position on the class leaderboard.
    pub rank: i64,
    /// How many players are rated in this class.
    pub ranked_players: i64,
}

/// One player's line in one match, already grouped across the rows their
/// class/side swaps left behind.
///
/// Everything on a profile is derived from this list: totals per mode, and the
/// best single match for each statistic. That way the number in the header and
/// the match it links to cannot disagree.
#[derive(Debug, Clone)]
pub struct MatchLine {
    pub instance_id: i64,
    pub map_label: Option<String>,
    pub map_name: String,
    pub queue_code: Option<String>,
    pub started_at: i64,
    pub pvp: bool,
    /// The player's side carried it. Draws and abandoned matches count as
    /// "not a win" rather than being excluded, so the rate is honest.
    pub won: bool,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub damage_dealt: i64,
    pub damage_taken: i64,
    pub healing: i64,
    pub obj_points: i64,
    pub bot_kills: i64,
    pub beacons_destroyed: i64,
    /// Fractional: the game stores real seconds, and rounding per match would
    /// drift across a career. Rounded once in `totals_for`.
    pub seconds_played: f64,
}

/// A player's best single match at one statistic.
#[derive(Debug, Clone, Serialize)]
pub struct BestStat {
    /// Machine name of the statistic, matching `ModeTotals` fields.
    pub stat: String,
    pub value: i64,
    pub instance_id: i64,
    pub map_label: Option<String>,
    pub map_name: String,
    pub queue_code: Option<String>,
    pub started_at: i64,
}

/// Totals for one kind of play.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ModeTotals {
    pub matches: i64,
    pub wins: i64,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub damage_dealt: i64,
    pub damage_taken: i64,
    pub healing: i64,
    pub obj_points: i64,
    pub seconds_played: i64,
    pub bot_kills: i64,
    pub beacons_destroyed: i64,
}

/// The same career, split by what was actually being played.
///
/// Blended totals were misleading: a player who mostly runs PvE missions
/// collects deaths and bot kills that say nothing about how they do against
/// other people.
#[derive(Debug, Clone, Serialize)]
pub struct CareerSplit {
    pub all: ModeTotals,
    pub pvp: ModeTotals,
    pub pve: ModeTotals,
}

/// One agent, in public.
#[derive(Debug, Clone, Serialize)]
pub struct BestSplit {
    pub all: Vec<BestStat>,
    pub pvp: Vec<BestStat>,
    pub pve: Vec<BestStat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentProfile {
    pub user_id: i64,
    pub username: String,
    pub first_seen: Option<i64>,
    pub last_active: Option<i64>,
    pub totals: CareerSplit,
    /// Best single match per statistic, per mode.
    pub bests: BestSplit,
    pub ratings: Vec<RankedRating>,
}

/// The directory: accounts with at least one recorded match, most recently
/// active first.
///
/// Accounts with no matches are deliberately absent — there is nothing to show
/// on them — but `/api/agents/{id}` still answers for them, so a search result
/// or an old link does not turn into a 404.
pub async fn directory(
    live: &SqlitePool,
    search: Option<&str>,
    limit: u32,
    offset: u32,
) -> Result<(i64, Vec<AgentRow>), sqlx::Error> {
    let term = search.map(str::trim).filter(|s| !s.is_empty());
    let pattern = term.map(|s| format!("%{s}%"));

    // `?1 IS NULL` keeps one statement for both cases; SQLite binds the same
    // parameter twice, so the filter disappears when no search is given.
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT s.user_id) AS total
           FROM ga_match_player_stats s
           JOIN ga_users u ON u.id = s.user_id
           JOIN ga_instances i ON i.instance_id = s.instance_id
          WHERE i.queue_id IS NOT NULL
            AND (?1 IS NULL OR u.username LIKE ?1)",
    )
    .bind(pattern.as_deref())
    .fetch_one(live)
    .await?;

    let rows = sqlx::query(
        "SELECT u.id AS user_id,
                u.username AS username,
                COUNT(DISTINCT CASE WHEN i.queue_id IS NOT NULL THEN s.instance_id END) AS matches,
                MAX(CASE WHEN i.queue_id IS NOT NULL THEN i.started_at END) AS last_active,
                COALESCE(SUM(s.damage_dealt), 0) AS damage_dealt
           FROM ga_match_player_stats s
           JOIN ga_users u ON u.id = s.user_id
           LEFT JOIN ga_instances i ON i.instance_id = s.instance_id
          WHERE (?1 IS NULL OR u.username LIKE ?1)
          GROUP BY u.id
          ORDER BY last_active DESC, u.username ASC
          LIMIT ?2 OFFSET ?3",
    )
    .bind(pattern.as_deref())
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(live)
    .await?;

    let agents = rows
        .iter()
        .map(|row| AgentRow {
            user_id: row.try_get::<i64, _>("user_id").unwrap_or(0),
            username: row
                .try_get::<Option<String>, _>("username")
                .ok()
                .flatten()
                .unwrap_or_else(|| "Unknown".to_string()),
            matches: row
                .try_get::<Option<i64>, _>("matches")
                .ok()
                .flatten()
                .unwrap_or(0),
            last_active: row.try_get::<Option<i64>, _>("last_active").ok().flatten(),
            damage_dealt: row
                .try_get::<Option<i64>, _>("damage_dealt")
                .ok()
                .flatten()
                .unwrap_or(0),
        })
        .collect();

    Ok((total, agents))
}

/// Everything public about one agent. `None` when there is no such account.
pub async fn profile(live: &SqlitePool, user_id: i64) -> Result<Option<AgentProfile>, sqlx::Error> {
    let Some(username) =
        sqlx::query_scalar::<_, Option<String>>("SELECT username FROM ga_users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(live)
            .await?
            .flatten()
    else {
        return Ok(None);
    };

    let lines = match_lines(live, user_id).await;

    // Same shape as the count above: probe instances through the unique
    // instance_id index instead of joining from the stats side.
    let span = sqlx::query(
        "SELECT MIN(i.started_at) AS first_seen, MAX(i.started_at) AS last_active
           FROM ga_instances i
          WHERE i.instance_id IN (
                    SELECT instance_id FROM ga_match_player_stats WHERE user_id = ?
                )
            AND i.queue_id IS NOT NULL",
    )
    .bind(user_id)
    .fetch_one(live)
    .await?;

    // Ratings with their leaderboard position: the number that makes a profile
    // interesting ("#1 Medic") rather than just a value.
    let ratings = mmr::latest_for_user(live, user_id).await;
    let mut ranked = Vec::with_capacity(ratings.len());
    for rating in ratings {
        let board = mmr::standings(live, &rating.class_name, 10_000, 0).await;
        let Some(rank) = board.iter().position(|row| row.user_id == user_id) else {
            continue;
        };
        ranked.push(RankedRating {
            class_name: rating.class_name,
            mmr: mmr::round1(rating.mmr),
            games: rating.games,
            played_at: rating.played_at,
            rank: rank as i64 + 1,
            ranked_players: board.len() as i64,
        });
    }

    let pvp: Vec<&MatchLine> = lines.iter().filter(|line| line.pvp).collect();
    let pve: Vec<&MatchLine> = lines.iter().filter(|line| !line.pvp).collect();
    let all: Vec<&MatchLine> = lines.iter().collect();

    Ok(Some(AgentProfile {
        user_id,
        username,
        first_seen: span.try_get::<Option<i64>, _>("first_seen").ok().flatten(),
        last_active: span.try_get::<Option<i64>, _>("last_active").ok().flatten(),
        totals: CareerSplit {
            all: totals_for(&all),
            pvp: totals_for(&pvp),
            pve: totals_for(&pve),
        },
        bests: BestSplit {
            all: bests_for(&all),
            pvp: bests_for(&pvp),
            pve: bests_for(&pve),
        },
        ratings: ranked,
    }))
}

/// Every match this player appears in, one row each.
///
/// Grouped here rather than in SQL because a player who swapped class or side
/// has several stat rows per match, and summing them is the same rule the match
/// detail uses.
async fn match_lines(live: &SqlitePool, user_id: i64) -> Vec<MatchLine> {
    let rows = sqlx::query(
        "SELECT i.instance_id AS instance_id,
                i.map_name AS map_name,
                m.label AS map_label,
                q.code AS queue_code,
                i.started_at AS started_at,
                MAX(CASE WHEN i.winning_task_force IS NOT NULL
                          AND s.task_force = i.winning_task_force
                         THEN 1 ELSE 0 END) AS won,
                COALESCE(SUM(s.kills), 0) AS kills,
                COALESCE(SUM(s.deaths), 0) AS deaths,
                COALESCE(SUM(s.assists), 0) AS assists,
                COALESCE(SUM(s.damage_dealt), 0) AS damage_dealt,
                COALESCE(SUM(s.damage_taken), 0) AS damage_taken,
                COALESCE(SUM(s.healing), 0) AS healing,
                COALESCE(SUM(s.obj_points), 0) AS obj_points,
                COALESCE(SUM(s.bot_kills), 0) AS bot_kills,
                COALESCE(SUM(s.beacons_destroyed), 0) AS beacons_destroyed,
                COALESCE(SUM(s.time_played_seconds), 0) AS seconds_played
           FROM ga_match_player_stats s
           JOIN ga_instances i ON i.instance_id = s.instance_id
           LEFT JOIN (
               -- One row per map name: map_game_info can carry the same map
               -- twice (different game_class rows), and a fan-out here would
               -- multiply every statistic summed through it.
               SELECT mg.map_name AS name, MAX(mt.message) AS label
                 FROM map_game_info mg
                 LEFT JOIN asm_data_set_msg_translations mt ON mt.msg_id = mg.friendly_name_msg_id
                GROUP BY mg.map_name
           ) m ON m.name = i.map_name
           LEFT JOIN (
               SELECT g.queue_id AS id, g.name AS code
                 FROM ga_queues g
           ) q ON q.id = i.queue_id
          WHERE s.user_id = ?
            AND i.queue_id IS NOT NULL
          GROUP BY i.instance_id
          ORDER BY i.started_at DESC",
    )
    .bind(user_id)
    .fetch_all(live)
    .await;

    let Ok(rows) = rows else {
        return Vec::new();
    };

    rows.iter()
        .map(|row| {
            let queue_code: Option<String> = row.try_get("queue_code").ok().flatten();
            MatchLine {
                instance_id: row.try_get("instance_id").unwrap_or(0),
                map_name: row.try_get("map_name").unwrap_or_default(),
                map_label: row.try_get("map_label").ok().flatten(),
                pvp: crate::match_detail::is_pvp_queue(queue_code.as_deref()),
                queue_code,
                started_at: row.try_get("started_at").unwrap_or(0),
                won: row
                    .try_get::<Option<i64>, _>("won")
                    .ok()
                    .flatten()
                    .unwrap_or(0)
                    == 1,
                kills: row.try_get("kills").unwrap_or(0),
                deaths: row.try_get("deaths").unwrap_or(0),
                assists: row.try_get("assists").unwrap_or(0),
                damage_dealt: row.try_get("damage_dealt").unwrap_or(0),
                damage_taken: row.try_get("damage_taken").unwrap_or(0),
                healing: row.try_get("healing").unwrap_or(0),
                obj_points: row.try_get("obj_points").unwrap_or(0),
                bot_kills: row.try_get("bot_kills").unwrap_or(0),
                beacons_destroyed: row.try_get("beacons_destroyed").unwrap_or(0),
                seconds_played: row.try_get("seconds_played").unwrap_or(0.0),
            }
        })
        .collect()
}

fn totals_for(lines: &[&MatchLine]) -> ModeTotals {
    let mut totals = ModeTotals {
        matches: lines.len() as i64,
        ..ModeTotals::default()
    };
    let mut seconds = 0.0_f64;
    for line in lines {
        if line.won {
            totals.wins += 1;
        }
        totals.kills += line.kills;
        totals.deaths += line.deaths;
        totals.assists += line.assists;
        totals.damage_dealt += line.damage_dealt;
        totals.damage_taken += line.damage_taken;
        totals.healing += line.healing;
        totals.obj_points += line.obj_points;
        seconds += line.seconds_played;
        totals.bot_kills += line.bot_kills;
        totals.beacons_destroyed += line.beacons_destroyed;
    }
    totals.seconds_played = seconds.round() as i64;
    totals
}

/// The best single match for each statistic, so a profile can say "43 kills,
/// and here it is" instead of only a lifetime sum.
fn bests_for(lines: &[&MatchLine]) -> Vec<BestStat> {
    /// A statistic's name and how to read it off a match line.
    type StatReader = (&'static str, fn(&MatchLine) -> i64);

    const STATS: [StatReader; 8] = [
        ("kills", |line| line.kills),
        ("assists", |line| line.assists),
        ("damage_dealt", |line| line.damage_dealt),
        ("damage_taken", |line| line.damage_taken),
        ("healing", |line| line.healing),
        ("obj_points", |line| line.obj_points),
        ("bot_kills", |line| line.bot_kills),
        ("beacons_destroyed", |line| line.beacons_destroyed),
    ];

    STATS
        .iter()
        .filter_map(|(stat, value_of)| {
            let best = lines.iter().copied().max_by_key(|line| value_of(line))?;
            let value = value_of(best);
            // A zero is not a personal best, it is an absence.
            (value > 0).then(|| BestStat {
                stat: (*stat).to_string(),
                value,
                instance_id: best.instance_id,
                map_label: best.map_label.clone(),
                map_name: best.map_name.clone(),
                queue_code: best.queue_code.clone(),
                started_at: best.started_at,
            })
        })
        .collect()
}

/// One line of a class leaderboard.
#[derive(Debug, Clone, Serialize)]
pub struct BoardEntry {
    pub rank: i64,
    pub user_id: i64,
    pub username: String,
    pub mmr: f64,
    pub played_at: i64,
}

/// A class leaderboard page.
#[derive(Debug, Clone, Serialize)]
pub struct Leaderboard {
    pub class_name: String,
    pub total: i64,
    pub entries: Vec<BoardEntry>,
}

pub async fn leaderboard(
    live: &SqlitePool,
    class_name: &str,
    limit: u32,
    offset: u32,
) -> Leaderboard {
    let total = mmr::count_rated(live, class_name).await;
    let rows = mmr::standings(live, class_name, limit as i64, offset as i64).await;

    Leaderboard {
        class_name: class_name.to_string(),
        total,
        entries: rows
            .into_iter()
            .enumerate()
            .map(|(index, standing)| BoardEntry {
                rank: offset as i64 + index as i64 + 1,
                user_id: standing.user_id,
                username: standing.username,
                mmr: mmr::round1(standing.mmr),
                played_at: standing.played_at,
            })
            .collect(),
    }
}

/// A player's rating over time, as parallel arrays for the chart.
#[derive(Debug, Clone, Serialize)]
pub struct RatingSeries {
    pub class_name: Option<String>,
    pub engine: String,
    pub times: Vec<i64>,
    pub mmr: Vec<f64>,
    /// Match ids alongside the points, so a point can link to its match.
    pub instances: Vec<i64>,
}

pub async fn rating_series(
    live: &SqlitePool,
    user_id: i64,
    class_name: Option<&str>,
    days: u32,
) -> RatingSeries {
    let engine = mmr::engine(live).await;
    let mut points = mmr::series_for_user(live, user_id, class_name).await;

    if days > 0
        && let Some(newest) = points.iter().map(|point| point.played_at).max()
    {
        let cutoff = newest - (days as i64) * 86_400;
        points.retain(|point| point.played_at >= cutoff);
    }

    RatingSeries {
        class_name: class_name.map(str::to_string),
        engine: engine.code().to_string(),
        times: points.iter().map(|point| point.played_at).collect(),
        mmr: points.iter().map(|point| mmr::round1(point.mmr)).collect(),
        instances: points.iter().map(|point| point.instance_id).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_silly_page_size_is_clamped() {
        assert_eq!(0_u32.clamp(1, PAGE_LIMIT_MAX), 1);
        assert_eq!(9_999_u32.clamp(1, PAGE_LIMIT_MAX), PAGE_LIMIT_MAX);
        assert_eq!(PAGE_LIMIT_DEFAULT, 24);
    }
}
