use std::convert::Infallible;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::AppState;
use crate::status::StatusPayload;

/// `GET /api/status` — plain text, e.g. `3 players in-game`.
///
/// **This shape is a public contract.** The Discord bot polls it and displays
/// the body verbatim. Do not change the path, the content type, or the
/// wording without coordinating.
pub async fn status(State(state): State<Arc<AppState>>) -> Response {
    match state.status.current().await {
        Ok(status) => plain_text(StatusCode::OK, status.text),
        Err(err) => {
            // Degrade, never 500 with a stack trace, and never an empty body:
            // the bot has to be able to tell "down" apart from "zero players".
            tracing::warn!(error = %err, "status query failed");
            plain_text(StatusCode::SERVICE_UNAVAILABLE, "server status unavailable")
        }
    }
}

fn plain_text(status: StatusCode, body: impl Into<String>) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body.into(),
    )
        .into_response()
}

/// `GET /api/health` — JSON liveness with a real probe of the live DB.
pub async fn health(State(state): State<Arc<AppState>>) -> Response {
    let live_db_ok = match &state.live_pool {
        None => false,
        Some(pool) => sqlx::query_scalar::<_, i64>("SELECT 1")
            .fetch_one(pool)
            .await
            .is_ok(),
    };

    let status = if live_db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(json!({
            "ok": live_db_ok,
            "live_db": live_db_ok,
            "snapshot_db": state.snapshot_available().await,
            "site_db": state.site_pool.is_some(),
        })),
    )
        .into_response()
}

/// `GET /api/live` — server-sent events carrying `StatusPayload` JSON.
///
/// The current value is sent immediately so a client never renders an empty
/// state, then every broadcast tick after that. One process-wide ticker feeds
/// all subscribers, so a thousand open tabs cost exactly one query.
pub async fn live(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let initial = match state.status.current().await {
        Ok(status) => StatusPayload::from_status(&status),
        Err(_) => StatusPayload::unavailable(),
    };

    let initial_event = Event::default()
        .event("status")
        .data(serde_json::to_string(&initial).unwrap_or_else(|_| "{}".to_string()));

    let broadcast = BroadcastStream::new(state.events.subscribe())
        .filter_map(|message| message.ok())
        .map(|json| Ok(Event::default().event("status").data(json)));

    let stream = tokio_stream::once(Ok(initial_event)).chain(broadcast);

    Sse::new(stream).keep_alive(KeepAlive::default())
}
