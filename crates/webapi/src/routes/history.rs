use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::AppState;
use crate::error::ApiResult;
use crate::history::{DAYS_DEFAULT, DAYS_MAX};

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    days: Option<u32>,
}

/// `GET /api/online-history?days=N` — hourly peak concurrent players.
///
/// Two parallel arrays, hour-aligned and oldest first, ready to plot. The whole
/// series is computed once and cached; the window is applied per request.
pub async fn series(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Response> {
    // 0 is the explicit "everything" request; anything else is a day window.
    let days = query.days.unwrap_or(DAYS_DEFAULT).min(DAYS_MAX);
    let history = state.history.series(days).await?;

    Ok(Json(json!({
        "days": days,
        "times": history.times,
        "players": history.players,
    }))
    .into_response())
}
