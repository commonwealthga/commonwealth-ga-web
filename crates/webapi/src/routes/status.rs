use std::convert::Infallible;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

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

/// `GET /api/players` — the same reading as `/api/status`, as JSON.
///
/// The text endpoint stays exactly as it is (the bot prints it verbatim). This is
/// for the site itself: it renders the number into the page server-side, so the
/// count is in the HTML a crawler receives rather than appearing only once the
/// browser has opened a live stream. The shape mirrors the SSE payload so the
/// client needs no second model.
pub async fn players(State(state): State<Arc<AppState>>) -> Response {
    let payload = match state.status.current().await {
        Ok(status) => StatusPayload::from_status(&status),
        Err(err) => {
            tracing::warn!(error = %err, "players query failed");
            // Same degradation as the text endpoint: "down" is not "zero".
            StatusPayload::unavailable()
        }
    };

    // Short cache: a crawl or a burst of readers must not become a stampede on
    // the game database, and a count twenty seconds old is still a live count.
    (
        [
            (header::CONTENT_TYPE, "application/json; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=20"),
        ],
        Json(json!({
            "ok": payload.ok,
            "players": payload.players,
            "text": payload.text,
            "control_server": payload.control_server,
            "ts": payload.ts,
        })),
    )
        .into_response()
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

    // Forward updates through a channel so the stream has an end: an SSE
    // response never finishes on its own, which is what made shutdown wait
    // forever for a browser tab. Closing the channel is how this stream hangs up.
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let mut updates = state.events.subscribe();
    let mut stopping = state.shutdown.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stopping.recv() => break,
                received = updates.recv() => match received {
                    Ok(json) => {
                        let event = Event::default().event("status").data(json);
                        // A closed receiver means the client disconnected.
                        if tx.send(Ok(event)).await.is_err() {
                            break;
                        }
                    }
                    // A slow client missing ticks is fine: the next tick is 15s
                    // away and carries the whole state.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
            }
        }
    });

    let stream = tokio_stream::once(Ok(initial_event)).chain(ReceiverStream::new(rx));

    Sse::new(stream).keep_alive(KeepAlive::default())
}
