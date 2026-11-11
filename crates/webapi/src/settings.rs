//! Player settings: the preferences that used to be chat-command only.
//!
//! Reading is the website's own business — the values sit in `server.db`,
//! which is mounted read-only, so `GET` never needs the control server.
//! Writing is not: the live effect of the suit toggles is an IPC player-action
//! that only the control server can push to the game DLL, and the DLL caches a
//! player's preferences in memory, so a bare database write would not be seen
//! until the player relogged. Writes are therefore forwarded to the control
//! server's settings endpoint, which persists *and* pushes.
//!
//! With `WEB_CONTROL_API_URL` unset the page still renders (values and help),
//! and only the toggles are reported as unavailable.

use serde::Serialize;
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

/// One toggle, as the page needs it.
#[derive(Debug, Clone, Serialize)]
pub struct Preference {
    pub key: String,
    pub value: String,
    /// What the game shows the player; used for the label, not the logic.
    pub enabled: bool,
}

/// One DLC pack from `ga_dlc` plus this account's installed flag.
#[derive(Debug, Clone, Serialize)]
pub struct DlcPack {
    pub identifier: String,
    pub name: String,
    pub installed: bool,
    /// `pvp`, `pve`, or null for a pack with no maps linked to it yet. Derived
    /// from the pack's own map pool rather than a hand-kept list, so a new pack
    /// classifies itself.
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    pub preferences: Vec<Preference>,
    pub dlc: Vec<DlcPack>,
    /// Whether writes can actually be delivered right now.
    pub writable: bool,
}

/// The keys the page may change, with the value each defaults to when the
/// account has never touched it. Mirrors the whitelist in the control server's
/// settings endpoint — a key missing there is rejected there, so keep both in
/// step when adding one.
pub const KEYS: [(&str, &str); 3] = [
    ("show_broken_suits", "1"),
    ("show_all_suits", "1"),
    ("solo_mode", "0"),
];

pub async fn load(live: &SqlitePool, user_id: i64, writable: bool) -> Settings {
    let mut preferences = Vec::with_capacity(KEYS.len());

    for (key, default) in KEYS {
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT config_value FROM ga_user_preferences WHERE user_id = ? AND config_key = ?",
        )
        .bind(user_id)
        .bind(key)
        .fetch_optional(live)
        .await
        .ok()
        .flatten();

        let value = stored
            .filter(|v| v == "0" || v == "1")
            .unwrap_or_else(|| default.to_string());
        preferences.push(Preference {
            key: key.to_string(),
            enabled: value == "1",
            value,
        });
    }

    Settings {
        preferences,
        dlc: load_dlc(live, user_id).await,
        writable,
    }
}

/// The DLC catalog with this account's flags. Empty when the game database
/// predates the DLC tables.
async fn load_dlc(live: &SqlitePool, user_id: i64) -> Vec<DlcPack> {
    // `ga_map_pool_entries.dlc_id` is what ties a pack to its maps, and
    // `map_game_info.is_pvp` says which side of the game they belong to.
    let rows = sqlx::query(
        "SELECT d.identifier AS identifier,
                d.name AS name,
                COALESCE(MAX(ud.installed), 0) AS installed,
                COUNT(mgi.map_name) AS linked_maps,
                COALESCE(MAX(mgi.is_pvp), 0) AS any_pvp
           FROM ga_dlc d
           LEFT JOIN ga_user_dlc ud ON ud.dlc_id = d.dlc_id AND ud.user_id = ?
           LEFT JOIN ga_map_pool_entries p ON p.dlc_id = d.dlc_id
           LEFT JOIN map_game_info mgi ON mgi.map_name = p.map_name
          GROUP BY d.dlc_id
          ORDER BY d.dlc_id",
    )
    .bind(user_id)
    .fetch_all(live)
    .await;

    let Ok(rows) = rows else {
        return Vec::new();
    };

    rows.iter()
        .map(|row| {
            let linked = row
                .try_get::<Option<i64>, _>("linked_maps")
                .ok()
                .flatten()
                .unwrap_or(0);
            let any_pvp = row
                .try_get::<Option<i64>, _>("any_pvp")
                .ok()
                .flatten()
                .unwrap_or(0);
            DlcPack {
                identifier: row.try_get("identifier").unwrap_or_default(),
                name: row.try_get("name").unwrap_or_default(),
                installed: row
                    .try_get::<Option<i64>, _>("installed")
                    .ok()
                    .flatten()
                    .unwrap_or(0)
                    == 1,
                mode: if linked == 0 {
                    None
                } else if any_pvp == 1 {
                    Some("pvp".to_string())
                } else {
                    Some("pve".to_string())
                },
            }
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("player settings are not configured on this server")]
    NotConfigured,
    #[error("the game server refused the change")]
    Rejected,
    #[error("the game server could not be reached")]
    Unreachable,
}

/// Forward one change to the control server.
///
/// Hand-rolled over a loopback socket rather than pulling an HTTP client in:
/// the endpoint is plain HTTP on 127.0.0.1 and one-shot per connection, and
/// this crate has no HTTP *client* dependency for anything else. If the
/// endpoint ever moves off the host, this is the seam to replace.
pub async fn send(
    base_url: &str,
    token: &str,
    path: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, SettingsError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let authority = base_url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    if authority.is_empty() {
        return Err(SettingsError::NotConfigured);
    }

    let payload = body.to_string();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nX-Admin-Token: {token}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        payload.len(),
        payload
    );

    let deadline = std::time::Duration::from_secs(5);
    let mut stream = tokio::time::timeout(deadline, tokio::net::TcpStream::connect(authority))
        .await
        .map_err(|_| SettingsError::Unreachable)?
        .map_err(|_| SettingsError::Unreachable)?;

    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|_| SettingsError::Unreachable)?;

    let mut raw = Vec::new();
    tokio::time::timeout(deadline, stream.read_to_end(&mut raw))
        .await
        .map_err(|_| SettingsError::Unreachable)?
        .map_err(|_| SettingsError::Unreachable)?;

    let text = String::from_utf8_lossy(&raw).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or(SettingsError::Unreachable)?;

    let body_start = text
        .find("\r\n\r\n")
        .map(|index| index + 4)
        .unwrap_or(text.len());
    let body_text = text[body_start..].trim();

    match status {
        200 => serde_json::from_str(body_text).map_err(|_| SettingsError::Unreachable),
        400 | 403 | 404 => Err(SettingsError::Rejected),
        _ => Err(SettingsError::Unreachable),
    }
}
