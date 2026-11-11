//! Sign-in with in-game credentials.
//!
//! The game never sends a password: the client derives an RC4 keystream from
//! `MD5(MD5(utf16le(lower(user) + password + STATIC_SALT)))` and encrypts a
//! known plaintext with it. The control server stores `SHA256(keystream)` in
//! `ga_users.password_verifier` (see `Auth/LoginAuth.hpp` in the server repo).
//!
//! So this module never sees a password either. The browser reproduces the
//! derivation and posts the keystream; we hash it and compare. The keystream is
//! therefore password-equivalent: it is only ever accepted over HTTPS, never
//! logged, and immediately exchanged for an opaque session cookie.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// The client's static salt, recovered by reverse engineering (see
/// `.planning/2026-06-08-login-password-flow.md` in the server repo).
pub const STATIC_SALT: &str = "FA00CDF5-8D87-4a76-BEE1-D9E483220C13";

/// `UuidToStringW()` canonical form: 36 characters, always.
pub const GUID_STRING_LEN: usize = 36;

pub const SESSION_COOKIE: &str = "cw_session";
pub const SESSION_TTL: Duration = Duration::from_secs(30 * 24 * 3600);

/// Failed sign-in attempts tolerated per IP inside `ATTEMPT_WINDOW`.
pub const ATTEMPTS_PER_WINDOW: u32 = 10;
pub const ATTEMPT_WINDOW: Duration = Duration::from_secs(5 * 60);

/// Byte length of the keystream the client produces for an account name:
/// the encrypted plaintext is `guidString + lower(user)`, in UTF-16LE.
pub fn expected_keystream_len(username: &str) -> usize {
    (GUID_STRING_LEN + username.len()) * 2
}

/// `SHA256(keystream)` — the value stored in `ga_users.password_verifier`.
pub fn verifier_from_keystream(keystream: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(keystream);
    hasher.finalize().into()
}

/// Constant-time comparison, so a wrong verifier cannot be probed byte by byte.
pub fn verifier_matches(stored: &[u8], computed: &[u8]) -> bool {
    if stored.len() != computed.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in stored.iter().zip(computed.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Account names are ASCII and modest in length; anything else is rejected
/// before it can reach the database.
pub fn valid_username(username: &str) -> bool {
    !username.is_empty()
        && username.len() <= 32
        && username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Session rows live in the website's own database, never in the game DB.
pub async fn migrate(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS web_sessions (
             token      TEXT PRIMARY KEY,
             user_id    INTEGER NOT NULL,
             username   TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             expires_at INTEGER NOT NULL
         )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_web_sessions_expires ON web_sessions(expires_at)")
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Session {
    pub user_id: i64,
    pub username: String,
}

pub async fn create_session(
    pool: &SqlitePool,
    user_id: i64,
    username: &str,
) -> sqlx::Result<String> {
    let mut raw = [0u8; 32];
    rand::rng().fill_bytes(&mut raw);
    let token: String = raw.iter().map(|b| format!("{b:02x}")).collect();

    let now = now_unix();
    let expires = now + SESSION_TTL.as_secs() as i64;
    sqlx::query(
        "INSERT INTO web_sessions (token, user_id, username, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&token)
    .bind(user_id)
    .bind(username)
    .bind(now)
    .bind(expires)
    .execute(pool)
    .await?;

    // Opportunistic cleanup keeps the table from growing forever.
    sqlx::query("DELETE FROM web_sessions WHERE expires_at < ?")
        .bind(now)
        .execute(pool)
        .await?;

    Ok(token)
}

pub async fn load_session(pool: &SqlitePool, token: &str) -> sqlx::Result<Option<Session>> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT user_id, username FROM web_sessions WHERE token = ? AND expires_at > ?",
    )
    .bind(token)
    .bind(now_unix())
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(user_id, username)| Session { user_id, username }))
}

pub async fn delete_session(pool: &SqlitePool, token: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM web_sessions WHERE token = ?")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

/// Minimal cookie extraction — one dependency fewer than a cookie crate for the
/// single cookie this service sets.
pub fn cookie_value<'a>(cookie_header: &'a str, name: &str) -> Option<&'a str> {
    cookie_header.split(';').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key.trim() == name).then_some(value.trim())
    })
}

pub fn format_session_cookie(token: &str, secure: bool) -> String {
    let mut cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        SESSION_TTL.as_secs()
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

pub fn format_cleared_cookie(secure: bool) -> String {
    let mut cookie = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Crude in-memory throttle: this is a single-container service, so a shared
/// store would be over-engineering. It exists to make online guessing pointless.
#[derive(Default)]
pub struct AttemptLimiter {
    hits: Mutex<HashMap<IpAddr, (Instant, u32)>>,
}

impl AttemptLimiter {
    pub fn allowed(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut hits = self.hits.lock().expect("attempt limiter poisoned");
        let entry = hits.entry(ip).or_insert((now, 0));
        if now.duration_since(entry.0) > ATTEMPT_WINDOW {
            *entry = (now, 0);
        }
        entry.1 < ATTEMPTS_PER_WINDOW
    }

    pub fn record_failure(&self, ip: IpAddr) {
        let now = Instant::now();
        let mut hits = self.hits.lock().expect("attempt limiter poisoned");
        let entry = hits.entry(ip).or_insert((now, 0));
        if now.duration_since(entry.0) > ATTEMPT_WINDOW {
            *entry = (now, 0);
        }
        entry.1 += 1;
    }

    pub fn clear(&self, ip: IpAddr) {
        self.hits
            .lock()
            .expect("attempt limiter poisoned")
            .remove(&ip);
    }
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keystream_length_matches_the_client_plaintext() {
        // guidString (36) + account name, in UTF-16LE.
        assert_eq!(expected_keystream_len("agent"), 41 * 2);
        assert_eq!(expected_keystream_len(""), 36 * 2);
    }

    #[test]
    fn verifier_is_sha256_of_the_keystream() {
        // Known-answer check: SHA-256 of the single byte 0x00.
        let digest = verifier_from_keystream(&[0x00]);
        assert_eq!(
            digest[..4],
            [0x6e, 0x34, 0x0b, 0x9c],
            "SHA-256 must match a reference implementation"
        );
    }

    #[test]
    fn verifier_comparison_rejects_length_and_content_mismatches() {
        let a = verifier_from_keystream(b"keystream");
        let b = verifier_from_keystream(b"keystream");
        let c = verifier_from_keystream(b"other");
        assert!(verifier_matches(&a, &b));
        assert!(!verifier_matches(&a, &c));
        assert!(!verifier_matches(&a, &a[..16]));
    }

    #[test]
    fn usernames_are_ascii_and_bounded() {
        assert!(valid_username("Agent_01"));
        assert!(valid_username("a"));
        assert!(!valid_username(""));
        assert!(!valid_username("with space"));
        assert!(!valid_username("ünicode"));
        assert!(!valid_username(&"x".repeat(33)));
    }

    #[test]
    fn cookie_header_parsing_finds_the_session() {
        let header = "theme=dark; cw_session=abc123; other=1";
        assert_eq!(cookie_value(header, SESSION_COOKIE), Some("abc123"));
        assert_eq!(cookie_value(header, "missing"), None);
        assert_eq!(cookie_value("", SESSION_COOKIE), None);
    }

    #[test]
    fn limiter_blocks_after_the_budget_and_resets_when_cleared() {
        let limiter = AttemptLimiter::default();
        let ip: IpAddr = "203.0.113.7".parse().expect("ip");
        for _ in 0..ATTEMPTS_PER_WINDOW {
            assert!(limiter.allowed(ip));
            limiter.record_failure(ip);
        }
        assert!(!limiter.allowed(ip), "budget should be exhausted");
        limiter.clear(ip);
        assert!(limiter.allowed(ip), "clearing resets the budget");
    }
}
