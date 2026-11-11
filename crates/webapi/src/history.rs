//! Concurrent-player history: hourly peaks, for the "when is everyone online"
//! chart.
//!
//! Ported from the previous Python site, including its two subtleties, because
//! both change the shape of the curve:
//!
//! 1. A session with no `logout_at` (crash, or never closed) is assumed to last
//!    30 minutes — but if the player kept generating match events past that, the
//!    presence is extended to their last such event, because they demonstrably
//!    were still online.
//! 2. Concurrency is piecewise-constant between session boundaries, so an hour
//!    with no boundaries in it still records the peak that held across it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

/// How long an unclosed session is assumed to last on its own.
pub const UNKNOWN_LOGOUT_SECS: i64 = 30 * 60;
pub const HOUR_SECS: i64 = 3600;

pub const DAYS_DEFAULT: u32 = 30;
pub const DAYS_MAX: u32 = 365;

#[derive(Debug, Clone, Default, Serialize)]
pub struct OnlineHistory {
    /// Hour-aligned unix timestamps, oldest first.
    pub times: Vec<i64>,
    /// Peak concurrent players in each hour — same length as `times`.
    pub players: Vec<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("live game database is not configured")]
    NotConfigured,
    #[error("live game database error: {0}")]
    Query(#[from] sqlx::Error),
}

pub struct HistoryService {
    pool: Option<SqlitePool>,
    ttl: Duration,
    cache: Mutex<Option<(Instant, Arc<OnlineHistory>)>>,
}

impl HistoryService {
    pub fn new(pool: Option<SqlitePool>, ttl: Duration) -> Self {
        Self {
            pool,
            ttl,
            cache: Mutex::new(None),
        }
    }

    /// Hourly peaks over the last `days` days (the whole series is computed and
    /// cached once; the window is applied on the way out).
    pub async fn series(&self, days: u32) -> Result<OnlineHistory, HistoryError> {
        let full = self.cached().await?;
        if days == 0 {
            return Ok((*full).clone());
        }

        let days = days.clamp(1, DAYS_MAX);
        let cutoff = now_unix() - i64::from(days) * 24 * HOUR_SECS;

        let start = full.times.partition_point(|t| *t < cutoff);
        Ok(OnlineHistory {
            times: full.times[start..].to_vec(),
            players: full.players[start..].to_vec(),
        })
    }

    async fn cached(&self) -> Result<Arc<OnlineHistory>, HistoryError> {
        if let Some((at, series)) = self.cache.lock().await.as_ref()
            && at.elapsed() < self.ttl
        {
            return Ok(Arc::clone(series));
        }

        let pool = self.pool.as_ref().ok_or(HistoryError::NotConfigured)?;
        let series = Arc::new(self.compute_from_db(pool).await?);
        *self.cache.lock().await = Some((Instant::now(), Arc::clone(&series)));
        Ok(series)
    }

    async fn compute_from_db(&self, pool: &SqlitePool) -> Result<OnlineHistory, HistoryError> {
        let rows: Vec<SessionRow> = sqlx::query_as(
            "SELECT user_id, login_at, logout_at FROM ga_user_sessions
              WHERE outcome = 'ok' AND login_at IS NOT NULL AND user_id IS NOT NULL",
        )
        .fetch_all(pool)
        .await?;

        if rows.is_empty() {
            return Ok(OnlineHistory::default());
        }

        // Only sessions that never closed need an activity lookup, and there are
        // normally very few of them.
        let open_users: Vec<i64> = {
            let mut ids: Vec<i64> = rows
                .iter()
                .filter(|r| r.logout_at.is_none())
                .map(|r| r.user_id)
                .collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };
        let activity = self.activity_for(pool, &open_users).await?;

        Ok(compute(
            &rows
                .iter()
                .map(|r| (r.user_id, r.login_at, r.logout_at))
                .collect::<Vec<_>>(),
            &activity,
        ))
    }

    /// Last in-match activity per user, used to bound sessions whose logout was
    /// never recorded. Missing table (minimal test DB) means no signal at all.
    async fn activity_for(
        &self,
        pool: &SqlitePool,
        user_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<i64>>, HistoryError> {
        let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
        if user_ids.is_empty() {
            return Ok(out);
        }

        let mut builder = sqlx::QueryBuilder::new(
            "SELECT actor_user_id AS uid, ts FROM ga_match_events WHERE actor_user_id IN (",
        );
        let mut separated = builder.separated(", ");
        for id in user_ids {
            separated.push_bind(*id);
        }
        separated.push_unseparated(
            ") UNION ALL SELECT target_user_id AS uid, ts FROM ga_match_events WHERE target_user_id IN (",
        );
        let mut separated = builder.separated(", ");
        for id in user_ids {
            separated.push_bind(*id);
        }
        separated.push_unseparated(")");

        let rows: Result<Vec<ActivityRow>, sqlx::Error> =
            builder.build_query_as().fetch_all(pool).await;
        let rows = match rows {
            Ok(rows) => rows,
            // Old or minimal databases may not have the events table; a missing
            // signal is not an error, it just means we fall back to the default.
            Err(_) => return Ok(out),
        };

        for row in rows {
            if let Some(uid) = row.uid {
                out.entry(uid).or_default().push(row.ts);
            }
        }
        for times in out.values_mut() {
            times.sort_unstable();
        }
        Ok(out)
    }
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    user_id: i64,
    login_at: i64,
    logout_at: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct ActivityRow {
    uid: Option<i64>,
    ts: i64,
}

/// Pure core: sessions + activity in, hourly peaks out.
///
/// `sessions` is `(user_id, login_at, logout_at)`.
pub fn compute(
    sessions: &[(i64, i64, Option<i64>)],
    activity: &HashMap<i64, Vec<i64>>,
) -> OnlineHistory {
    // Group by user so an unclosed session can be bounded by that user's *next*
    // login: a stale session must not bleed across a clean reconnect.
    let mut by_user: HashMap<i64, Vec<(i64, Option<i64>)>> = HashMap::new();
    for (user_id, login_at, logout_at) in sessions {
        by_user
            .entry(*user_id)
            .or_default()
            .push((*login_at, *logout_at));
    }

    let mut intervals: Vec<(i64, i64)> = Vec::new();
    for (user_id, mut user_sessions) in by_user {
        user_sessions.sort_by_key(|(start, _)| *start);
        let user_activity = activity.get(&user_id).map(Vec::as_slice).unwrap_or(&[]);

        let mut segments: Vec<(i64, i64)> = Vec::with_capacity(user_sessions.len());
        for i in 0..user_sessions.len() {
            let (login_at, logout_at) = user_sessions[i];
            let end = match logout_at {
                Some(logout) => logout,
                None => {
                    let mut end = login_at + UNKNOWN_LOGOUT_SECS;
                    let next_login = user_sessions.get(i + 1).map(|(start, _)| *start);
                    let last_activity = user_activity
                        .iter()
                        .copied()
                        .filter(|t| *t >= login_at && next_login.is_none_or(|next| *t < next))
                        .max();
                    if let Some(last) = last_activity
                        && last > end
                    {
                        end = last;
                    }
                    end
                }
            };
            segments.push((login_at, end.max(login_at)));
        }

        // Merge overlapping segments for the same user: one player counts once.
        segments.sort_unstable();
        let Some(&(first_start, first_end)) = segments.first() else {
            continue;
        };
        let (mut current_start, mut current_end) = (first_start, first_end);
        for (start, end) in segments.into_iter().skip(1) {
            if start <= current_end {
                current_end = current_end.max(end);
            } else {
                intervals.push((current_start, current_end));
                current_start = start;
                current_end = end;
            }
        }
        intervals.push((current_start, current_end));
    }

    if intervals.is_empty() {
        return OnlineHistory::default();
    }

    // Sweep line: +1 at presence start, -1 at end. At equal timestamps ends
    // apply before starts, so two players swapping in the same second register
    // as one continuous presence rather than a momentary two.
    let mut events: Vec<(i64, i64)> = Vec::with_capacity(intervals.len() * 2);
    for (start, end) in intervals {
        events.push((start, 1));
        events.push((end, -1));
    }
    events.sort_unstable();

    let mut hour_peak: HashMap<i64, i64> = HashMap::new();
    let mut current = 0_i64;
    let mut previous_ts = events[0].0;
    for (ts, delta) in events {
        if current > 0 && ts > previous_ts {
            // The level held across [previous_ts, ts): every hour it touches.
            let mut hour = (previous_ts / HOUR_SECS) * HOUR_SECS;
            while hour < ts {
                let slot = hour_peak.entry(hour).or_insert(0);
                if current > *slot {
                    *slot = current;
                }
                hour += HOUR_SECS;
            }
        }
        current += delta;
        previous_ts = ts;

        let bucket = (ts / HOUR_SECS) * HOUR_SECS;
        let slot = hour_peak.entry(bucket).or_insert(0);
        if current > *slot {
            *slot = current;
        }
    }

    if hour_peak.is_empty() {
        return OnlineHistory::default();
    }

    // Fill every hour in the span so the axis is continuous (gaps are zeros).
    let first = *hour_peak.keys().min().expect("non-empty");
    let last = *hour_peak.keys().max().expect("non-empty");
    let mut times = Vec::new();
    let mut players = Vec::new();
    let mut hour = first;
    while hour <= last {
        times.push(hour);
        players.push(*hour_peak.get(&hour).unwrap_or(&0));
        hour += HOUR_SECS;
    }

    OnlineHistory { times, players }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Index of an hour bucket in the series, for readable assertions.
    fn hour_index(history: &OnlineHistory, t: i64) -> Option<usize> {
        history.times.iter().position(|x| *x == t)
    }

    #[test]
    fn closed_sessions_produce_hourly_peaks() {
        let base = 1_700_000_000 / HOUR_SECS * HOUR_SECS;
        // Two players overlapping for ~2 hours, one playing a third hour alone.
        let sessions = vec![
            (1, base + 60, Some(base + 2 * HOUR_SECS)),
            (2, base + 120, Some(base + HOUR_SECS + 300)),
            (3, base + 3 * HOUR_SECS, Some(base + 4 * HOUR_SECS)),
        ];
        let history = compute(&sessions, &HashMap::new());
        assert_eq!(history.times.len(), history.players.len());
        assert_eq!(history.players[0], 2, "both players present in hour 0");
        assert_eq!(history.players[1], 2, "overlap extends into hour 1");
        assert!(history.players[3] >= 1, "the lone session still counts");
    }

    #[test]
    fn open_session_is_bounded_to_the_default_window() {
        let base = 1_700_000_000 / HOUR_SECS * HOUR_SECS;
        let sessions = vec![(1, base, None)];
        let history = compute(&sessions, &HashMap::new());
        // 30 minutes of presence cannot reach the following hour.
        assert_eq!(history.players[0], 1);
        assert_eq!(history.players.get(1).copied().unwrap_or(0), 0);
    }

    #[test]
    fn open_session_extends_to_last_match_activity() {
        let base = 1_700_000_000 / HOUR_SECS * HOUR_SECS;
        let sessions = vec![(1, base, None)];
        let mut activity = HashMap::new();
        activity.insert(1, vec![base + 90 * 60]); // still playing 90 minutes later
        let history = compute(&sessions, &activity);
        assert_eq!(
            history.players[1], 1,
            "activity extends past the default window"
        );
    }

    #[test]
    fn stale_open_session_does_not_bleed_into_the_next_login() {
        let base = 1_700_000_000 / HOUR_SECS * HOUR_SECS;
        let day = 24 * HOUR_SECS;
        let sessions = vec![(1, base, None), (1, base + day, Some(base + day + 600))];
        let history = compute(&sessions, &HashMap::new());
        let second_hour = (base + day) / HOUR_SECS * HOUR_SECS;
        let pos = hour_index(&history, second_hour).expect("second session hour");
        assert_eq!(history.players[pos], 1, "the later session counts");

        // The stale session must not bridge the gap between the two logins.
        let middle = (base + day / 2) / HOUR_SECS * HOUR_SECS;
        let mid_pos = hour_index(&history, middle).expect("midpoint hour is filled in");
        assert_eq!(history.players[mid_pos], 0, "no presence across the gap");
    }

    #[test]
    fn empty_input_is_empty_output() {
        let history = compute(&[], &HashMap::new());
        assert!(history.times.is_empty());
        assert!(history.players.is_empty());
    }
}
