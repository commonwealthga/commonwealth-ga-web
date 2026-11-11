//! The `/api/status` contract.
//!
//! The Discord bot polls `/api/status` and shows the text verbatim, so the
//! shape is frozen: plain text, `"3 players in-game"`, `"1 player in-game"` for
//! the singular, and an override string when an operator has set one.
//!
//! The count itself comes from the *live* game DB, never from a snapshot — a
//! player count that lags five minutes is worse than useless.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

/// Players currently in a running instance.
///
/// Deliberately identical to the query the Python site used, so counts don't
/// shift across the migration.
pub const ONLINE_PLAYERS_SQL: &str = r#"
    SELECT COUNT(*)
    FROM ga_instance_players ip
    JOIN ga_instances i ON i.instance_id = ip.instance_id
    WHERE ip.left_at IS NULL AND i.state != 'STOPPED'
"#;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Status {
    /// `None` when an operator override replaced the count.
    pub players: Option<u32>,
    pub text: String,
    /// Whether the control server accepted a connection: `None` when this
    /// deployment has no control server configured, so the readers can say
    /// "unknown" rather than inventing an outage.
    pub control_server: Option<bool>,
}

impl Status {
    pub fn from_count(players: u32) -> Self {
        Self {
            players: Some(players),
            text: format_status(players),
            control_server: None,
        }
    }

    pub fn override_text(text: String) -> Self {
        Self {
            players: None,
            text,
            control_server: None,
        }
    }
}

/// The one place the wording lives. Singular on exactly one player.
pub fn format_status(players: u32) -> String {
    if players == 1 {
        "1 player in-game".to_string()
    } else {
        format!("{players} players in-game")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StatusError {
    #[error("live game database is not configured")]
    NotConfigured,
    #[error("live game database error: {0}")]
    Query(#[from] sqlx::Error),
}

/// A status snapshot plus the wall-clock time it was taken, serialised to
/// `/api/live` subscribers so the frontend can render an outage as an outage.
#[derive(Debug, Clone, Serialize)]
pub struct StatusPayload {
    pub ok: bool,
    pub players: Option<u32>,
    pub text: String,
    /// Control server reachable? `None` = not configured on this deployment.
    /// The count and this are different failures: an empty server is healthy,
    /// a control server that does not answer means nobody can even queue.
    pub control_server: Option<bool>,
    pub ts: i64,
}

impl StatusPayload {
    pub fn from_status(status: &Status) -> Self {
        Self {
            ok: true,
            players: status.players,
            text: status.text.clone(),
            control_server: status.control_server,
            ts: now_unix(),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            ok: false,
            players: None,
            text: "server status unavailable".to_string(),
            control_server: None,
            ts: now_unix(),
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub struct StatusService {
    pool: Option<SqlitePool>,
    ttl: Duration,
    override_path: Option<PathBuf>,
    /// `host:port` of the control server, when one is configured. A player
    /// count with no control server behind it is a count of nobody: the server
    /// cannot be played on, so the site says so instead of reporting "0".
    control_addr: Option<String>,
    cache: Mutex<Option<(Instant, Status)>>,
}

impl StatusService {
    pub fn new(
        pool: Option<SqlitePool>,
        ttl: Duration,
        override_path: Option<PathBuf>,
        control_addr: Option<String>,
    ) -> Self {
        Self {
            pool,
            ttl,
            override_path,
            control_addr,
            cache: Mutex::new(None),
        }
    }

    /// Current status, honouring the override and the TTL cache.
    pub async fn current(&self) -> Result<Status, StatusError> {
        // The override wins over everything and is never cached, so setting or
        // deleting the file takes effect on the very next request.
        if let Some(text) = self.override_text() {
            return Ok(Status::override_text(text));
        }

        {
            let cache = self.cache.lock().await;
            if let Some((at, status)) = cache.as_ref()
                && at.elapsed() < self.ttl
            {
                return Ok(status.clone());
            }
        }

        let status = self.query().await?;
        *self.cache.lock().await = Some((Instant::now(), status.clone()));
        Ok(status)
    }

    /// Recompute ignoring the cache. Used by the background ticker that feeds
    /// `/api/live`.
    pub async fn refresh(&self) -> Result<Status, StatusError> {
        if let Some(text) = self.override_text() {
            return Ok(Status::override_text(text));
        }
        let status = self.query().await?;
        *self.cache.lock().await = Some((Instant::now(), status.clone()));
        Ok(status)
    }

    async fn query(&self) -> Result<Status, StatusError> {
        let pool = self.pool.as_ref().ok_or(StatusError::NotConfigured)?;
        let players: i64 = sqlx::query_scalar(ONLINE_PLAYERS_SQL)
            .fetch_one(pool)
            .await?;
        let mut status = Status::from_count(players.max(0) as u32);
        status.control_server = self.control_server_up().await;
        Ok(status)
    }

    /// A TCP connect is the honest check: the control server always listens on
    /// its settings or game port while it is running, and a connect either
    /// completes or does not. Bounded so a wedged host cannot hold the tick.
    async fn control_server_up(&self) -> Option<bool> {
        let addr = self.control_addr.as_ref()?;
        let attempt = tokio::net::TcpStream::connect(addr);
        Some(matches!(
            tokio::time::timeout(Duration::from_millis(750), attempt).await,
            Ok(Ok(_))
        ))
    }

    fn override_text(&self) -> Option<String> {
        let path = self.override_path.as_ref()?;
        let text = std::fs::read_to_string(path).ok()?;
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singular_only_for_one_player() {
        assert_eq!(format_status(0), "0 players in-game");
        assert_eq!(format_status(1), "1 player in-game");
        assert_eq!(format_status(2), "2 players in-game");
    }

    #[test]
    fn payload_reflects_outage() {
        let payload = StatusPayload::unavailable();
        assert!(!payload.ok);
        assert!(payload.players.is_none());
    }
}
