//! `GET /db-export/{token}` — the redacted database, for third parties.
//!
//! The cards registry depends on this: it folds the archive into its own
//! statistics and needs the whole match history, not a page of API responses.
//! The previous site had the same route with the same idea (a token in the path,
//! a redacted copy, served as a file).
//!
//! What is served is the *published snapshot*, not a fresh copy per request.
//! Building one takes a second of disk I/O against the game's database; doing
//! that per download meant a lock and a queue. The snapshotter already produces
//! exactly this artefact on a timer, redacted the same way, so this is a file
//! send.
//!
//! Fail-closed in two directions: no token configured means the route does not
//! exist, and no snapshot on disk means 404 rather than an error — a missing
//! file is an operator problem, not something to explain to a stranger.

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::AppState;

/// Path of the published snapshot, when one is configured.
fn snapshot_path(state: &AppState) -> Option<PathBuf> {
    state.cfg.snapshot_db.clone()
}

/// Constant-time-ish comparison: length first, then a byte-wise fold, so a
/// wrong token cannot be narrowed down by timing.
fn token_matches(candidate: &str, expected: &str) -> bool {
    let candidate = candidate.as_bytes();
    let expected = expected.as_bytes();
    if candidate.len() != expected.len() {
        return false;
    }
    candidate
        .iter()
        .zip(expected)
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// `/db-export` and `/db-export/` — no token at all.
pub async fn no_token() -> StatusCode {
    StatusCode::NOT_FOUND
}

pub async fn download(State(state): State<Arc<AppState>>, Path(token): Path<String>) -> Response {
    let Some(expected) = state
        .cfg
        .db_export_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        // Not configured: indistinguishable from "no such page".
        return StatusCode::NOT_FOUND.into_response();
    };

    if !token_matches(&token, expected) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(path) = snapshot_path(&state) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let Ok(file) = tokio::fs::File::open(&path).await else {
        tracing::warn!(path = %path.display(), "db export requested but no snapshot is published");
        return StatusCode::NOT_FOUND.into_response();
    };

    let size = file.metadata().await.map(|meta| meta.len()).unwrap_or(0);
    let stream = tokio_util::io::ReaderStream::new(file);

    let mut response = Body::from_stream(stream).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.sqlite3"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"server-redacted.db\""),
    );
    // The file changes under the same URL, so never let a proxy cache it.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(&size.to_string()) {
        headers.insert(header::CONTENT_LENGTH, value);
    }

    tracing::info!(bytes = size, "db export served");
    response
}

#[cfg(test)]
mod tests {
    use super::token_matches;

    #[test]
    fn only_the_exact_token_passes() {
        assert!(token_matches("abc123", "abc123"));
        assert!(!token_matches("abc124", "abc123"));
        assert!(!token_matches("abc12", "abc123"));
        assert!(!token_matches("", "abc123"));
    }
}
