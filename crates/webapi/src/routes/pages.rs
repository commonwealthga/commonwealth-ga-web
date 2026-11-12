//! The crawlable HTML: `/news`, the post pages, and `/sitemap.xml`.
//!
//! These are mounted at the root rather than under `/api` because they are pages,
//! not data. Anything under `/news/` that is not a post (the submission form, the
//! moderator queue) is a prerendered SvelteKit page, so `/news/{*rest}` falls
//! through to the build output for non-numeric paths instead of answering 404.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::AppState;
use crate::error::{ApiResult, AppError};
use crate::news::{Status, access, pages, store};
use crate::routes::auth::current_session;
use crate::sitemap;

/// `GET /news` — the public feed, server-rendered.
pub async fn feed(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;

    let posts = store::feed(site, 50, 0).await.map_err(internal)?;
    let verdict = match &viewer.session {
        None => Err(access::Denial::Anonymous),
        Some(_) => access::may_post(state.cfg.news_posting_mode, viewer.roles),
    };
    let hint = pages::PostingHint {
        mode: state.cfg.news_posting_mode.as_str(),
        signed_in: viewer.session.is_some(),
        may_post: verdict.is_ok(),
        reason: verdict.err().map(|denial| denial.message().to_string()),
    };

    let chip = chip_text(&state).await;
    let chrome = pages::Chrome {
        signed_in_as: viewer.session.as_ref().map(|s| s.username.as_str()),
        status: chip.as_deref(),
    };
    let html = pages::feed(
        &posts,
        &hint,
        &chrome,
        &state.stylesheets,
        &state.cfg.site_url,
    );
    Ok(html_response(html))
}

/// `GET /news/{id}-{slug}` — one post as a page.
///
/// The id is what identifies the post; a mismatched slug is a redirect to the
/// canonical path rather than a 404, because a title edit should not break a
/// link someone shared.
pub async fn post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rest): Path<String>,
) -> ApiResult<Response> {
    let Some(id) = leading_id(&rest) else {
        // `/news/submit`, `/news/queue` and anything else non-numeric belongs to
        // the prerendered frontend.
        return match prerendered(&state, &format!("news/{rest}")).await {
            Some(response) => Ok(response),
            None => Err(AppError::NotFound("page".to_string())),
        };
    };

    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;

    let Some(post) = store::by_id(site, id).await.map_err(internal)? else {
        return Err(AppError::NotFound("post".to_string()));
    };
    if post.status == Status::Rejected {
        return Err(AppError::NotFound("post".to_string()));
    }

    let is_author = viewer
        .session
        .as_ref()
        .is_some_and(|session| session.user_id == post.author_id);
    let pending = post.status == Status::Pending;
    if pending && !is_author && !viewer.roles.moderator {
        return Err(AppError::NotFound("post".to_string()));
    }

    // Canonical slug, so one post has exactly one address in a search index.
    if rest != format!("{}-{}", post.id, post.slug) {
        return Ok((
            StatusCode::MOVED_PERMANENTLY,
            [(header::LOCATION, post.path())],
        )
            .into_response());
    }

    let reacted = match viewer.session.as_ref() {
        Some(session) => store::has_reacted(site, post.id, session.user_id)
            .await
            .map_err(internal)?,
        None => false,
    };

    let chip = chip_text(&state).await;
    let chrome = pages::Chrome {
        signed_in_as: viewer.session.as_ref().map(|s| s.username.as_str()),
        status: chip.as_deref(),
    };
    let html = pages::post_page(
        &post,
        reacted,
        pending,
        &chrome,
        &state.stylesheets,
        &state.cfg.site_url,
    );
    Ok(html_response(html))
}

/// `GET /sitemap.xml` — static pages plus every published post.
pub async fn sitemap_xml(State(state): State<Arc<AppState>>) -> ApiResult<Response> {
    // A missing site database should not take the sitemap down entirely: the
    // static pages are still worth crawling.
    let posts = match site_pool(&state) {
        Ok(site) => store::all_approved(site).await.unwrap_or_else(|error| {
            tracing::warn!(%error, "sitemap: could not read posts");
            Vec::new()
        }),
        Err(_) => Vec::new(),
    };
    let xml = sitemap::render(&state.cfg.site_url, &posts);
    Ok((
        [
            (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=600"),
        ],
        xml,
    )
        .into_response())
}

/// What the header chip says, from the same service `/api/status` reads. The
/// Svelte chip updates over SSE; this one is correct as of the request.
async fn chip_text(state: &AppState) -> Option<String> {
    let status = state.status.current().await.ok()?;
    if status.control_server == Some(false) {
        return Some("server down".to_string());
    }
    Some(match status.players {
        Some(1) => "1 agent in-game".to_string(),
        Some(players) => format!("{players} agents in-game"),
        // An operator override replaced the count.
        None => status.text,
    })
}

fn html_response(html: String) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            // News changes when a moderator publishes, not per request.
            (header::CACHE_CONTROL, "no-cache"),
        ],
        html,
    )
        .into_response()
}

/// The `12` of `12-some-slug`.
fn leading_id(rest: &str) -> Option<i64> {
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok().filter(|id| *id > 0)
}

/// Serve a page from the SvelteKit build, for paths the router intercepted but
/// that belong to the frontend.
async fn prerendered(state: &AppState, relative: &str) -> Option<Response> {
    let path = state.cfg.static_dir.join(format!("{relative}.html"));
    let html = tokio::fs::read_to_string(&path).await.ok()?;
    Some(html_response(html))
}

fn site_pool(state: &AppState) -> ApiResult<&sqlx::SqlitePool> {
    state
        .site_pool
        .as_ref()
        .ok_or_else(|| AppError::Unavailable("site database".to_string()))
}

fn internal(error: sqlx::Error) -> AppError {
    AppError::Internal(anyhow::Error::new(error))
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

/// `GET /api/news/posting-rules` — what the submission form needs to render,
/// including the reason it cannot be used.
pub async fn posting_rules(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let viewer = viewer(&state, &headers).await?;
    let verdict = match &viewer.session {
        None => Err(access::Denial::Anonymous),
        Some(_) => access::may_post(state.cfg.news_posting_mode, viewer.roles),
    };
    Ok(axum::Json(json!({
        "mode": state.cfg.news_posting_mode.as_str(),
        "signed_in": viewer.session.is_some(),
        "moderator": viewer.roles.moderator,
        "may_post": verdict.is_ok(),
        "reason": verdict.err().map(|denial| denial.message()),
        "kinds": crate::news::Kind::ALL
            .iter()
            .map(|kind| json!({
                "value": kind.as_str(),
                "label": kind.label(),
                "requires_link": kind.requires_link(),
                "requires_body": kind.requires_body(),
            }))
            .collect::<Vec<_>>(),
        "limits": {
            "title_max": crate::news::TITLE_MAX,
            "summary_max": crate::news::SUMMARY_MAX,
            "body_max": crate::news::BODY_MAX,
            "per_day": crate::news::RATE_LIMIT,
        },
        "formatting_help": access::FORMATTING_HELP,
    }))
    .into_response())
}
