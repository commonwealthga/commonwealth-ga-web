//! One match, in full: the header, the scoreboard, and — for PvP — the rating
//! context the analyst views depend on.
//!
//! Two shapes come out of the same query, because the server decides which one
//! a match needs:
//!
//! * **PvE** (every queue except the PvP ones): who was there, what they did,
//!   which side they were on. The map and the mission are the interesting part.
//! * **PvP** (`merc`, `1v1`): the same rows plus pre-match MMR, a winner/loser
//!   split and the team balance that the rating engine actually saw — the
//!   numbers that decide whether a win meant anything.
//!
//! A player can have several rows in one match (class swap, team switch), so
//! rows are grouped per user: numeric stats are summed, the side and class are
//! whichever they spent the most time on, matching the previous site's
//! methodology so historical pages keep agreeing with it.

use serde::Serialize;
use sqlx::{Row, SqlitePool};

/// Queues that are player-versus-player. Everything else is PvE.
pub const PVP_QUEUES: [&str; 3] = ["merc", "1v1", "beta_pvp"];

/// `ga_characters.profile_id` -> class name (`src/GameServer/Constants/GameTypes.h`).
pub const CLASS_BY_PROFILE: [(i64, &str); 4] = [
    (680, "Assault"),
    (567, "Medic"),
    (681, "Recon"),
    (679, "Robotic"),
];

/// Stat columns summed when one player holds several rows in the same match.
const SUM_COLUMNS: [&str; 17] = [
    "rep_points",
    "kills",
    "assists",
    "deaths",
    "damage_dealt",
    "damage_taken",
    "healing",
    "buff_value",
    "defense",
    "obj_points",
    "bot_kills",
    "objective_captures",
    "beacon_spawns_provided",
    "beacon_spawns_used",
    "beacons_destroyed",
    "capture_seconds",
    "contest_seconds",
];

#[derive(Debug, Clone, Serialize)]
pub struct MatchDetail {
    pub instance_id: i64,
    /// `pvp` or `pve` — the frontend renders one of two layouts.
    pub kind: &'static str,
    pub map_name: String,
    pub map_label: Option<String>,
    pub queue_code: Option<String>,
    pub queue_label: Option<String>,
    pub queue_location: Option<String>,
    /// Queue description; the Double Agent queues differ only here.
    pub queue_description: Option<String>,
    pub game_mode: String,
    pub outcome: Option<String>,
    pub winning_task_force: Option<i64>,
    pub started_at: i64,
    pub sealed_at: Option<i64>,
    /// Seconds between start and seal, when the match was sealed.
    pub duration_seconds: Option<i64>,
    pub teams: Vec<TeamSummary>,
    pub participants: Vec<Participant>,
    /// Only for PvP with a recorded winner; `None` otherwise.
    pub balance: Option<Balance>,
    pub mmr_engine: Option<String>,
    /// Set when the request carried a session: who is looking, and how they did.
    pub viewer: Option<Viewer>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamSummary {
    pub task_force: i64,
    pub label: &'static str,
    pub players: i64,
    pub kills: i64,
    pub deaths: i64,
    pub damage_dealt: i64,
    pub healing: i64,
    pub obj_points: i64,
    pub bot_kills: i64,
    pub won: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Participant {
    pub user_id: i64,
    pub player: String,
    pub task_force: Option<i64>,
    /// Every class played, most-played first.
    pub classes: Vec<String>,
    /// Pre-match rating for a PvP match.
    pub mmr: Option<f64>,
    /// What this match did to that rating; `None` when the engine recorded none
    /// (a player who was rated by fallback only, for instance).
    pub mmr_delta: Option<f64>,
    /// The class that rating belongs to.
    pub mmr_class: Option<String>,
    pub won: bool,
    pub is_viewer: bool,
    pub rep_points: f64,
    pub kills: i64,
    pub assists: i64,
    pub deaths: i64,
    pub damage_dealt: i64,
    pub damage_taken: i64,
    pub healing: i64,
    pub buff_value: i64,
    pub defense: i64,
    pub obj_points: i64,
    pub bot_kills: i64,
    pub objective_captures: i64,
    pub beacon_spawns_provided: i64,
    pub beacon_spawns_used: i64,
    pub beacons_destroyed: i64,
    pub capture_seconds: f64,
    pub contest_seconds: f64,
    pub time_played_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Balance {
    pub wt_avg: f64,
    pub lt_avg: f64,
    pub delta: f64,
    /// Probability (%) the winning team was expected to win, from `delta`.
    pub wt_prob: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Viewer {
    pub user_id: i64,
    pub task_force: Option<i64>,
    pub won: bool,
    pub draw: bool,
}

pub fn class_name(profile_id: i64) -> String {
    CLASS_BY_PROFILE
        .iter()
        .find(|(id, _)| *id == profile_id)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| format!("#{profile_id}"))
}

pub fn is_pvp_queue(code: Option<&str>) -> bool {
    code.is_some_and(|code| PVP_QUEUES.contains(&code))
}

pub fn team_label(task_force: i64) -> &'static str {
    match task_force {
        1 => "Attackers",
        2 => "Defenders",
        _ => "Unassigned",
    }
}

/// Elo-style expectation: the chance the stronger average converts.
///
/// Expressed as a head-to-head between two averages; the shared module owns the
/// formula so the leaderboards and the match view agree.
pub fn win_probability(delta: f64) -> f64 {
    crate::mmr::win_probability(1000.0 + delta, 1000.0)
}

/// Load one match. `viewer` is the signed-in user id, when there is one.
pub async fn load(
    live: &SqlitePool,
    instance_id: i64,
    viewer: Option<i64>,
) -> Result<Option<MatchDetail>, sqlx::Error> {
    if instance_id <= 0 {
        // `instance_id` only identifies rows when it is non-zero.
        return Ok(None);
    }

    let header = sqlx::query(
        "SELECT i.instance_id, i.map_name, m.label AS map_label, i.game_mode,
                i.outcome, i.winning_task_force, i.started_at, i.sealed_at,
                q.code AS queue_code, q.label AS queue_label, q.location AS queue_location,
                q.description AS queue_description
           FROM ga_instances i
           LEFT JOIN (
               SELECT mg.map_name AS name, mt.message AS label
                 FROM map_game_info mg
                 LEFT JOIN asm_data_set_msg_translations mt ON mt.msg_id = mg.friendly_name_msg_id
           ) m ON m.name = i.map_name
           LEFT JOIN (
               SELECT g.queue_id AS id, g.name AS code, mt.message AS label,
                      mt2.message AS description, vv.text_msg_translated AS location
                 FROM ga_queues g
                 LEFT JOIN asm_data_set_msg_translations mt ON mt.msg_id = g.name_msg_id
                 LEFT JOIN asm_data_set_msg_translations mt2 ON mt2.msg_id = g.desc_msg_id
                 LEFT JOIN asm_data_set_valid_values vv ON vv.value_id = g.location_value_id
           ) q ON q.id = i.queue_id
          WHERE i.instance_id = ?
            -- Same rule as the archive: no queue, no match.
            AND i.queue_id IS NOT NULL
          LIMIT 1",
    )
    .bind(instance_id)
    .fetch_optional(live)
    .await?;

    let Some(header) = header else {
        return Ok(None);
    };

    let mut participants = load_participants(live, instance_id).await?;
    let winning = header
        .try_get::<Option<i64>, _>("winning_task_force")
        .ok()
        .flatten();
    let outcome: Option<String> = header.try_get("outcome").ok().flatten();
    let draw = outcome.as_deref() == Some("STALEMATE") || winning.is_none();

    for participant in &mut participants {
        participant.won = winning.is_some() && participant.task_force == winning;
        participant.is_viewer = viewer.is_some_and(|id| id == participant.user_id);
    }

    let queue_code: Option<String> = header.try_get("queue_code").ok().flatten();
    let kind = if is_pvp_queue(queue_code.as_deref()) {
        "pvp"
    } else {
        "pve"
    };

    let mut mmr_engine = None;
    if kind == "pvp" {
        let players: Vec<(i64, Option<String>)> = participants
            .iter()
            .map(|p| (p.user_id, p.classes.first().cloned()))
            .collect();
        let (engine, ratings) = crate::mmr::pre_match(
            live,
            instance_id,
            &players,
            header.try_get("started_at").unwrap_or(0),
        )
        .await;
        mmr_engine = Some(engine.code().to_string());
        for participant in &mut participants {
            if let Some((before, after, class)) = ratings.get(&participant.user_id) {
                participant.mmr = Some(*before);
                participant.mmr_delta = after.map(|after| crate::mmr::round1(after - before));
                participant.mmr_class = Some(class.clone());
            }
        }
    }

    let teams = summarize_teams(&participants, winning);
    let balance = if kind == "pvp" {
        balance_for(&participants, winning)
    } else {
        None
    };

    let started_at: i64 = header.try_get("started_at").unwrap_or(0);
    let sealed_at: Option<i64> = header.try_get("sealed_at").ok().flatten();

    let viewer = viewer.and_then(|id| {
        participants
            .iter()
            .find(|p| p.user_id == id)
            .map(|p| Viewer {
                user_id: id,
                task_force: p.task_force,
                won: p.won,
                draw,
            })
    });

    Ok(Some(MatchDetail {
        instance_id,
        kind,
        map_name: header.try_get("map_name").unwrap_or_default(),
        map_label: header.try_get("map_label").ok().flatten(),
        queue_code,
        queue_label: header.try_get("queue_label").ok().flatten(),
        queue_location: header.try_get("queue_location").ok().flatten(),
        queue_description: header.try_get("queue_description").ok().flatten(),
        game_mode: header.try_get("game_mode").unwrap_or_default(),
        outcome,
        winning_task_force: winning,
        started_at,
        sealed_at,
        duration_seconds: sealed_at.map(|end| (end - started_at).max(0)),
        teams,
        participants,
        balance,
        mmr_engine,
        viewer,
    }))
}

/// Read a numeric column whatever the game server stored it as.
///
/// The stat columns are a mix of INTEGER and REAL, and sqlx decodes strictly:
/// asking an INTEGER column for an f64 fails, and swallowing that error would
/// silently report every kill as zero.
fn numeric(row: &sqlx::sqlite::SqliteRow, column: &str) -> f64 {
    if let Ok(Some(value)) = row.try_get::<Option<f64>, _>(column) {
        return value;
    }
    if let Ok(Some(value)) = row.try_get::<Option<i64>, _>(column) {
        return value as f64;
    }
    0.0
}

async fn load_participants(
    live: &SqlitePool,
    instance_id: i64,
) -> Result<Vec<Participant>, sqlx::Error> {
    let sums = SUM_COLUMNS
        .iter()
        .map(|column| format!("s.{column} AS {column}"))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT s.user_id, s.task_force, s.time_played_seconds,
                COALESCE(u.username, 'Player ' || s.user_id) AS player,
                c.profile_id, {sums}
           FROM ga_match_player_stats s
           LEFT JOIN ga_users u ON u.id = s.user_id
           LEFT JOIN ga_characters c ON c.id = s.character_id
          WHERE s.instance_id = ?"
    );

    let rows = sqlx::query(&sql).bind(instance_id).fetch_all(live).await?;

    // Group by user: sums add up, side and class follow the most time played.
    struct Agg {
        player: String,
        side_time: std::collections::HashMap<i64, f64>,
        class_time: std::collections::HashMap<String, f64>,
        stats: std::collections::HashMap<&'static str, f64>,
        time: f64,
    }

    let mut grouped: std::collections::BTreeMap<i64, Agg> = std::collections::BTreeMap::new();

    for row in rows {
        let user_id: i64 = row.try_get("user_id").unwrap_or(0);
        let entry = grouped.entry(user_id).or_insert_with(|| Agg {
            player: row.try_get("player").unwrap_or_default(),
            side_time: std::collections::HashMap::new(),
            class_time: std::collections::HashMap::new(),
            stats: SUM_COLUMNS.iter().map(|column| (*column, 0.0)).collect(),
            time: 0.0,
        });

        for column in SUM_COLUMNS {
            *entry.stats.entry(column).or_insert(0.0) += numeric(&row, column);
        }

        let played = numeric(&row, "time_played_seconds");
        entry.time += played;

        if let Ok(Some(side)) = row.try_get::<Option<i64>, _>("task_force") {
            *entry.side_time.entry(side).or_insert(0.0) += played;
        }
        if let Ok(Some(profile_id)) = row.try_get::<Option<i64>, _>("profile_id") {
            *entry
                .class_time
                .entry(class_name(profile_id))
                .or_insert(0.0) += played;
        }
    }

    let mut participants: Vec<Participant> = grouped
        .into_iter()
        .map(|(user_id, agg)| {
            let task_force = agg
                .side_time
                .iter()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(side, _)| *side);

            let mut classes: Vec<(String, f64)> = agg.class_time.into_iter().collect();
            classes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let stat = |name: &str| agg.stats.get(name).copied().unwrap_or(0.0);
            Participant {
                user_id,
                player: agg.player,
                task_force,
                classes: classes.into_iter().map(|(name, _)| name).collect(),
                mmr: None,
                mmr_delta: None,
                mmr_class: None,
                won: false,
                is_viewer: false,
                rep_points: stat("rep_points"),
                kills: stat("kills") as i64,
                assists: stat("assists") as i64,
                deaths: stat("deaths") as i64,
                damage_dealt: stat("damage_dealt") as i64,
                damage_taken: stat("damage_taken") as i64,
                healing: stat("healing") as i64,
                buff_value: stat("buff_value") as i64,
                defense: stat("defense") as i64,
                obj_points: stat("obj_points") as i64,
                bot_kills: stat("bot_kills") as i64,
                objective_captures: stat("objective_captures") as i64,
                beacon_spawns_provided: stat("beacon_spawns_provided") as i64,
                beacon_spawns_used: stat("beacon_spawns_used") as i64,
                beacons_destroyed: stat("beacons_destroyed") as i64,
                capture_seconds: stat("capture_seconds"),
                contest_seconds: stat("contest_seconds"),
                time_played_seconds: agg.time,
            }
        })
        .collect();

    // Attackers first, then defenders, then unknown sides; within a team, the
    // biggest damage contribution first.
    participants.sort_by(|a, b| {
        let rank = |side: Option<i64>| match side {
            Some(1) => 0,
            Some(2) => 1,
            _ => 2,
        };
        rank(a.task_force)
            .cmp(&rank(b.task_force))
            .then(b.damage_dealt.cmp(&a.damage_dealt))
            .then(a.player.to_lowercase().cmp(&b.player.to_lowercase()))
    });

    Ok(participants)
}

fn summarize_teams(participants: &[Participant], winning: Option<i64>) -> Vec<TeamSummary> {
    let mut teams: Vec<TeamSummary> = Vec::new();
    for side in [1_i64, 2] {
        let members: Vec<&Participant> = participants
            .iter()
            .filter(|p| p.task_force == Some(side))
            .collect();
        if members.is_empty() {
            continue;
        }
        teams.push(TeamSummary {
            task_force: side,
            label: team_label(side),
            players: members.len() as i64,
            kills: members.iter().map(|p| p.kills).sum(),
            deaths: members.iter().map(|p| p.deaths).sum(),
            damage_dealt: members.iter().map(|p| p.damage_dealt).sum(),
            healing: members.iter().map(|p| p.healing).sum(),
            obj_points: members.iter().map(|p| p.obj_points).sum(),
            bot_kills: members.iter().map(|p| p.bot_kills).sum(),
            won: winning == Some(side),
        });
    }
    teams
}

fn balance_for(participants: &[Participant], winning: Option<i64>) -> Option<Balance> {
    let winner = winning?;
    let rated: Vec<&Participant> = participants
        .iter()
        .filter(|p| p.mmr.is_some() && p.task_force.is_some())
        .collect();
    if rated.is_empty() {
        return None;
    }

    let average = |team: bool| -> Option<f64> {
        let values: Vec<f64> = rated
            .iter()
            .filter(|p| (p.task_force == Some(winner)) == team)
            .filter_map(|p| p.mmr)
            .collect();
        if values.is_empty() {
            return None;
        }
        Some(crate::mmr::round1(
            values.iter().sum::<f64>() / values.len() as f64,
        ))
    };

    let wt_avg = average(true)?;
    let lt_avg = average(false)?;
    let delta = crate::mmr::round1(wt_avg - lt_avg);

    Some(Balance {
        wt_avg,
        lt_avg,
        delta,
        wt_prob: win_probability(delta),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_pvp_queues_get_the_pvp_layout() {
        assert!(is_pvp_queue(Some("merc")));
        assert!(is_pvp_queue(Some("1v1")));
        assert!(is_pvp_queue(Some("beta_pvp")));
        assert!(!is_pvp_queue(Some("umax")));
        assert!(!is_pvp_queue(Some("desert_raids")));
        assert!(!is_pvp_queue(None));
    }

    #[test]
    fn classes_come_from_the_profile_ids() {
        assert_eq!(class_name(680), "Assault");
        assert_eq!(class_name(567), "Medic");
        assert_eq!(class_name(681), "Recon");
        assert_eq!(class_name(679), "Robotic");
        assert_eq!(class_name(1234), "#1234");
    }

    #[test]
    fn an_even_match_is_a_coin_flip() {
        assert_eq!(win_probability(0.0), 50.0);
        assert!(win_probability(100.0) > 60.0);
        assert!(win_probability(-100.0) < 40.0);
    }
}
