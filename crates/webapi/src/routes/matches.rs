use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqlitePool;

use crate::AppState;
use crate::error::AppError;
use crate::match_detail;
use crate::matches::{Kind, LIMIT_DEFAULT, MatchSummary, PAGE_LIMIT_MAX};
use crate::routes::auth::current_session;

#[derive(Debug, Deserialize)]
pub struct RecentQuery {
    /// Rows per page.
    pub limit: Option<u32>,
    /// 0-based page. `offset` is accepted too, for callers that think in rows.
    pub page: Option<u32>,
    pub offset: Option<u32>,
    /// `pvp`, `pve`, or anything else for the whole archive.
    pub kind: Option<String>,
}

/// `GET /api/matches/recent?limit=N` — the newest completed missions.
///
/// Bounded on purpose: `limit` is clamped, so a client cannot ask the game
/// database for an unbounded scan through this route.
pub async fn recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RecentQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let limit = query
        .limit
        .unwrap_or(LIMIT_DEFAULT)
        .clamp(1, PAGE_LIMIT_MAX);
    let kind = Kind::parse(query.kind.as_deref());
    let offset = crate::matches::page_offset(query.page, query.offset, limit);

    let (total, matches) = state.matches.page(kind, limit, offset).await?;

    // Results are cached for everyone; how *you* did is not, so it is attached
    // per request. Signed-out visitors get the same list without the column.
    let viewer = match current_session(&state, &headers).await? {
        Some(session) => Some((
            session.user_id,
            viewer_sides(&state, session.user_id).await?,
        )),
        None => None,
    };

    let views: Vec<MatchView> = matches
        .into_iter()
        .map(|summary| {
            let outcome = viewer.as_ref().and_then(|(user_id, sides)| {
                let side = sides.get(&summary.instance_id)?;
                Some(ViewerOutcome {
                    user_id: *user_id,
                    task_force: Some(*side),
                    won: summary.winning_task_force == Some(*side),
                    draw: summary.winning_task_force.is_none(),
                })
            });
            MatchView {
                summary,
                viewer: outcome,
            }
        })
        .collect();

    let page = offset / limit;
    Ok(Json(json!({
        "kind": kind.code(),
        "limit": limit,
        "page": page,
        "offset": offset,
        "total": total,
        "has_more": (offset as i64 + views.len() as i64) < total,
        "matches": views,
    }))
    .into_response())
}

#[derive(Debug, Serialize)]
pub struct MatchView {
    #[serde(flatten)]
    pub summary: MatchSummary,
    /// Present only when the request carried a session.
    pub viewer: Option<ViewerOutcome>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewerOutcome {
    pub user_id: i64,
    pub task_force: Option<i64>,
    pub won: bool,
    pub draw: bool,
}

/// The signed-in player's side in each of their recent matches.
///
/// One indexed lookup by `user_id` rather than one query per match; a match the
/// player did not take part in is simply absent from the map.
async fn viewer_sides(state: &AppState, user_id: i64) -> Result<HashMap<i64, i64>, AppError> {
    let Some(pool) = state.stats_pool() else {
        return Ok(HashMap::new());
    };
    sides_for(pool, user_id)
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::new(e)))
}

async fn sides_for(pool: &SqlitePool, user_id: i64) -> sqlx::Result<HashMap<i64, i64>> {
    // The archive is newest-first, so the player's most recent matches are the
    // ones that can appear in it. Anything older cannot be on the page.
    let rows: Vec<(i64, Option<i64>)> = sqlx::query_as(
        "SELECT instance_id, task_force
           FROM ga_match_player_stats
          WHERE user_id = ?
          ORDER BY rowid DESC
          LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut sides = HashMap::new();
    for (instance_id, task_force) in rows {
        if let Some(side) = task_force {
            sides.entry(instance_id).or_insert(side);
        }
    }
    Ok(sides)
}

/// `GET /api/matches/{instance_id}` — one match in full.
///
/// The response carries `kind`, which decides the layout: `pvp` for the rated
/// queues, `pve` for everything else. Degrades to 404 for an unknown or
/// unsealed match rather than inventing an empty page.
pub async fn detail(
    State(state): State<Arc<AppState>>,
    Path(raw_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let Some(live) = state.stats_pool() else {
        return Err(AppError::Unavailable("match archive".to_string()));
    };

    // Parsed here rather than by the extractor so a junk id is the same 404 as
    // an id that simply is not in the archive.
    let Ok(instance_id) = raw_id.parse::<i64>() else {
        return Err(AppError::NotFound("match".to_string()));
    };

    let viewer = current_session(&state, &headers)
        .await?
        .map(|session| session.user_id);

    let detail = match_detail::load(live, instance_id, viewer)
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::new(e)))?
        .ok_or_else(|| AppError::NotFound("match".to_string()))?;

    Ok(Json(detail).into_response())
}
