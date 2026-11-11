//! Route table, mounted under `/api`. Keeping it in one place makes it obvious
//! what the public surface is — every route here is a contract someone else
//! may depend on.

pub mod agents;

use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// `GET /api/activity` — how many agents played and how many missions ran.
#[allow(clippy::unused_async)]
async fn activity(State(state): State<Arc<AppState>>) -> Response {
    match state.activity.current().await {
        Some(activity) => Json(json!({
            "window_hours": activity.window_hours,
            "agents": activity.agents,
            "missions": activity.missions,
        }))
        .into_response(),
        None => Json(json!({
            "window_hours": 24,
            "agents": 0,
            "missions": 0,
        }))
        .into_response(),
    }
}
pub mod auth;
pub mod history;
pub mod matches;
pub mod settings;
pub mod status;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};

use crate::AppState;
use crate::error::AppError;

pub fn api() -> Router<Arc<AppState>> {
    Router::new()
        // Frozen contract: plain text player count, polled by the Discord bot.
        .route("/status", get(status::status))
        // Liveness for the deploy health check and the container runtime.
        .route("/health", get(status::health))
        // Server-sent events: one-way live feed for the frontend.
        .route("/live", get(status::live))
        // Newest completed missions, for the public archive.
        .route("/matches/recent", get(matches::recent))
        // One match in full: scoreboard, and the PvP rating context.
        .route("/matches/{instance_id}", get(matches::detail))
        // Public agent directory, profiles, rating history and leaderboards.
        .route("/agents", get(agents::directory))
        .route("/agents/{user_id}", get(agents::profile))
        .route("/agents/{user_id}/matches", get(agents::matches))
        .route("/agents/{user_id}/history", get(agents::history))
        .route("/leaderboard", get(agents::leaderboard))
        // Hourly peak concurrent players, for the activity chart.
        .route("/online-history", get(history::series))
        // Recent play for the homepage hero: agents and missions over the
        // narrowest window that has any activity.
        .route("/activity", get(activity))
        // Sign-in with in-game credentials.
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        // The signed-in player's own data.
        .route("/me/matches", get(auth::matches))
        .route("/me/summary", get(auth::summary))
        // The player's own settings: read here, writes forwarded to the
        // control server, which pushes the live change to the game client.
        .route(
            "/me/settings",
            get(settings::get).post(settings::set_preference),
        )
        .route("/me/settings/dlc", post(settings::set_dlc))
        // Unknown paths under the API prefix are a JSON 404 — they must never
        // fall through to the file server and answer a typo with index.html.
        .fallback(|| async { AppError::NotFound("endpoint".to_string()) })
}
