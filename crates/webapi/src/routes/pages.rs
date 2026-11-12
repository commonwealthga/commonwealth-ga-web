//! The two things the news pages ask the API for directly.
//!
//! The pages themselves are SvelteKit routes now — server-rendered, so a crawler
//! reads them, and client-side navigated, so crossing into the feed is not a page
//! load. That is why there is no HTML rendering left in this crate: one renderer,
//! one set of templates, one copy of the site chrome.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::AppState;
use crate::auth::now_unix;
use crate::error::{ApiResult, AppError};
use crate::news::access;
use crate::routes::auth::current_session;
use crate::sitemap;

/// `GET /api/news/posting-rules` — what the submission form needs to render:
/// the per-kind rules, the limits, and what is left of today's allowance.
pub async fn posting_rules(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let viewer = viewer(&state, &headers).await?;
    let verdict = match &viewer.session {
        None => Err(access::Denial::Anonymous),
        Some(_) => access::may_post(state.cfg.news_posting_mode, viewer.roles),
    };
    let allowance = match (&viewer.session, site_pool(&state).ok()) {
        (Some(session), Some(site)) => {
            Some(crate::routes::news::allowance(site, session.user_id, now_unix()).await?)
        }
        _ => None,
    };

    Ok(axum::Json(json!({
        "mode": state.cfg.news_posting_mode.as_str(),
        "allowance": allowance.map(|allowance| json!({
            "limit": allowance.limit,
            "used": allowance.used,
            "remaining": allowance.remaining,
            "resets_at": allowance.resets_at,
            "resets_at_clock": allowance.resets_at.map(crate::news::clock_utc),
        })),
        "signed_in": viewer.session.is_some(),
        "moderator": viewer.roles.moderator,
        "may_post": verdict.is_ok(),
        "reason": verdict.err().map(|denial| denial.message()),
        "kinds": crate::news::Kind::ALL
            .iter()
            .map(|kind| json!({
                "value": kind.as_str(),
                "label": kind.label(),
                "phrase": kind.phrase(),
                "requires_link": kind.requires_link(),
                "requires_body": kind.requires_body(),
                "has_editor": kind.has_editor(),
            }))
            .collect::<Vec<_>>(),
        "limits": {
            "title_max": crate::news::TITLE_MAX,
            "body_max": crate::news::BODY_MAX,
            "per_day": crate::news::RATE_LIMIT,
            "max_videos": crate::news::MAX_VIDEOS,
        },
    }))
    .into_response())
}

/// `GET /sitemap.xml` — the site's pages, from the release that knows them.
///
/// No post URLs: every post lives on `/news`, and one URL per post would be a
/// sitemap of anchors.
pub async fn sitemap_xml(State(state): State<Arc<AppState>>) -> ApiResult<Response> {
    // A missing site database means no entry pages, not no sitemap: the static
    // pages are still true.
    // Owned paths, because the sitemap outlives the rows they came from.
    let published = match state.site_pool.as_ref() {
        Some(site) => crate::guides::store::all_approved(site)
            .await
            .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?,
        None => Vec::new(),
    };
    let paths: Vec<String> = published.iter().map(|entry| entry.path()).collect();
    let entries: Vec<sitemap::EntryRef<'_>> = paths
        .iter()
        .zip(published.iter())
        .map(|(path, entry)| sitemap::EntryRef {
            path,
            updated: entry.created_at,
        })
        .collect();
    let xml = sitemap::render(&state.cfg.site_url, &entries);
    Ok((
        [
            (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=600"),
        ],
        xml,
    )
        .into_response())
}

struct Viewer {
    session: Option<crate::auth::Session>,
    roles: access::Roles,
}

async fn viewer(state: &AppState, headers: &HeaderMap) -> ApiResult<Viewer> {
    let session = current_session(state, headers).await?;
    let roles = match &session {
        Some(session) => access::roles_of(state.live_pool.as_ref(), session.user_id).await,
        None => access::Roles::NONE,
    };
    Ok(Viewer { session, roles })
}

fn site_pool(state: &AppState) -> ApiResult<&sqlx::SqlitePool> {
    state
        .site_pool
        .as_ref()
        .ok_or_else(|| AppError::Unavailable("site database".to_string()))
}
