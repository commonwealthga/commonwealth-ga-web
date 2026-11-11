//! `GET /api/me/settings` and the two writes behind it.
//!
//! Reading is a database read; writing is a call to the control server, which
//! persists the preference and pushes the live change to the game DLL. The
//! website never writes the game database.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::AppState;
use crate::error::{ApiResult, AppError};
use crate::routes::auth::current_session;
use crate::settings;

#[derive(Debug, Deserialize)]
pub struct PreferenceBody {
    key: String,
    value: String,
}

#[derive(Debug, Deserialize)]
pub struct DlcBody {
    identifier: String,
    installed: bool,
}

/// The control server's settings endpoint, when configured.
fn endpoint(state: &AppState) -> Option<(&str, &str)> {
    match (
        state.cfg.control_api_url.as_deref(),
        state.cfg.control_api_token.as_deref(),
    ) {
        (Some(url), Some(token)) if !url.is_empty() && !token.is_empty() => Some((url, token)),
        _ => None,
    }
}

pub async fn get(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult<Response> {
    let session = current_session(&state, &headers)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let live = state
        .live_pool
        .as_ref()
        .ok_or_else(|| AppError::Unavailable("live game database".to_string()))?;

    let settings = settings::load(live, session.user_id, endpoint(&state).is_some()).await;
    Ok(Json(settings).into_response())
}

pub async fn set_preference(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PreferenceBody>,
) -> ApiResult<Response> {
    let session = current_session(&state, &headers)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if !settings::KEYS.iter().any(|(key, _)| *key == body.key) {
        return Err(AppError::BadRequest("unknown preference".to_string()));
    }
    if body.value != "0" && body.value != "1" {
        return Err(AppError::BadRequest("value must be '0' or '1'".to_string()));
    }

    let Some((url, token)) = endpoint(&state) else {
        return Err(AppError::Unavailable("player settings".to_string()));
    };

    let reply = settings::send(
        url,
        token,
        "/player/preferences",
        &json!({ "user_id": session.user_id, "key": body.key, "value": body.value }),
    )
    .await?;

    Ok(Json(reply).into_response())
}

pub async fn set_dlc(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<DlcBody>,
) -> ApiResult<Response> {
    let session = current_session(&state, &headers)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if body.identifier.trim().is_empty() {
        return Err(AppError::BadRequest("missing identifier".to_string()));
    }

    let Some((url, token)) = endpoint(&state) else {
        return Err(AppError::Unavailable("player settings".to_string()));
    };

    let reply = settings::send(
        url,
        token,
        "/player/dlc",
        &json!({ "user_id": session.user_id, "identifier": body.identifier, "installed": body.installed }),
    )
    .await?;

    Ok(Json(reply).into_response())
}
