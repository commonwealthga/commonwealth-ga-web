//! One error type for the whole API.
//!
//! Every route must degrade gracefully: a broken database yields a 503 with a
//! short JSON body, never a panic, never an internal error string. Details go
//! to the log, not to the client.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::history::HistoryError;
use crate::matches::MatchError;
use crate::status::StatusError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0} is not available")]
    Unavailable(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("not signed in")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("too many attempts, try again later")]
    TooManyAttempts,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    fn status_and_message(&self) -> (StatusCode, String) {
        match self {
            AppError::Unavailable(what) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("{what} is not available"),
            ),
            AppError::NotFound(what) => (StatusCode::NOT_FOUND, format!("{what} not found")),
            AppError::BadRequest(what) => (StatusCode::BAD_REQUEST, what.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "not signed in".to_string()),
            AppError::Forbidden(what) => (StatusCode::FORBIDDEN, what.clone()),
            AppError::TooManyAttempts => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many attempts, try again later".to_string(),
            ),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal server error".to_string(),
            ),
        }
    }
}

impl From<StatusError> for AppError {
    fn from(err: StatusError) -> Self {
        match err {
            StatusError::NotConfigured => AppError::Unavailable("live game database".to_string()),
            StatusError::Query(e) => AppError::Internal(anyhow::Error::new(e)),
        }
    }
}

impl From<MatchError> for AppError {
    fn from(err: MatchError) -> Self {
        match err {
            MatchError::NotConfigured => AppError::Unavailable("live game database".to_string()),
            MatchError::Query(e) => AppError::Internal(anyhow::Error::new(e)),
        }
    }
}

impl From<crate::settings::SettingsError> for AppError {
    fn from(err: crate::settings::SettingsError) -> Self {
        match err {
            crate::settings::SettingsError::NotConfigured => {
                AppError::Unavailable("player settings".to_string())
            }
            crate::settings::SettingsError::Rejected => {
                AppError::BadRequest("the game server refused that change".to_string())
            }
            crate::settings::SettingsError::Unreachable => {
                AppError::Unavailable("the game server is not reachable".to_string())
            }
        }
    }
}

impl From<HistoryError> for AppError {
    fn from(err: HistoryError) -> Self {
        match err {
            HistoryError::NotConfigured => AppError::Unavailable("live game database".to_string()),
            HistoryError::Query(e) => AppError::Internal(anyhow::Error::new(e)),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let AppError::Internal(err) = &self {
            tracing::error!(error = ?err, "request failed");
        }
        let (status, message) = self.status_and_message();
        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, AppError>;
