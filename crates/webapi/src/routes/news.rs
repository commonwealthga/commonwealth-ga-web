//! News API: the public feed, submissions, and the moderator queue.
//!
//! Every write is gated in one place — [`crate::news::access::may_post`] for
//! submissions, [`crate::news::access::Roles::moderator`] for moderation — so
//! the rules cannot drift between the API, the pages and the Svelte client.
//! Nothing here can grant a role: the dashboard owns that.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::auth::now_unix;
use crate::error::{ApiResult, AppError};
use crate::news::{self, Status, access, markdown, store};
use crate::routes::auth::current_session;

/// Feed page size when the client does not ask, and the most it may ask for.
const FEED_DEFAULT: i64 = 20;
const FEED_MAX: i64 = 50;

/// How many submissions an account may still make today, and when the oldest
/// one in the window expires. Computed in one place so the form, the feed and
/// the refusal all say the same thing.
#[derive(Debug, Clone, Copy)]
pub struct Allowance {
    pub limit: i64,
    pub used: i64,
    pub remaining: i64,
    /// When the oldest submission in the window drops out. `None` when the
    /// allowance is untouched.
    pub resets_at: Option<i64>,
}

pub async fn allowance(site: &sqlx::SqlitePool, user_id: i64, now: i64) -> ApiResult<Allowance> {
    let since = now - news::RATE_WINDOW_SECS;
    let (used, oldest) = store::allowance_used(site, user_id, since)
        .await
        .map_err(internal)?;
    Ok(Allowance {
        limit: news::RATE_LIMIT,
        used,
        remaining: (news::RATE_LIMIT - used).max(0),
        resets_at: oldest.map(|at| at + news::RATE_WINDOW_SECS),
    })
}

#[derive(Debug, Deserialize)]
pub struct Page {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitBody {
    kind: String,
    /// Optional for a video entry, which is links and nothing else.
    #[serde(default)]
    title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    body: String,
    /// Video entries send one or more links; everything else sends at most one.
    /// `link` stays accepted so a single-link client keeps working.
    #[serde(default)]
    links: Vec<String>,
    #[serde(default)]
    link: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NoteBody {
    #[serde(default)]
    note: String,
}

#[derive(Debug, Deserialize)]
pub struct ThumbBody {
    active: bool,
}

/// A post as the API serves it. `body_markdown` is only present for the author
/// and for moderators — everyone else gets the rendered HTML, which is what the
/// page shows anyway.
#[derive(Debug, Serialize)]
pub struct PostView {
    pub id: i64,
    pub kind: &'static str,
    pub kind_label: &'static str,
    pub title: String,
    pub slug: String,
    pub path: String,
    pub summary: String,
    pub body_html: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_markdown: Option<String>,
    pub link: Option<String>,
    pub videos: Vec<String>,
    pub author: String,
    pub created_at: i64,
    pub thumbs_up: i64,
    pub reacted: bool,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderation_note: Option<String>,
}

impl PostView {
    fn public(post: &store::Post, reacted: bool) -> Self {
        Self::from_post(post, reacted, false)
    }

    /// The author's and moderator's view: source Markdown, and the note the last
    /// decision carried. The author sees the note too — when a post is
    /// unpublished for a fix, that note is the only thing saying what to fix.
    fn detailed(post: &store::Post, reacted: bool, with_note: bool) -> Self {
        let mut view = Self::from_post(post, reacted, true);
        if !with_note {
            view.moderation_note = None;
        }
        view
    }

    fn from_post(post: &store::Post, reacted: bool, with_markdown: bool) -> Self {
        Self {
            id: post.id,
            kind: post.kind.as_str(),
            kind_label: post.kind.label(),
            title: post.title.clone(),
            slug: post.slug.clone(),
            path: post.path(),
            summary: post.summary.clone(),
            body_html: post.body_html.clone(),
            body_markdown: with_markdown.then(|| post.body_markdown.clone()),
            link: post.link.clone(),
            videos: post.videos.clone(),
            author: post.author_name.clone(),
            created_at: post.created_at,
            thumbs_up: post.thumbs_up,
            reacted,
            status: post.status.as_str(),
            moderation_note: post.moderation_note.clone(),
        }
    }
}

/// What the client needs to decide whether to offer the submission form, and
/// what to say if it cannot.
#[derive(Debug, Serialize)]
struct PostingRules {
    mode: &'static str,
    signed_in: bool,
    may_post: bool,
    reason: Option<&'static str>,
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

impl Viewer {
    fn moderator(&self) -> ApiResult<&crate::auth::Session> {
        match (&self.session, self.roles.moderator) {
            (Some(session), true) => Ok(session),
            _ => Err(AppError::Forbidden(
                "only a moderator may do that".to_string(),
            )),
        }
    }

    fn require_session(&self) -> ApiResult<&crate::auth::Session> {
        self.session.as_ref().ok_or(AppError::Unauthorized)
    }
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

fn rules_for(state: &AppState, viewer: &Viewer) -> PostingRules {
    let mode = state.cfg.news_posting_mode;
    let verdict = match &viewer.session {
        None => Err(access::Denial::Anonymous),
        Some(_) => access::may_post(mode, viewer.roles),
    };
    PostingRules {
        mode: mode.as_str(),
        signed_in: viewer.session.is_some(),
        may_post: verdict.is_ok(),
        reason: verdict.err().map(access::Denial::message),
    }
}

/// `GET /api/news` — approved posts, newest first.
pub async fn feed(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let limit = page.limit.unwrap_or(FEED_DEFAULT).clamp(1, FEED_MAX);
    let offset = page.offset.unwrap_or(0).max(0);

    let posts = store::feed(site, limit, offset).await.map_err(internal)?;
    let reacted = reacted_set(site, &viewer, &posts).await?;

    let items: Vec<PostView> = posts
        .iter()
        .map(|post| PostView::public(post, reacted.contains(&post.id)))
        .collect();

    Ok(Json(serde_json::json!({
        "posts": items,
        "posting": rules_for(&state, &viewer),
    }))
    .into_response())
}

/// Which of these posts this viewer has thumbed up — one query, not one per post.
async fn reacted_set(
    site: &sqlx::SqlitePool,
    viewer: &Viewer,
    posts: &[store::Post],
) -> ApiResult<std::collections::HashSet<i64>> {
    let Some(session) = viewer.session.as_ref() else {
        return Ok(std::collections::HashSet::new());
    };
    let mut reacted = std::collections::HashSet::new();
    for post in posts {
        if store::has_reacted(site, post.id, session.user_id)
            .await
            .map_err(internal)?
        {
            reacted.insert(post.id);
        }
    }
    Ok(reacted)
}

/// `GET /api/news/{id}` — one post. Approved posts are public; a pending one is
/// visible to its author and to moderators; a rejected one only to moderators.
pub async fn detail(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;

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
        // Rejected is the soft delete: out of every view but the database.
        Status::Rejected => viewer.roles.moderator,
    };
    if !visible {
        return Err(AppError::NotFound("post".to_string()));
    }

    let reacted = match viewer.session.as_ref() {
        Some(session) => store::has_reacted(site, post.id, session.user_id)
            .await
            .map_err(internal)?,
        None => false,
    };

    let view = if post.status == Status::Approved && !viewer.roles.moderator && !is_author {
        PostView::public(&post, reacted)
    } else {
        PostView::detailed(&post, reacted, viewer.roles.moderator || is_author)
    };
    Ok(Json(view).into_response())
}

/// `POST /api/news` — submit an entry. Always lands in the queue, never on the
/// site: the response says `pending` and the author is told so.
pub async fn submit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<SubmitBody>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let session = viewer.require_session()?;

    access::may_post(state.cfg.news_posting_mode, viewer.roles)
        .map_err(|denial| AppError::Forbidden(denial.message().to_string()))?;

    let now = now_unix();
    let left = allowance(site, session.user_id, now).await?;
    if left.remaining == 0 {
        // No "ask a moderator": moderators cannot change an allowance, so
        // suggesting it only sends the reader somewhere useless.
        let when = match left.resets_at {
            Some(at) => format!(" You can post again after {}.", news::clock_utc(at)),
            None => String::new(),
        };
        return Err(AppError::RateLimited(format!(
            "You have used today's {} posts.{when}",
            left.limit
        )));
    }

    let mut links = body.links;
    if let Some(single) = body.link {
        links.push(single);
    }
    let entry = news::validate(&body.kind, &body.title, &body.summary, &body.body, &links)
        .map_err(|error| AppError::BadRequest(error.message().to_string()))?;

    let id = store::create(site, session.user_id, &session.username, &entry, now)
        .await
        .map_err(internal)?;

    tracing::info!(
        post_id = id,
        author = %session.username,
        kind = entry.kind.as_str(),
        "news submission queued for review"
    );

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "id": id,
            "path": format!("/news/{id}-{}", entry.slug),
            "status": Status::Pending.as_str(),
            "message": "Thanks — a moderator will review it before it appears on the site.",
        })),
    )
        .into_response())
}

/// `GET /api/news/queue` — the moderator queue: pending submissions.
pub async fn queue(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    viewer.moderator()?;

    let posts = store::pending(site).await.map_err(internal)?;
    let items: Vec<PostView> = posts
        .iter()
        .map(|post| PostView::detailed(post, false, true))
        .collect();
    Ok(Json(serde_json::json!({ "posts": items })).into_response())
}

/// `GET /api/news/mine` — the author's own submissions and what happened to
/// them. Rejected posts are absent: rejection is a soft delete everywhere.
pub async fn mine(State(state): State<Arc<AppState>>, headers: HeaderMap) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let session = viewer.require_session()?;

    let posts = store::by_author(site, session.user_id, FEED_MAX)
        .await
        .map_err(internal)?;
    // Their own posts, so their own notes: this is where a "please fix this"
    // lands.
    let items: Vec<PostView> = posts
        .iter()
        .map(|post| PostView::detailed(post, false, true))
        .collect();
    Ok(Json(serde_json::json!({ "posts": items })).into_response())
}

/// `POST /api/news/{id}/approve` — publish. The audit entry records who did it;
/// a note is optional here because approval is the expected outcome.
pub async fn approve(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<NoteBody>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let moderator = viewer.moderator()?;

    let note = body.note.trim();
    if note.chars().count() > store::NOTE_MAX {
        return Err(AppError::BadRequest(
            "The note is too long (500 characters at most).".to_string(),
        ));
    }
    let now = now_unix();

    let changed = store::approve(site, id, moderator.user_id, &moderator.username, now)
        .await
        .map_err(internal)?;
    if !changed {
        return Err(AppError::NotFound(
            "pending post (it may already have been reviewed)".to_string(),
        ));
    }
    store::audit(
        site,
        Some(id),
        Some(moderator.user_id),
        &moderator.username,
        "approve",
        note,
        now,
    )
    .await
    .map_err(internal)?;

    tracing::info!(post_id = id, moderator = %moderator.username, "news post approved");
    Ok(Json(serde_json::json!({ "id": id, "status": Status::Approved.as_str() })).into_response())
}

/// `POST /api/news/{id}/reject` — the soft delete. A note is mandatory: six
/// months from now the only thing that explains the decision is this string.
pub async fn reject(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<NoteBody>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let moderator = viewer.moderator()?;

    let note = store::validate_note(&body.note)
        .map_err(|message| AppError::BadRequest(message.to_string()))?;
    let now = now_unix();

    let changed = store::reject(site, id, moderator.user_id, &moderator.username, &note, now)
        .await
        .map_err(internal)?;
    if !changed {
        return Err(AppError::NotFound(
            "post that can be rejected (it may already be rejected)".to_string(),
        ));
    }
    store::audit(
        site,
        Some(id),
        Some(moderator.user_id),
        &moderator.username,
        "reject",
        &note,
        now,
    )
    .await
    .map_err(internal)?;

    tracing::info!(
        post_id = id,
        moderator = %moderator.username,
        note = %note,
        "news post rejected"
    );
    Ok(Json(serde_json::json!({ "id": id, "status": Status::Rejected.as_str() })).into_response())
}

/// `PATCH /api/news/{id}` — fix your own entry while it is still pending.
///
/// Not for approved posts: publishing edited text without review would make the
/// queue advisory. A moderator's way to allow a fix is to unapprove, which puts
/// the entry back here.
pub async fn update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<SubmitBody>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let session = viewer.require_session()?;

    let post = store::by_id(site, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::NotFound("post".to_string()))?;
    if post.author_id != session.user_id {
        return Err(AppError::Forbidden(
            "only the author can edit this post".to_string(),
        ));
    }
    if post.status != Status::Pending {
        return Err(AppError::Forbidden(
            "this post has been reviewed and can no longer be edited".to_string(),
        ));
    }

    let mut links = body.links;
    if let Some(single) = body.link {
        links.push(single);
    }
    let entry = news::validate(&body.kind, &body.title, &body.summary, &body.body, &links)
        .map_err(|error| AppError::BadRequest(error.message().to_string()))?;

    let changed = store::update_pending(site, id, session.user_id, &entry, now_unix())
        .await
        .map_err(internal)?;
    if !changed {
        return Err(AppError::NotFound(
            "own pending post (it may have just been reviewed)".to_string(),
        ));
    }
    store::audit(
        site,
        Some(id),
        Some(session.user_id),
        &session.username,
        "edit",
        "",
        now_unix(),
    )
    .await
    .map_err(internal)?;

    tracing::info!(post_id = id, author = %session.username, "news post edited while pending");
    Ok(Json(serde_json::json!({
        "id": id,
        "status": Status::Pending.as_str(),
        "path": format!("/news/{id}-{}", entry.slug),
    }))
    .into_response())
}

/// `POST /api/news/{id}/unapprove` — take a published post back for a fix.
///
/// The note is required because it is the only thing telling the author what to
/// change; without it the entry simply vanishes from the feed with no reason.
pub async fn unapprove(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<NoteBody>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let moderator = viewer.moderator()?;

    let note = store::validate_note(&body.note)
        .map_err(|message| AppError::BadRequest(message.to_string()))?;
    let now = now_unix();

    let changed = store::unapprove(site, id, moderator.user_id, &moderator.username, &note, now)
        .await
        .map_err(internal)?;
    if !changed {
        return Err(AppError::NotFound(
            "published post (it may already be unpublished)".to_string(),
        ));
    }
    store::audit(
        site,
        Some(id),
        Some(moderator.user_id),
        &moderator.username,
        "unapprove",
        &note,
        now,
    )
    .await
    .map_err(internal)?;

    tracing::info!(
        post_id = id,
        moderator = %moderator.username,
        note = %note,
        "news post unapproved for a fix"
    );
    Ok(Json(serde_json::json!({ "id": id, "status": Status::Pending.as_str() })).into_response())
}

/// `POST /api/news/{id}/thumb` — one thumbs-up per account, toggled.
pub async fn thumb(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<ThumbBody>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let session = viewer.require_session()?;

    // Reacting to something invisible would leak that it exists.
    let post = store::approved_by_id(site, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::NotFound("post".to_string()))?;

    store::set_reaction(site, post.id, session.user_id, body.active, now_unix())
        .await
        .map_err(internal)?;
    let count = store::thumb_count(site, post.id).await.map_err(internal)?;

    Ok(Json(serde_json::json!({ "thumbs_up": count, "reacted": body.active })).into_response())
}

/// `POST /api/news/preview` — render Markdown exactly as a post body will be
/// rendered, so the editor cannot show a preview the published page disagrees
/// with.
pub async fn preview(Json(body): Json<PreviewBody>) -> ApiResult<Response> {
    if body.body.chars().count() > news::BODY_MAX {
        return Err(AppError::BadRequest(
            "The body is too long (20,000 characters at most).".to_string(),
        ));
    }
    Ok(Json(serde_json::json!({ "html": markdown::to_html(&body.body) })).into_response())
}

#[derive(Debug, Deserialize)]
pub struct PreviewBody {
    #[serde(default)]
    body: String,
}
