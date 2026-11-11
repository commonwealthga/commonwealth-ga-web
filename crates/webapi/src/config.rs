//! Runtime configuration, entirely from the environment so the container needs
//! no config file and no arguments.

use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    /// Address the HTTP server binds to. TLS is terminated in front of it
    /// (Caddy, or the host's own proxy) — this process speaks plain HTTP.
    pub bind: SocketAddr,
    /// The live game database. Mounted read-only, opened read-only, never
    /// written. Only `/api/status` reads from it: the Discord bot's player
    /// count must not lag behind a snapshot.
    pub live_db: Option<PathBuf>,
    /// Redacted periodic snapshot of the live DB — the source for every heavy
    /// stats query.
    pub snapshot_db: Option<PathBuf>,
    /// Website-owned read-write database (accounts, sessions, user content).
    pub site_db: Option<PathBuf>,
    /// Built frontend (`web/build`). Served as static assets with an SPA
    /// fallback.
    pub static_dir: PathBuf,
    /// Download directory served under `/files/*` — restored map archives the
    /// launcher fetches. Read from disk per request so files can be added or
    /// replaced without a restart.
    pub files_dir: PathBuf,
    /// How long a player count may be reused before the DB is queried again.
    pub status_ttl: Duration,
    /// How often the status is recomputed and pushed to `/api/live` clients.
    pub live_tick: Duration,
    /// How long a recent-matches result may be reused.
    pub match_ttl: Duration,
    /// How long the player-activity series may be reused. It walks every
    /// session ever recorded, so this is deliberately slow-changing.
    pub history_ttl: Duration,
    /// Optional text that replaces the status line entirely (maintenance
    /// notices). Read fresh on every request; absent/empty means no override.
    pub status_override: Option<PathBuf>,
    /// The control server's player-settings endpoint, e.g.
    /// `http://127.0.0.1:19020`. Absent disables the settings *writes*; the
    /// page still shows current values.
    pub control_api_url: Option<String>,
    /// Shared token for that endpoint. Never in the repo: it travels the same
    /// way the db-export token does, through the deploy-time override.
    pub control_api_token: Option<String>,
    /// Path token for `/db-export/{token}`. Absent means the route 404s, which
    /// is the right default: the redacted database should not be public just
    /// because nobody configured a secret.
    pub db_export_token: Option<String>,
}

impl Config {
    /// `host:port` of the control server, for the liveness probe.
    ///
    /// Derived from `WEB_CONTROL_API_URL` so there is one place to configure
    /// it; a URL is for humans, a socket address is for `TcpStream`.
    pub fn control_addr(&self) -> Option<String> {
        let url = self.control_api_url.as_deref()?;
        let authority = url
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_end_matches('/');
        let authority = authority.split('/').next().unwrap_or(authority);
        (!authority.is_empty()).then(|| authority.to_string())
    }

    pub fn from_env() -> Self {
        let live_db = env_path("WEB_DB_LIVE");
        let site_db = env_path("WEB_DB_SITE");
        let static_dir = env_path("WEB_STATIC_DIR").unwrap_or_else(|| PathBuf::from("web/build"));
        Self {
            bind: env("WEB_BIND", "0.0.0.0:8080")
                .parse()
                .expect("WEB_BIND must be a socket address like 0.0.0.0:8080"),
            status_override: resolve_override(
                env_path("WEB_STATUS_OVERRIDE"),
                site_db.as_deref(),
                live_db.as_deref(),
            ),
            live_db,
            snapshot_db: env_path("WEB_DB_SNAPSHOT"),
            site_db,
            files_dir: crate::files::resolve_dir(env_path("WEB_FILES_DIR")),
            static_dir,
            status_ttl: env_duration("WEB_STATUS_TTL_SECS", 60),
            live_tick: env_duration("WEB_LIVE_TICK_SECS", 15),
            match_ttl: env_duration("WEB_MATCH_TTL_SECS", 30),
            history_ttl: env_duration("WEB_HISTORY_TTL_SECS", 3600),
            control_api_url: env_opt("WEB_CONTROL_API_URL"),
            control_api_token: env_opt("WEB_CONTROL_API_TOKEN"),
            db_export_token: env_opt("WEB_DB_EXPORT_TOKEN"),
        }
    }
}

/// Where the maintenance-override file lives.
///
/// An explicit setting always wins. Otherwise prefer the *writable* site-data
/// directory: the live DB is mounted read-only in production, so an operator
/// could not create a file next to it.
fn resolve_override(
    explicit: Option<PathBuf>,
    site_db: Option<&Path>,
    live_db: Option<&Path>,
) -> Option<PathBuf> {
    explicit
        .or_else(|| site_db.and_then(default_override_path))
        .or_else(|| live_db.and_then(default_override_path))
}

/// `status-override.txt` in the database's directory — same filename the
/// previous Python site used, so operators keep one thing to remember.
fn default_override_path(db: &Path) -> Option<PathBuf> {
    Some(db.parent()?.join("status-override.txt"))
}

fn env(key: &str, default: &str) -> String {
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => v,
        _ => default.to_string(),
    }
}

/// An optional string setting: absent or blank means "not configured".
fn env_opt(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_path(key: &str) -> Option<PathBuf> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn env_duration(key: &str, default_secs: u64) -> Duration {
    let secs = env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default_secs);
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_defaults_next_to_the_database() {
        let path = default_override_path(Path::new("/data/server.db")).unwrap();
        assert_eq!(path, PathBuf::from("/data/status-override.txt"));
    }

    #[test]
    fn override_prefers_the_writable_site_dir_then_the_live_db() {
        // Production shape: live DB read-only, site DB writable -> use site.
        assert_eq!(
            resolve_override(
                None,
                Some(Path::new("/site-data/site.db")),
                Some(Path::new("/data/server.db"))
            ),
            Some(PathBuf::from("/site-data/status-override.txt"))
        );
        // Site DB not configured -> fall back next to the live DB.
        assert_eq!(
            resolve_override(None, None, Some(Path::new("/data/server.db"))),
            Some(PathBuf::from("/data/status-override.txt"))
        );
        // An explicit setting always wins, and nothing configured means off.
        assert_eq!(
            resolve_override(
                Some(PathBuf::from("/etc/override.txt")),
                Some(Path::new("/site-data/site.db")),
                Some(Path::new("/data/server.db"))
            ),
            Some(PathBuf::from("/etc/override.txt"))
        );
        assert_eq!(resolve_override(None, None, None), None);
    }

    #[test]
    fn override_default_works_for_a_relative_db_path() {
        // Parent of a bare filename is the empty path, so the override file
        // lands in the working directory next to the (relative) database.
        assert_eq!(
            default_override_path(Path::new("server.db")),
            Some(PathBuf::from("status-override.txt"))
        );
    }
}
