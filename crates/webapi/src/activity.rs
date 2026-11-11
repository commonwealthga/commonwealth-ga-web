//! How busy the server has been lately.
//!
//! The homepage hero used to print the live player count, which is often zero
//! at 9am on a Tuesday — a number that says "nobody is here" to a visitor
//! deciding whether to bother. What it shows instead is how many agents played,
//! and how many missions ran, over the most recent window that has anything in
//! it: 24 hours, then 48, then 72, a week, a month, all time.
//!
//! The window is chosen server-side so the label and the numbers cannot
//! disagree, and the whole thing is cached: it only changes when the snapshot
//! does (every five minutes), so a 60-second cache is invisible.

use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::snapshot::StatsSource;

/// Widest first? No — narrowest first, and the first window with any activity
/// wins. `0` means "all time" and is the last resort.
pub const WINDOWS_HOURS: [u32; 6] = [24, 48, 72, 168, 720, 0];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Activity {
    /// The window these numbers cover. `0` = all time.
    pub window_hours: u32,
    /// Distinct agents with a recorded match in that window.
    pub agents: i64,
    /// Completed missions (instances with a queue) in that window.
    pub missions: i64,
}

/// Counts for one window.
///
/// Written as two scalar subqueries rather than a join: joined from the stats
/// side, SQLite scans every instance and probes stats per row (the same trap
/// that made the profile queries take 1.4 s). This shape uses the instance-id
/// index instead.
const COUNT_SQL: &str = r#"
    SELECT
      (SELECT COUNT(DISTINCT s.user_id)
         FROM ga_match_player_stats s
        WHERE s.instance_id IN (
                  SELECT instance_id FROM ga_instances
                   WHERE queue_id IS NOT NULL AND (?1 = 0 OR started_at >= ?2)
              )
      ) AS agents,
      (SELECT COUNT(*)
         FROM ga_instances
        WHERE queue_id IS NOT NULL AND (?1 = 0 OR started_at >= ?2)
      ) AS missions
"#;

pub async fn window(
    pool: &SqlitePool,
    window_hours: u32,
    now: i64,
) -> Result<(i64, i64), sqlx::Error> {
    let since = if window_hours == 0 {
        0
    } else {
        now - i64::from(window_hours) * 3600
    };
    let row: (i64, i64) = sqlx::query_as(COUNT_SQL)
        .bind(i64::from(window_hours))
        .bind(since)
        .fetch_one(pool)
        .await?;
    Ok(row)
}

/// The narrowest window that has anything in it.
pub async fn summary(pool: &SqlitePool, now: i64) -> Result<Activity, sqlx::Error> {
    for window_hours in WINDOWS_HOURS {
        let (agents, missions) = window(pool, window_hours, now).await?;
        if agents > 0 || missions > 0 {
            return Ok(Activity {
                window_hours,
                agents,
                missions,
            });
        }
    }
    Ok(Activity {
        window_hours: 0,
        agents: 0,
        missions: 0,
    })
}

/// A cached view of [`summary`], for a hero block every visitor loads.
pub struct ActivityService {
    stats: StatsSource,
    ttl: Duration,
    cache: Mutex<Option<(Instant, Activity)>>,
}

impl ActivityService {
    pub fn new(stats: impl Into<StatsSource>, ttl: Duration) -> Self {
        Self {
            stats: stats.into(),
            ttl,
            cache: Mutex::new(None),
        }
    }

    pub async fn current(&self) -> Option<Activity> {
        {
            let cache = self.cache.lock().await;
            if let Some((at, activity)) = cache.as_ref()
                && at.elapsed() < self.ttl
            {
                return Some(activity.clone());
            }
        }

        // Re-checked per call: the snapshotter replaces the file as it runs.
        let pool = self.stats.pool().await?;
        let activity = summary(&pool, now_unix()).await.ok()?;
        *self.cache.lock().await = Some((Instant::now(), activity.clone()));
        Some(activity)
    }
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

    #[test]
    fn the_windows_go_from_tight_to_all_time() {
        assert_eq!(WINDOWS_HOURS[0], 24);
        // `0` is the "all time" sentinel and closes the ladder; everything
        // before it must widen strictly, so the walk in `summary` terminates
        // and never revisits a window.
        assert_eq!(*WINDOWS_HOURS.last().unwrap(), 0);
        for pair in WINDOWS_HOURS[..WINDOWS_HOURS.len() - 1].windows(2) {
            assert!(pair[0] < pair[1], "windows must widen: {pair:?}");
        }
    }
}
