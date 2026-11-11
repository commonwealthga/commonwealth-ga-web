//! Public agent routes: directory, one agent, rating history and leaderboards.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::AppState;
use crate::agents::{self, PAGE_LIMIT_DEFAULT, PAGE_LIMIT_MAX};
use crate::error::{ApiResult, AppError};
use crate::matches::{self as match_data, Kind, page_offset};

#[derive(Debug, Deserialize)]
pub struct DirectoryQuery {
    /// Name search, case-insensitive substring.
    q: Option<String>,
    limit: Option<u32>,
    page: Option<u32>,
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct AgentMatchesQuery {
    kind: Option<String>,
    limit: Option<u32>,
    page: Option<u32>,
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// One of the four class names; omitted means every class at once.
    class: Option<String>,
    /// Trailing window in days; 0 or absent means the whole history.
    days: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct BoardQuery {
    class: Option<String>,
    limit: Option<u32>,
    page: Option<u32>,
    offset: Option<u32>,
}

async fn stats_pool(state: &AppState) -> ApiResult<SqlitePool> {
    state
        .stats_pool()
        .await
        .ok_or_else(|| AppError::Unavailable("live game database".to_string()))
}

/// `GET /api/agents?q=&page=&limit=` — the public directory.
pub async fn directory(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DirectoryQuery>,
) -> ApiResult<Response> {
    let live = stats_pool(&state).await?;
    let live = &live;
    let limit = query
        .limit
        .unwrap_or(PAGE_LIMIT_DEFAULT)
        .clamp(1, PAGE_LIMIT_MAX);
    let offset = page_offset(query.page, query.offset, limit);

    let (total, rows) = agents::directory(live, query.q.as_deref(), limit, offset)
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::new(e)))?;

    Ok(Json(json!({
        "query": query.q,
        "limit": limit,
        "page": offset / limit,
        "offset": offset,
        "total": total,
        "has_more": (offset as i64 + rows.len() as i64) < total,
        "agents": rows,
    }))
    .into_response())
}

/// `GET /api/agents/{user_id}` — one agent's public profile.
pub async fn profile(
    State(state): State<Arc<AppState>>,
    Path(raw_id): Path<String>,
) -> ApiResult<Response> {
    let live = stats_pool(&state).await?;
    let live = &live;
    let Ok(user_id) = raw_id.parse::<i64>() else {
        return Err(AppError::NotFound("agent".to_string()));
    };

    match agents::profile(live, user_id)
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::new(e)))?
    {
        Some(profile) => Ok(Json(profile).into_response()),
        None => Err(AppError::NotFound("agent".to_string())),
    }
}

/// `GET /api/agents/{user_id}/matches?kind=&page=` — their match history.
pub async fn matches(
    State(state): State<Arc<AppState>>,
    Path(raw_id): Path<String>,
    Query(query): Query<AgentMatchesQuery>,
) -> ApiResult<Response> {
    let live = stats_pool(&state).await?;
    let live = &live;
    let Ok(user_id) = raw_id.parse::<i64>() else {
        return Err(AppError::NotFound("agent".to_string()));
    };

    let limit = query
        .limit
        .unwrap_or(PAGE_LIMIT_DEFAULT)
        .clamp(1, PAGE_LIMIT_MAX);
    let kind = Kind::parse(query.kind.as_deref());
    let offset = page_offset(query.page, query.offset, limit);

    let (total, rows) = match_data::player_matches(live, user_id, kind, limit, offset)
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::new(e)))?;

    Ok(Json(json!({
        "user_id": user_id,
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

/// `GET /api/agents/{user_id}/history?class=&days=` — rating over time.
pub async fn history(
    State(state): State<Arc<AppState>>,
    Path(raw_id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Response> {
    let live = stats_pool(&state).await?;
    let live = &live;
    let Ok(user_id) = raw_id.parse::<i64>() else {
        return Err(AppError::NotFound("agent".to_string()));
    };

    let days = query.days.unwrap_or(0).min(3650);

    // No profile check here: a valid account with no rated matches simply has
    // an empty series, which is what the chart renders as "nothing yet".
    let series = agents::rating_series(live, user_id, query.class.as_deref(), days).await;
    Ok(Json(series).into_response())
}

/// `GET /api/leaderboard?class=&page=` — one class's standings.
pub async fn leaderboard(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BoardQuery>,
) -> ApiResult<Response> {
    let live = stats_pool(&state).await?;
    let live = &live;
    let limit = query
        .limit
        .unwrap_or(PAGE_LIMIT_DEFAULT)
        .clamp(1, PAGE_LIMIT_MAX);
    let offset = page_offset(query.page, query.offset, limit);

    let class = query.class.unwrap_or_else(|| "Assault".to_string());
    let board = agents::leaderboard(live, &class, limit, offset).await;

    Ok(Json(json!({
        "classes": crate::mmr::CLASSES,
        "limit": limit,
        "page": offset / limit,
        "offset": offset,
        "has_more": (offset as i64 + board.entries.len() as i64) < board.total,
        "class_name": board.class_name,
        "total": board.total,
        "entries": board.entries,
    }))
    .into_response())
}
