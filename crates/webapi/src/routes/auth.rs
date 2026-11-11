//! Sign-in, sign-out, and the signed-in player's own data.
//!
//! Everything under `/api/me` requires a session; everything here degrades to a
//! clean 401 rather than leaking why.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqlitePool;

use crate::AppState;
use crate::auth::{self, SESSION_COOKIE, Session};
use crate::error::{ApiResult, AppError};
use crate::matches::{Kind, PAGE_LIMIT_DEFAULT, PAGE_LIMIT_MAX};
use crate::routes::matches::RecentQuery;

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub username: String,
    /// Hex-encoded RC4 keystream, derived in the browser from the credentials.
    pub keystream: String,
}

#[derive(Debug, Serialize)]
struct PublicUser {
    id: i64,
    username: String,
}

/// `POST /api/auth/login`
pub async fn login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> ApiResult<Response> {
    let ip = client_ip(&headers);
    if !state.limiter.allowed(ip) {
        return Err(AppError::TooManyAttempts);
    }

    let username = body.username.trim().to_string();
    if !auth::valid_username(&username) {
        return Err(AppError::BadRequest(
            "account names are 1-32 characters, letters, digits, '_' or '-'".to_string(),
        ));
    }

    let keystream = decode_hex(&body.keystream)
        .ok_or_else(|| AppError::BadRequest("malformed key material".to_string()))?;
    if keystream.len() != auth::expected_keystream_len(&username) {
        return Err(AppError::BadRequest("malformed key material".to_string()));
    }

    // Login reads a credential, so it stays on the live database: a snapshot
    // could be five minutes old and would reject an account registered since.
    let live = state
        .live_pool
        .as_ref()
        .ok_or_else(|| AppError::Unavailable("live game database".to_string()))?;

    let row: Option<(i64, String, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT id, username, password_verifier FROM ga_users
          WHERE lower(username) = lower(?) LIMIT 1",
    )
    .bind(&username)
    .fetch_optional(live)
    .await
    .map_err(|e| AppError::Internal(anyhow::Error::new(e)))?;

    let Some((user_id, stored_name, verifier)) = row else {
        state.limiter.record_failure(ip);
        return Err(AppError::Unauthorized);
    };

    let Some(verifier) = verifier else {
        // Account exists but has never completed a game login, so no credential
        // has been registered yet.
        return Err(AppError::Forbidden(
            "this account has no password yet — log into the game once first".to_string(),
        ));
    };

    if !auth::verifier_matches(&verifier, &auth::verifier_from_keystream(&keystream)) {
        state.limiter.record_failure(ip);
        return Err(AppError::Unauthorized);
    }

    let site = state
        .site_pool
        .as_ref()
        .ok_or_else(|| AppError::Unavailable("civic datastore".to_string()))?;
    let token = auth::create_session(site, user_id, &stored_name)
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::new(e)))?;

    state.limiter.clear(ip);

    Ok((
        [(
            header::SET_COOKIE,
            auth::format_session_cookie(&token, is_https(&headers)),
        )],
        Json(json!({ "user": PublicUser { id: user_id, username: stored_name } })),
    )
        .into_response())
}

/// `POST /api/auth/logout`
pub async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let (Some(site), Some(token)) = (state.site_pool.as_ref(), session_token(&headers))
        && let Err(err) = auth::delete_session(site, token).await
    {
        tracing::warn!(error = %err, "failed to delete session");
    }

    (
        [(
            header::SET_COOKIE,
            auth::format_cleared_cookie(is_https(&headers)),
        )],
        Json(json!({ "ok": true })),
    )
        .into_response()
}

/// `GET /api/auth/me`
pub async fn me(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult<Response> {
    match current_session(&state, &headers).await? {
        Some(session) => Ok(Json(json!({
            "authenticated": true,
            "user": PublicUser { id: session.user_id, username: session.username },
        }))
        .into_response()),
        None => Ok(Json(json!({ "authenticated": false })).into_response()),
    }
}

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    #[allow(dead_code)]
    limit: Option<u32>,
}

/// `GET /api/me/matches?limit=N` — this player's recent matches.
pub async fn matches(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<RecentQuery>,
) -> ApiResult<Response> {
    let session = require_session(&state, &headers).await?;

    let live = state
        .stats_pool()
        .ok_or_else(|| AppError::Unavailable("live game database".to_string()))?;

    let limit = query
        .limit
        .unwrap_or(PAGE_LIMIT_DEFAULT)
        .clamp(1, PAGE_LIMIT_MAX);
    let kind = Kind::parse(query.kind.as_deref());
    let offset = match (query.page, query.offset) {
        (Some(page), _) => page.saturating_mul(limit),
        (None, Some(offset)) => offset,
        _ => 0,
    };

    let (total, rows) = crate::matches::player_matches(live, session.user_id, kind, limit, offset)
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::new(e)))?;

    Ok(Json(json!({
        "kind": kind.code(),
        "limit": limit,
        "page": offset / limit,
        "offset": offset,
        "total": total,
        "has_more": (offset as i64 + rows.len() as i64) < total,
        "matches": rows,
    }))
    .into_response())
}

#[derive(Debug, Serialize)]
pub struct MmrEntry {
    pub class_name: String,
    pub mmr: f64,
    pub games: i64,
    pub played_at: i64,
}

/// `GET /api/me/summary` — career totals and current rating per class.
pub async fn summary(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let session = require_session(&state, &headers).await?;
    let live = state
        .stats_pool()
        .ok_or_else(|| AppError::Unavailable("live game database".to_string()))?;

    let totals: CareerTotals = sqlx::query_as(
        "SELECT COUNT(*) AS matches,
                COALESCE(SUM(kills), 0) AS kills,
                COALESCE(SUM(deaths), 0) AS deaths,
                COALESCE(SUM(assists), 0) AS assists,
                COALESCE(SUM(damage_dealt), 0) AS damage_dealt,
                COALESCE(SUM(healing), 0) AS healing,
                COALESCE(SUM(obj_points), 0) AS obj_points,
                COALESCE(CAST(SUM(time_played_seconds) AS INTEGER), 0) AS seconds_played
           FROM ga_match_player_stats WHERE user_id = ?",
    )
    .bind(session.user_id)
    .fetch_one(live)
    .await
    .map_err(|e| AppError::Internal(anyhow::Error::new(e)))?;

    // Rating per class, from whichever engine the control server is feeding.
    let ratings = crate::mmr::latest_for_user(live, session.user_id).await;
    let mmr: Vec<MmrEntry> = ratings
        .into_iter()
        .map(|rating| MmrEntry {
            class_name: rating.class_name,
            mmr: rating.mmr,
            games: rating.games,
            played_at: rating.played_at,
        })
        .collect();

    // Agenda points live on the account row.

    Ok(Json(json!({
        "username": session.username,
        "totals": totals,
        "mmr": mmr,
    }))
    .into_response())
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CareerTotals {
    pub matches: i64,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub damage_dealt: i64,
    pub healing: i64,
    pub obj_points: i64,
    pub seconds_played: i64,
}

pub(crate) async fn current_session(
    state: &AppState,
    headers: &HeaderMap,
) -> ApiResult<Option<Session>> {
    let Some(site) = state.site_pool.as_ref() else {
        return Ok(None);
    };
    let Some(token) = session_token(headers) else {
        return Ok(None);
    };
    auth::load_session(site, token)
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::new(e)))
}

async fn require_session(state: &AppState, headers: &HeaderMap) -> ApiResult<Session> {
    current_session(state, headers)
        .await?
        .ok_or(AppError::Unauthorized)
}

fn session_token(headers: &HeaderMap) -> Option<&str> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    auth::cookie_value(cookie, SESSION_COOKIE)
}

/// Behind Caddy the connection is loopback, so the forwarded address is the only
/// meaningful one. Anything unparseable buckets together.
fn client_ip(headers: &HeaderMap) -> IpAddr {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
}

/// Same reasoning for the cookie's Secure flag.
fn is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.eq_ignore_ascii_case("https"))
        .unwrap_or(false)
}

fn decode_hex(input: &str) -> Option<Vec<u8>> {
    let bytes = input.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
    }
    Some(out)
}

#[allow(dead_code)]
fn assert_pool_type(_: &SqlitePool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_decoding_rejects_odd_and_invalid_input() {
        assert_eq!(decode_hex("00ff10"), Some(vec![0x00, 0xff, 0x10]));
        assert_eq!(decode_hex(""), Some(vec![]));
        assert_eq!(decode_hex("abc"), None);
        assert_eq!(decode_hex("zz"), None);
    }
}
