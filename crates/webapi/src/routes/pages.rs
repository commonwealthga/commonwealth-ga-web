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
use crate::auth::now_unix;
use crate::error::{ApiResult, AppError};
use crate::news::{Status, access, pages, store};
use crate::routes::auth::current_session;
use crate::sitemap;
use axum::extract::Query;

/// How many entries one request of the feed returns. Small enough that the
/// first paint is quick, large enough that a reader rarely waits twice.
const PAGE: i64 = 10;

/// `GET /news` — the public feed, server-rendered.
pub async fn feed(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;

    let approved = store::feed(site, PAGE, 0).await.map_err(internal)?;
    let total = store::count_approved(site).await.map_err(internal)?;

    // Pending means different things to different readers: the whole queue for a
    // moderator, and only their own submissions for anyone else. Both are read
    // here so the feed is the one place moderation happens.
    let pending = if viewer.roles.moderator {
        store::pending(site).await.map_err(internal)?
    } else if let Some(session) = viewer.session.as_ref() {
        store::pending_by_author(site, session.user_id)
            .await
            .map_err(internal)?
    } else {
        Vec::new()
    };
    let verdict = match &viewer.session {
        None => Err(access::Denial::Anonymous),
        Some(_) => access::may_post(state.cfg.news_posting_mode, viewer.roles),
    };
    // What is left of today's allowance, so the reader knows before they write
    // anything — and does not see a form they cannot use.
    let left = match viewer.session.as_ref() {
        Some(session) => {
            Some(crate::routes::news::allowance(site, session.user_id, now_unix()).await?)
        }
        None => None,
    };
    let hint = pages::PostingHint {
        mode: state.cfg.news_posting_mode.as_str(),
        signed_in: viewer.session.is_some(),
        may_post: verdict.is_ok(),
        remaining: left.map(|allowance| allowance.remaining),
        limit: left.map_or(crate::news::RATE_LIMIT, |allowance| allowance.limit),
        resets_at: left.and_then(|allowance| allowance.resets_at),
        reason: verdict.err().map(|denial| denial.message().to_string()),
    };

    let chip = chip_text(&state).await;
    let chrome = pages::Chrome {
        signed_in_as: viewer.session.as_ref().map(|s| s.username.as_str()),
        status: chip.as_deref(),
    };
    // Which entries this reader already thumbed up, in one query rather than
    // one per card.
    let mut reacted = std::collections::HashSet::new();
    if let Some(session) = viewer.session.as_ref() {
        for post in approved.iter().chain(pending.iter()) {
            if store::has_reacted(site, post.id, session.user_id)
                .await
                .map_err(internal)?
            {
                reacted.insert(post.id);
            }
        }
    }

    let data = pages::FeedData {
        approved: &approved,
        pending: &pending,
        moderator: viewer.roles.moderator,
        reacted: &reacted,
        hint,
        viewer_id: viewer.session.as_ref().map(|session| session.user_id),
        offset: 0,
        total,
    };
    let html = pages::feed(
        &data,
        &chrome,
        &crate::stylesheets(&state.cfg.static_dir),
        &state.cfg.site_url,
    );
    Ok(html_response(html))
}

/// `GET /news/{id}-{slug}` — the feed is the page now.
///
/// Entries are shown in full on `/news`, so a post has no page of its own. A
/// link to one (from Discord, or from a crawler that read an older sitemap)
/// lands on the entry itself rather than a 404: one page per post meant the
/// content lived in a place nobody was ever sent to.
pub async fn post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(rest): Path<String>,
) -> ApiResult<Response> {
    let Some(id) = leading_id(&rest) else {
        // `/news/submit` and anything else non-numeric belongs to the
        // prerendered frontend.
        return match prerendered(&state, &format!("news/{rest}")).await {
            Some(response) => Ok(response),
            None => Err(AppError::NotFound("page".to_string())),
        };
    };

    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;

    // A pending or rejected post is not an address anyone else may follow: the
    // anchor would lead to a feed without it.
    let post = store::by_id(site, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::NotFound("post".to_string()))?;
    let is_author = viewer
        .session
        .as_ref()
        .is_some_and(|session| session.user_id == post.author_id);
    let visible = match post.status {
        Status::Approved => true,
        Status::Pending => is_author || viewer.roles.moderator,
        Status::Rejected => false,
    };
    if !visible {
        return Err(AppError::NotFound("post".to_string()));
    }

    Ok((
        StatusCode::MOVED_PERMANENTLY,
        [(header::LOCATION, format!("/news#post-{id}"))],
    )
        .into_response())
}

/// `GET /news/cards?offset=&limit=` — the next batch of entries, as HTML.
///
/// Returned as the same `<li>` markup the first page uses, so there is one
/// renderer and the scroll cannot drift from what the server would have sent.
/// `X-News-More` says whether to ask again.
pub async fn cards(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(page): Query<CardsQuery>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let offset = page.offset.unwrap_or(0).max(0);
    let limit = page.limit.unwrap_or(PAGE).clamp(1, 50);

    let approved = store::feed(site, limit, offset).await.map_err(internal)?;
    let total = store::count_approved(site).await.map_err(internal)?;

    let mut reacted = std::collections::HashSet::new();
    if let Some(session) = viewer.session.as_ref() {
        for post in &approved {
            if store::has_reacted(site, post.id, session.user_id)
                .await
                .map_err(internal)?
            {
                reacted.insert(post.id);
            }
        }
    }

    let html = pages::cards(&approved, &reacted);
    let more = offset + (approved.len() as i64) < total;
    Ok((
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
            (
                header::HeaderName::from_static("x-news-more"),
                if more { "true" } else { "false" },
            ),
        ],
        html,
    )
        .into_response())
}

#[derive(Debug, serde::Deserialize)]
pub struct CardsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

/// `GET /sitemap.xml` — static pages plus every published post.
pub async fn sitemap_xml(State(state): State<Arc<AppState>>) -> ApiResult<Response> {
    // Pages only: every post is on /news, and a post URL redirects there, so
    // listing them would spend crawl budget on redirects. This also means the
    // sitemap cannot be broken by the news database.
    let xml = sitemap::render(&state.cfg.site_url);
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
            "summary_max": crate::news::SUMMARY_MAX,
            "body_max": crate::news::BODY_MAX,
            "per_day": crate::news::RATE_LIMIT,
            "max_videos": crate::news::MAX_VIDEOS,
        },
        "formatting_help": access::FORMATTING_HELP,
    }))
    .into_response())
}
