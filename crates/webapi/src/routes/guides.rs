//! Guides and builds API: the two lists, submissions, moderation, likes and the
//! image upload.
//!
//! Moderation is the news feed's, in every sense that matters: the same
//! `news_moderator` role decides, the queue is the same kind of queue, and the
//! rollback is the same — reject is a soft delete, unpublish returns an entry to
//! the queue carrying a note the author can see. Nothing here can grant a role;
//! the dashboard owns that.
//!
//! The image route is the one piece of this file that handles untrusted bytes,
//! and it hands them straight to [`crate::guides::images`]: nothing uploaded is
//! ever stored or served, only the re-encoded derivative.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::auth::now_unix;
use crate::error::{ApiResult, AppError};
use crate::guides::images;
use crate::guides::store::Filter;
use crate::guides::{self, Category, Class, Mode, Sort, Tag, store};
use crate::news::access;
use crate::routes::auth::current_session;

/// Page size when the client does not ask, and the most it may ask for. Ten fits
/// a tile grid two rows deep on a desktop before scrolling starts.
const FEED_DEFAULT: i64 = 12;
const FEED_MAX: i64 = 48;

#[derive(Debug, Deserialize)]
pub struct Page {
    category: Option<String>,
    tag: Option<String>,
    class: Option<String>,
    mode: Option<String>,
    q: Option<String>,
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitBody {
    category: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    class: String,
    #[serde(default)]
    modes: Vec<String>,
    #[serde(default)]
    discord: String,
    #[serde(default)]
    image_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ThumbBody {
    active: bool,
}

#[derive(Debug, Deserialize)]
pub struct NoteBody {
    #[serde(default)]
    note: String,
}

/// One entry as the API serves it. `body_markdown` is present only for the author
/// and for moderators — everyone else gets rendered HTML, which is what the page
/// shows.
#[derive(Debug, Serialize)]
pub struct EntryView {
    pub id: i64,
    pub category: &'static str,
    pub category_label: &'static str,
    pub title: String,
    pub slug: String,
    pub path: String,
    pub summary: String,
    pub body_html: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_markdown: Option<String>,
    pub tags: Vec<&'static str>,
    pub tag_labels: Vec<&'static str>,
    pub build_class: Option<&'static str>,
    pub class_label: Option<&'static str>,
    pub modes: Vec<&'static str>,
    pub mode_labels: Vec<&'static str>,
    pub discord_url: Option<String>,
    /// Set for builds: `/images/<hash>.webp`, the sanitiser's output.
    pub image_url: Option<String>,
    /// The stored image's id, so an edit that does not replace the screenshot
    /// can send the one it already has.
    pub image_id: Option<i64>,
    pub author: String,
    pub created_at: i64,
    pub thumbs_up: i64,
    pub reacted: bool,
    pub status: &'static str,
    /// Only its author, only while pending — decided here so the page never
    /// compares user ids.
    pub can_edit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderation_note: Option<String>,
}

impl EntryView {
    fn public(entry: &store::Entry, reacted: bool) -> Self {
        Self::build(entry, reacted, false)
    }

    fn detailed(entry: &store::Entry, reacted: bool, with_note: bool) -> Self {
        let mut view = Self::build(entry, reacted, true);
        if !with_note {
            view.moderation_note = None;
        }
        view
    }

    fn build(entry: &store::Entry, reacted: bool, with_markdown: bool) -> Self {
        Self {
            id: entry.id,
            category: entry.category.as_str(),
            category_label: entry.category.label(),
            title: entry.title.clone(),
            slug: entry.slug.clone(),
            path: entry.path(),
            summary: entry.summary.clone(),
            body_html: entry.body_html.clone(),
            body_markdown: with_markdown.then(|| entry.body_markdown.clone()),
            tags: entry.tags.iter().map(|tag| tag.as_str()).collect(),
            tag_labels: entry.tags.iter().map(|tag| tag.label()).collect(),
            build_class: entry.build_class.map(|class| class.as_str()),
            class_label: entry.build_class.map(|class| class.label()),
            modes: entry.modes.iter().map(|mode| mode.as_str()).collect(),
            mode_labels: entry.modes.iter().map(|mode| mode.label()).collect(),
            discord_url: entry.discord_url.clone(),
            image_url: entry
                .image_hash
                .as_ref()
                .map(|hash| format!("/images/{hash}.webp")),
            image_id: entry.image_id,
            author: entry.author_name.clone(),
            created_at: entry.created_at,
            thumbs_up: entry.thumbs_up,
            reacted,
            status: entry.status,
            can_edit: false,
            moderation_note: entry.moderation_note.clone(),
        }
    }
}

struct Viewer {
    session: Option<crate::auth::Session>,
    roles: access::Roles,
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

fn internal(error: sqlx::Error) -> AppError {
    AppError::Internal(anyhow::Error::new(error))
}

fn category_of(page_category: Option<&str>) -> Category {
    page_category
        .and_then(Category::parse)
        .unwrap_or(Category::Guide)
}

/// How many submissions an account may still make today, and when the oldest one
/// expires. The form, the list page and the refusal all read this, so they cannot
/// disagree.
#[derive(Debug, Serialize)]
pub struct Allowance {
    limit: i64,
    used: i64,
    remaining: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    resets_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resets_at_clock: Option<String>,
}

async fn allowance(site: &sqlx::SqlitePool, user_id: i64, now: i64) -> ApiResult<Allowance> {
    let since = now - guides::RATE_WINDOW_SECS;
    let used = store::submissions_since(site, user_id, since)
        .await
        .map_err(internal)?;
    Ok(Allowance {
        limit: guides::RATE_LIMIT,
        used,
        remaining: (guides::RATE_LIMIT - used).max(0),
        resets_at: (used >= guides::RATE_LIMIT).then(|| now + guides::RATE_WINDOW_SECS),
        resets_at_clock: None,
    })
}

#[derive(Debug, Serialize)]
struct Posting {
    mode: &'static str,
    signed_in: bool,
    may_post: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowance: Option<Allowance>,
}

async fn posting(state: &AppState, site: &sqlx::SqlitePool, viewer: &Viewer) -> ApiResult<Posting> {
    let mode = state.cfg.news_posting_mode;
    let verdict = match &viewer.session {
        None => Err(access::Denial::Anonymous),
        Some(_) => access::may_post(mode, viewer.roles),
    };
    // Shown before anything is written, and it names the limit and the reset
    // rather than sending the author to a moderator who cannot help.
    let (may_post, reason, allowance) = match (&viewer.session, verdict) {
        (_, Err(denial)) => (false, Some(denial.message()), None),
        (Some(session), Ok(())) => {
            let allowance = allowance(site, session.user_id, now_unix()).await?;
            if allowance.remaining == 0 {
                (
                    false,
                    Some("You have used today's submissions."),
                    Some(allowance),
                )
            } else {
                (true, None, Some(allowance))
            }
        }
        (None, Ok(())) => (false, Some(access::Denial::Anonymous.message()), None),
    };

    Ok(Posting {
        mode: mode.as_str(),
        signed_in: viewer.session.is_some(),
        may_post,
        reason,
        allowance,
    })
}

/// `GET /api/guides` — one category's approved entries, filtered and sorted.
pub async fn feed(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let category = category_of(page.category.as_deref());
    let limit = page.limit.unwrap_or(FEED_DEFAULT).clamp(1, FEED_MAX);
    let offset = page.offset.unwrap_or(0).max(0);

    let filter = Filter {
        tag: page.tag.as_deref().and_then(Tag::parse),
        class: page.class.as_deref().and_then(Class::parse),
        mode: page.mode.as_deref().and_then(Mode::parse),
        query: page.q.clone().unwrap_or_default(),
        sort: page.sort.as_deref().and_then(Sort::parse),
    };

    let entries = store::feed(site, category, &filter, limit, offset)
        .await
        .map_err(internal)?;
    let total = store::count_feed(site, category, &filter)
        .await
        .map_err(internal)?;
    let reacted = reacted_ids(site, &viewer, &entries).await?;

    // Only the first page carries the queue and the reader's own pending work: a
    // later page of the endless scroll is entries and nothing else.
    let queue = if offset == 0 {
        if viewer.roles.moderator {
            store::pending(site)
                .await
                .map_err(internal)?
                .into_iter()
                .filter(|entry| entry.category == category)
                .collect()
        } else if let Some(session) = viewer.session.as_ref() {
            store::pending_by_author(site, session.user_id)
                .await
                .map_err(internal)?
                .into_iter()
                .filter(|entry| entry.category == category)
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let items: Vec<EntryView> = entries
        .iter()
        .map(|entry| EntryView::public(entry, reacted.contains(&entry.id)))
        .collect();

    let owner = viewer.session.as_ref().map(|session| session.user_id);
    let queue_items: Vec<EntryView> = queue
        .iter()
        .map(|entry| {
            let mut view = EntryView::detailed(
                entry,
                reacted.contains(&entry.id),
                viewer.roles.moderator || owner == Some(entry.author_id),
            );
            view.can_edit = entry.status == "pending" && owner == Some(entry.author_id);
            view
        })
        .collect();

    Ok(Json(serde_json::json!({
        "entries": items,
        "pending": queue_items,
        "total": total,
        "offset": offset,
        "moderator": viewer.roles.moderator,
        "posting": posting(&state, site, &viewer).await?,
        "filter": {
            "category": category.as_str(),
            "tag": filter.tag.map(|tag| tag.as_str()),
            "class": filter.class.map(|class| class.as_str()),
            "mode": filter.mode.map(|mode| mode.as_str()),
            "q": filter.query,
            "sort": filter.sort.unwrap_or(Sort::Helpful).as_str(),
        },
    }))
    .into_response())
}

/// Which of these entries this viewer has thumbed, in one query.
async fn reacted_ids(
    site: &sqlx::SqlitePool,
    viewer: &Viewer,
    entries: &[store::Entry],
) -> ApiResult<std::collections::HashSet<i64>> {
    let Some(session) = viewer.session.as_ref() else {
        return Ok(std::collections::HashSet::new());
    };
    let mut reacted = std::collections::HashSet::new();
    for entry in entries {
        if store::has_reacted(site, entry.id, session.user_id)
            .await
            .map_err(internal)?
        {
            reacted.insert(entry.id);
        }
    }
    Ok(reacted)
}

/// `GET /api/guides/options` — everything the submission form and the filters
/// need, so the choices are defined once, in Rust.
pub async fn options(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;

    Ok(Json(serde_json::json!({
        "categories": Category::ALL
            .iter()
            .map(|category| serde_json::json!({
                "value": category.as_str(),
                "label": category.label(),
                "plural": category.plural(),
                "path": category.path(),
                "requires_image": category.requires_image(),
                "uses_tags": category.uses_tags(),
            }))
            .collect::<Vec<_>>(),
        "tags": Tag::ALL
            .iter()
            .map(|tag| serde_json::json!({ "value": tag.as_str(), "label": tag.label() }))
            .collect::<Vec<_>>(),
        "classes": Class::ALL
            .iter()
            .map(|class| serde_json::json!({ "value": class.as_str(), "label": class.label() }))
            .collect::<Vec<_>>(),
        "modes": Mode::ALL
            .iter()
            .map(|mode| serde_json::json!({ "value": mode.as_str(), "label": mode.label() }))
            .collect::<Vec<_>>(),
        "limits": {
            "title_min": guides::TITLE_MIN,
            "title_max": guides::TITLE_MAX,
            "summary_max": guides::SUMMARY_MAX,
            "body_max": guides::BODY_MAX,
            "description_max": guides::DESCRIPTION_MAX,
            "per_day": guides::RATE_LIMIT,
            "max_upload_bytes": images::MAX_UPLOAD_BYTES,
            "max_upload_width": images::MAX_WIDTH,
        },
        "posting": posting(&state, site, &viewer).await?,
        "formatting_help": access::FORMATTING_HELP,
    }))
    .into_response())
}

/// `GET /api/guides/{id}` — one entry.
pub async fn detail(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;

    let Some(entry) = store::by_id(site, id).await.map_err(internal)? else {
        return Err(AppError::NotFound("that entry".to_string()));
    };

    let owner = viewer.session.as_ref().map(|session| session.user_id);
    let is_author = owner == Some(entry.author_id);
    // An unapproved entry is visible to its author and to moderators, not to a
    // passer-by who guessed the id.
    if entry.status != "approved" && !viewer.roles.moderator && !is_author {
        return Err(AppError::NotFound("that entry".to_string()));
    }

    let reacted = match owner {
        Some(user_id) => store::has_reacted(site, id, user_id)
            .await
            .map_err(internal)?,
        None => false,
    };
    let mut view = EntryView::detailed(&entry, reacted, viewer.roles.moderator || is_author);
    view.can_edit = entry.status == "pending" && is_author;

    Ok(Json(serde_json::json!({
        "entry": view,
        "moderator": viewer.roles.moderator,
        "archived": entry.status == "rejected",
    }))
    .into_response())
}

/// `POST /api/guides` — submit an entry for review.
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
    let used = store::submissions_since(site, session.user_id, now - guides::RATE_WINDOW_SECS)
        .await
        .map_err(internal)?;
    if used >= guides::RATE_LIMIT {
        return Err(AppError::RateLimited(format!(
            "You have used today's {} submissions. You can post again in {}.",
            guides::RATE_LIMIT,
            guides::clock_utc(now + guides::RATE_WINDOW_SECS)
        )));
    }

    let entry = guides::validate(
        &body.category,
        &body.title,
        &body.summary,
        &body.body,
        &body.tags,
        &body.class,
        &body.modes,
        &body.discord,
        body.image_id,
    )
    .map_err(|error| AppError::BadRequest(error.message().to_string()))?;

    if let Some(image_id) = entry.image_id {
        let owner = store::image_owner(site, image_id).await.map_err(internal)?;
        if owner != Some(session.user_id) {
            return Err(AppError::BadRequest(
                "That image is not one of your uploads.".to_string(),
            ));
        }
    }

    let id = store::create(site, session.user_id, &session.username, &entry, now)
        .await
        .map_err(internal)?;

    Ok(Json(serde_json::json!({
        "id": id,
        "status": "pending",
        "path": entry.category.path(),
    }))
    .into_response())
}

/// `PATCH /api/guides/{id}` — the author fixing a pending entry.
pub async fn update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<SubmitBody>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let session = viewer.require_session()?;

    let Some(existing) = store::by_id(site, id).await.map_err(internal)? else {
        return Err(AppError::NotFound("that entry".to_string()));
    };
    if existing.author_id != session.user_id || existing.status != "pending" {
        return Err(AppError::Forbidden(
            "that entry is not yours to edit, or it is no longer pending".to_string(),
        ));
    }

    let entry = guides::validate(
        &body.category,
        &body.title,
        &body.summary,
        &body.body,
        &body.tags,
        &body.class,
        &body.modes,
        &body.discord,
        body.image_id,
    )
    .map_err(|error| AppError::BadRequest(error.message().to_string()))?;

    if let Some(image_id) = entry.image_id {
        let owner = store::image_owner(site, image_id).await.map_err(internal)?;
        if owner != Some(session.user_id) {
            return Err(AppError::BadRequest(
                "That image is not one of your uploads.".to_string(),
            ));
        }
    }

    let updated = store::update_pending(site, id, session.user_id, &entry)
        .await
        .map_err(internal)?;
    if !updated {
        return Err(AppError::Forbidden(
            "that entry is no longer pending".to_string(),
        ));
    }

    Ok(Json(serde_json::json!({ "id": id, "status": "pending" })).into_response())
}

/// `POST /api/guides/{id}/thumb` — one account's thumbs-up, on or off.
pub async fn thumb(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<ThumbBody>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let session = viewer.require_session()?;

    let Some(entry) = store::by_id(site, id).await.map_err(internal)? else {
        return Err(AppError::NotFound("that entry".to_string()));
    };
    if entry.status != "approved" {
        return Err(AppError::BadRequest(
            "that entry is not published".to_string(),
        ));
    }

    let count = store::set_reaction(site, id, session.user_id, body.active, now_unix())
        .await
        .map_err(internal)?;

    Ok(Json(serde_json::json!({ "thumbs_up": count, "reacted": body.active })).into_response())
}

/// `POST /api/guides/images` — a screenshot, sanitised into a stored derivative.
///
/// The request body *is* the image; there is no multipart parser in the path. The
/// declared content type is not consulted — the bytes decide what they are — and
/// what comes back is an id pointing at a re-encoded file, never the upload.
pub async fn upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let session = viewer.require_session()?;
    access::may_post(state.cfg.news_posting_mode, viewer.roles)
        .map_err(|denial| AppError::Forbidden(denial.message().to_string()))?;

    let now = now_unix();
    let recent = store::images_since(site, session.user_id, now - images::UPLOAD_WINDOW_SECS)
        .await
        .map_err(internal)?;
    if recent >= images::UPLOAD_LIMIT {
        return Err(AppError::RateLimited(
            "Too many image uploads today — try again tomorrow.".to_string(),
        ));
    }

    // Decoding is CPU-bound and pure Rust; keep it off the async workers.
    let sanitised = tokio::task::spawn_blocking(move || images::sanitise(&bytes))
        .await
        .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?
        .map_err(|error| AppError::BadRequest(error.message().to_string()))?;

    images::save(&state.cfg.images_dir, &sanitised)
        .await
        .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;

    let id = store::create_image(
        site,
        &sanitised.hash,
        sanitised.width,
        sanitised.height,
        sanitised.bytes.len(),
        session.user_id,
        now,
    )
    .await
    .map_err(internal)?;

    Ok(Json(serde_json::json!({
        "image_id": id,
        "url": format!("/images/{}.webp", sanitised.hash),
        "width": sanitised.width,
        "height": sanitised.height,
        "bytes": sanitised.bytes.len(),
    }))
    .into_response())
}

/// `POST /api/guides/{id}/approve`
pub async fn approve(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let session = viewer.moderator()?;

    let Some(entry) = store::by_id(site, id).await.map_err(internal)? else {
        return Err(AppError::NotFound("that entry".to_string()));
    };
    if entry.status == "rejected" {
        return Err(AppError::BadRequest(
            "a rejected entry stays rejected; the author can submit a new one".to_string(),
        ));
    }

    let now = now_unix();
    store::approve(site, id, session.user_id, &session.username, now)
        .await
        .map_err(internal)?;

    // The feed says so too: "X posted a guide", linking at the page. Written by
    // the entry's author and already approved, because the entry is.
    let summary = match entry.category {
        Category::Guide => format!("A new guide from {}.", entry.author_name),
        Category::Build => format!(
            "A new {} build from {}{}.",
            entry
                .build_class
                .map(|class| class.label())
                .unwrap_or("skill tree"),
            entry.author_name,
            if entry.modes.is_empty() {
                String::new()
            } else {
                format!(
                    " for {}",
                    entry
                        .modes
                        .iter()
                        .map(|mode| mode.label())
                        .collect::<Vec<_>>()
                        .join(" and ")
                )
            }
        ),
    };
    let title = format!(
        "{} has posted a {}: {}",
        entry.author_name,
        entry.category.label(),
        entry.title
    );
    crate::news::store::announce_guide(
        site,
        entry.id,
        entry.author_id,
        &entry.author_name,
        &title,
        &entry.path(),
        &summary,
        entry.category.as_str(),
        entry.build_class.map(|class| class.as_str()),
        entry
            .image_hash
            .as_ref()
            .map(|hash| format!("/images/{hash}.webp"))
            .as_deref(),
        now,
    )
    .await
    .map_err(internal)?;

    Ok(Json(serde_json::json!({ "id": id, "status": "approved" })).into_response())
}

/// `POST /api/guides/{id}/reject` — a soft delete, with a required reason.
pub async fn reject(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<NoteBody>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let session = viewer.moderator()?;
    let note = store::validate_note(&body.note)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;

    store::reject(
        site,
        id,
        session.user_id,
        &session.username,
        &note,
        now_unix(),
    )
    .await
    .map_err(internal)?;
    crate::news::store::retract_guide(site, id)
        .await
        .map_err(internal)?;

    Ok(Json(serde_json::json!({ "id": id, "status": "rejected" })).into_response())
}

/// `POST /api/guides/{id}/unapprove` — back to the queue, with what to fix.
pub async fn unapprove(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<NoteBody>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    let session = viewer.moderator()?;
    let note = store::validate_note(&body.note)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;

    store::unapprove(
        site,
        id,
        session.user_id,
        &session.username,
        &note,
        now_unix(),
    )
    .await
    .map_err(internal)?;
    crate::news::store::retract_guide(site, id)
        .await
        .map_err(internal)?;

    Ok(Json(serde_json::json!({ "id": id, "status": "pending" })).into_response())
}

/// `POST /api/guides/{id}/note` — request a change without deciding.
pub async fn note(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<NoteBody>,
) -> ApiResult<Response> {
    let site = site_pool(&state)?;
    let viewer = viewer(&state, &headers).await?;
    viewer.moderator()?;
    let note = store::validate_note(&body.note)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;

    store::annotate(site, id, &note).await.map_err(internal)?;
    Ok(Json(serde_json::json!({ "id": id, "status": "pending" })).into_response())
}

/// `GET /images/{name}` — a stored derivative, and nothing else.
///
/// The name must be one this process wrote: a hash and `.webp`. Anything else
/// never reaches the filesystem, so a traversal attempt is a 404 rather than a
/// file read.
pub async fn serve_image(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<Response> {
    if !images::name_is_valid(&name) {
        return Err(AppError::NotFound("that image".to_string()));
    }
    let path = state.cfg.images_dir.join(&name);
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(AppError::NotFound("that image".to_string()));
        }
        Err(error) => return Err(AppError::Internal(anyhow::Error::new(error))),
    };

    Ok((
        [
            (header::CONTENT_TYPE, "image/webp"),
            // Content-addressed, so the name changes when the bytes do.
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            // The file is an image this process re-encoded. Say so, and stop a
            // browser from second-guessing the type.
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response())
}
