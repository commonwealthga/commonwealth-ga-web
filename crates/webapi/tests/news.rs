//! News feed API tests.
//!
//! The rules these pin down are the ones that would be expensive to get wrong in
//! production: nothing is public before approval, rejection removes everything
//! including the author's own view, only a moderator can decide, and a body is
//! never served as markup it did not earn.

use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tower::ServiceExt;
use webapi::config::Config;
use webapi::news::{PostingMode, access, store};
use webapi::{AppState, app, auth, db};

struct Harness {
    dir: TempDir,
    site: SqlitePool,
    live_path: PathBuf,
}

impl Harness {
    async fn new() -> Self {
        let dir = TempDir::new().expect("temp dir");
        let site_path = dir.path().join("site.db");
        let site = db::open_read_write(&site_path, 1).await.expect("site db");
        auth::migrate(&site).await.expect("sessions table");
        store::migrate(&site).await.expect("news tables");
        Self {
            site,
            live_path: dir.path().join("server.db"),
            dir,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Write roles into a game database, then hand back a read-only pool to it —
    /// the same posture the service has in production.
    async fn live_with_roles(&self, grants: &[(i64, &str)]) -> SqlitePool {
        let writer = db::open_read_write(&self.live_path, 1)
            .await
            .expect("live db");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS ga_user_roles (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 user_id INTEGER NOT NULL,
                 role TEXT NOT NULL,
                 granted_at INTEGER NOT NULL DEFAULT 0,
                 UNIQUE(user_id, role))",
        )
        .execute(&writer)
        .await
        .expect("roles schema");
        for (user_id, role) in grants {
            sqlx::query("INSERT OR IGNORE INTO ga_user_roles (user_id, role) VALUES (?, ?)")
                .bind(user_id)
                .bind(role)
                .execute(&writer)
                .await
                .expect("grant");
        }
        writer.close().await;
        db::open_read_only(&self.live_path, 1)
            .await
            .expect("read-only pool")
    }

    fn router(&self, live: Option<SqlitePool>, mode: PostingMode) -> Router {
        let cfg = Config {
            bind: "127.0.0.1:0".parse().expect("addr"),
            live_db: Some(self.live_path.clone()),
            snapshot_db: None,
            site_db: Some(self.path("site.db")),
            files_dir: self.path("static"),
            images_dir: self.path("images"),
            status_ttl: Duration::from_secs(60),
            live_tick: Duration::from_secs(15),
            match_ttl: Duration::from_secs(30),
            history_ttl: Duration::from_secs(3600),
            status_override: None,
            control_api_url: None,
            control_api_token: None,
            db_export_token: None,
            news_posting_mode: mode,
            site_url: "https://example.test".to_string(),
        };
        app(AppState::new(cfg, live, None, Some(self.site.clone())))
    }

    async fn session(&self, user_id: i64, username: &str) -> String {
        auth::create_session(&self.site, user_id, username)
            .await
            .expect("session")
    }
}

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 512 * 1024)
        .await
        .expect("body");
    (status, String::from_utf8_lossy(&body).to_string())
}

/// One entry of a given kind, for the tests that only care about the kind.
fn of_kind(kind: &str, title: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": kind,
        "title": title,
        "body": "Something worth saying.",
        "links": [],
    })
}

fn submit_request(cookie: Option<&str>, payload: serde_json::Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/news")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, format!("cw_session={cookie}"));
    }
    builder
        .body(Body::from(payload.to_string()))
        .expect("request")
}

fn action_request(cookie: &str, uri: &str, payload: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, format!("cw_session={cookie}"))
        .body(Body::from(payload.to_string()))
        .expect("request")
}

fn get(cookie: Option<&str>, uri: &str) -> Request<Body> {
    let mut builder = Request::builder().uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, format!("cw_session={cookie}"));
    }
    builder.body(Body::empty()).expect("request")
}

fn json(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or_else(|error| panic!("not JSON ({error}): {body}"))
}

fn post_payload() -> serde_json::Value {
    serde_json::json!({
        "kind": "article",
        "title": "How to solo the Surfside bunker",
        "body": "## Prep\n\nBring **two** medkits and _do not_ rush the lift.",
    })
}

#[tokio::test]
async fn a_submission_is_invisible_until_a_moderator_publishes_it() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);

    let author = harness.session(7, "Agent07").await;
    let moderator = harness.session(1, "ModOne").await;

    let (status, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let created = json(&body);
    assert_eq!(created["status"], "pending");
    let id = created["id"].as_i64().expect("id");

    // Not in the public feed, and its page does not exist for a stranger.
    let (status, body) = send(&app, get(None, "/api/news")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json(&body)["posts"].as_array().expect("posts").len(), 0);

    let (status, _) = send(&app, get(None, &format!("/api/news/{id}"))).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a pending post is not public"
    );

    // The author can see their own pending post, and knows it is pending.
    let (status, body) = send(&app, get(Some(&author), &format!("/api/news/{id}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json(&body)["status"], "pending");

    // In the moderator queue, with the source Markdown for review.
    let (status, body) = send(&app, get(Some(&moderator), "/api/news/queue")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let queue = json(&body);
    let queued = queue["posts"].as_array().expect("queue");
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0]["id"], id);
    assert!(
        queued[0]["body_markdown"]
            .as_str()
            .unwrap_or_default()
            .contains("## Prep")
    );

    // Publish.
    let (status, body) = send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/approve"),
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = send(&app, get(None, "/api/news")).await;
    assert_eq!(status, StatusCode::OK);
    let feed = json(&body);
    let posts = feed["posts"].as_array().expect("posts");
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"], "How to solo the Surfside bunker");
    assert_eq!(posts[0]["kind_label"], "Article");
    assert_eq!(posts[0]["status"], "approved");
    assert!(
        posts[0]["body_markdown"].is_null(),
        "the public view carries rendered HTML only"
    );
    assert!(
        posts[0]["body_html"]
            .as_str()
            .unwrap_or_default()
            .contains("<h2>Prep</h2>")
    );
    assert_eq!(
        posts[0]["path"],
        format!("/news/{id}-how-to-solo-the-surfside-bunker")
    );

    // And the queue is empty again.
    let (status, body) = send(&app, get(Some(&moderator), "/api/news/queue")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json(&body)["posts"].as_array().expect("queue").len(), 0);
}

#[tokio::test]
async fn rejection_removes_the_post_from_every_view_but_keeps_the_reason() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);

    let author = harness.session(7, "Agent07").await;
    let moderator = harness.session(1, "ModOne").await;

    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let id = json(&body)["id"].as_i64().expect("id");

    // A rejection without a note is refused: the note is the whole record.
    let (status, body) = send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/reject"),
            serde_json::json!({ "note": "   " }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, _) = send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/reject"),
            serde_json::json!({ "note": "Advertising a paid carry service" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Gone from the feed, the queue, the author's list and its own page.
    let (_, feed) = send(&app, get(None, "/api/news")).await;
    assert_eq!(json(&feed)["posts"].as_array().expect("posts").len(), 0);
    let (_, queue) = send(&app, get(Some(&moderator), "/api/news/queue")).await;
    assert_eq!(json(&queue)["posts"].as_array().expect("queue").len(), 0);
    let (_, mine) = send(&app, get(Some(&author), "/api/news/mine")).await;
    assert_eq!(json(&mine)["posts"].as_array().expect("mine").len(), 0);
    let (status, _) = send(&app, get(Some(&author), &format!("/api/news/{id}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "not even the author sees it");
    let (status, _) = send(&app, get(Some(&moderator), &format!("/api/news/{id}"))).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a moderator can still open it to review the content"
    );

    // The row, the body and the note survive.
    let post = store::by_id(&harness.site, id)
        .await
        .expect("lookup")
        .expect("kept");
    assert_eq!(post.status, webapi::news::Status::Rejected);
    assert_eq!(
        post.moderation_note.as_deref(),
        Some("Advertising a paid carry service")
    );
    assert!(post.body_markdown.contains("## Prep"));

    let trail = store::audit_for(&harness.site, id).await.expect("audit");
    assert_eq!(trail.len(), 1);
    assert_eq!(trail[0].action, "reject");
    assert_eq!(trail[0].actor_name, "ModOne");
    assert_eq!(trail[0].note, "Advertising a paid carry service");
}

#[tokio::test]
async fn only_a_moderator_can_decide() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);

    let author = harness.session(7, "Agent07").await;
    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let id = json(&body)["id"].as_i64().expect("id");

    for uri in ["/api/news/queue"] {
        let (status, _) = send(&app, get(Some(&author), uri)).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{uri} must be moderator-only"
        );
    }
    for action in ["approve", "reject"] {
        let (status, _) = send(
            &app,
            action_request(
                &author,
                &format!("/api/news/{id}/{action}"),
                serde_json::json!({ "note": "I like my own post" }),
            ),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "an author must not be able to {action} their own post"
        );
    }

    // Still pending: nobody's mind was changed by the attempts.
    let post = store::by_id(&harness.site, id)
        .await
        .expect("lookup")
        .expect("present");
    assert_eq!(post.status, webapi::news::Status::Pending);
}

#[tokio::test]
async fn submitting_needs_an_account() {
    let harness = Harness::new().await;
    let app = harness.router(None, PostingMode::Open);

    let (status, _) = send(&app, submit_request(None, post_payload())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = send(&app, get(None, "/api/news")).await;
    assert_eq!(status, StatusCode::OK);
    let feed = json(&body);
    assert_eq!(feed["posting"]["may_post"], false);
    assert_eq!(feed["posting"]["signed_in"], false);
    assert!(
        feed["posting"]["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("Sign in")
    );
}

#[tokio::test]
async fn a_restricted_account_is_refused_and_told_why() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(7, access::ROLE_RESTRICTED)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);
    let author = harness.session(7, "Agent07").await;

    let (status, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body.contains("not permitted"), "{body}");

    let (_, feed) = send(&app, get(Some(&author), "/api/news")).await;
    assert_eq!(json(&feed)["posting"]["may_post"], false);
}

#[tokio::test]
async fn allowlisted_mode_admits_only_the_allowlist() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(7, access::ROLE_ALLOWLISTED), (8, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Allowlisted);

    let ordinary = harness.session(9, "Agent09").await;
    let (status, body) = send(&app, submit_request(Some(&ordinary), post_payload())).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (_, feed) = send(&app, get(Some(&ordinary), "/api/news")).await;
    assert_eq!(json(&feed)["posting"]["mode"], "allowlisted");

    let allowlisted = harness.session(7, "Agent07").await;
    let (status, body) = send(&app, submit_request(Some(&allowlisted), post_payload())).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn submissions_are_rate_limited_per_account() {
    let harness = Harness::new().await;
    let app = harness.router(None, PostingMode::Open);
    let author = harness.session(7, "Agent07").await;

    for i in 0..webapi::news::RATE_LIMIT {
        let mut payload = post_payload();
        payload["title"] = serde_json::json!(format!("Post number {i}"));
        let (status, body) = send(&app, submit_request(Some(&author), payload)).await;
        assert_eq!(status, StatusCode::CREATED, "submission {i}: {body}");
    }
    let mut payload = post_payload();
    payload["title"] = serde_json::json!("One too many");
    let (status, body) = send(&app, submit_request(Some(&author), payload)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert!(body.contains("used today's 5 posts"), "{body}");
    // No "ask a moderator": moderators cannot change an allowance.
    assert!(!body.contains("moderator"), "{body}");
    assert!(body.contains("post again after"), "says when: {body}");
}

#[tokio::test]
async fn the_form_is_told_what_is_left_before_anything_is_written() {
    let harness = Harness::new().await;
    let app = harness.router(None, PostingMode::Open);
    let author = harness.session(7, "Agent07").await;

    let (status, body) = send(&app, get(Some(&author), "/api/news/posting-rules")).await;
    assert_eq!(status, StatusCode::OK);
    let rules = json(&body);
    assert_eq!(rules["allowance"]["limit"], webapi::news::RATE_LIMIT);
    assert_eq!(rules["allowance"]["remaining"], webapi::news::RATE_LIMIT);
    assert_eq!(rules["allowance"]["used"], 0);
    assert!(
        rules["allowance"]["resets_at"].is_null(),
        "a full allowance does not expire"
    );

    // Two submissions later, the form knows there are three left.
    for i in 0..2 {
        let mut payload = post_payload();
        payload["title"] = serde_json::json!(format!("Post {i}"));
        send(&app, submit_request(Some(&author), payload)).await;
    }
    let (_, body) = send(&app, get(Some(&author), "/api/news/posting-rules")).await;
    let rules = json(&body);
    assert_eq!(rules["allowance"]["used"], 2);
    assert_eq!(
        rules["allowance"]["remaining"],
        webapi::news::RATE_LIMIT - 2
    );
    assert!(
        rules["allowance"]["resets_at"].as_i64().is_some(),
        "and when the first one expires: {rules}"
    );
}

#[tokio::test]
async fn a_rejected_post_does_not_cost_allowance() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);
    let author = harness.session(7, "Agent07").await;
    let moderator = harness.session(1, "ModOne").await;

    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let id = json(&body)["id"].as_i64().expect("id");
    send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/reject"),
            serde_json::json!({ "note": "no" }),
        ),
    )
    .await;

    let (_, body) = send(&app, get(Some(&author), "/api/news/posting-rules")).await;
    let rules = json(&body);
    assert_eq!(
        rules["allowance"]["remaining"],
        webapi::news::RATE_LIMIT,
        "a moderation decision is not the author's fault: {rules}"
    );
}

#[tokio::test]
async fn a_body_never_arrives_as_the_markup_it_contains() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);
    let author = harness.session(7, "Agent07").await;
    let moderator = harness.session(1, "ModOne").await;

    let mut payload = post_payload();
    payload["body"] = serde_json::json!(
        "<script>alert('x')</script>\n\n[click](javascript:alert(1))\n\n![img](https://evil.example/p.png)"
    );
    let (status, body) = send(&app, submit_request(Some(&author), payload)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = json(&body)["id"].as_i64().expect("id");

    let (_, body) = send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/approve"),
            serde_json::json!({}),
        ),
    )
    .await;
    assert!(body.contains("approved"), "{body}");

    let (_, body) = send(&app, get(None, &format!("/api/news/{id}"))).await;
    let html = json(&body)["body_html"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(!html.contains("<script"), "{html}");
    assert!(!html.contains("javascript:"), "{html}");
    assert!(!html.contains("<img"), "{html}");
    assert!(
        html.contains("&lt;script&gt;"),
        "shown, not interpreted: {html}"
    );
}

#[tokio::test]
async fn a_video_entry_is_links_and_nothing_else() {
    let harness = Harness::new().await;
    let app = harness.router(None, PostingMode::Open);
    let author = harness.session(7, "Agent07").await;

    // No title, no body: the form does not ask for them.
    let empty = serde_json::json!({ "kind": "video" });
    let (status, body) = send(&app, submit_request(Some(&author), empty)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("YouTube"), "{body}");

    let wrong_site = serde_json::json!({
        "kind": "video",
        "links": ["https://example.org/not-youtube"]
    });
    let (status, body) = send(&app, submit_request(Some(&author), wrong_site)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let links = serde_json::json!({
        "kind": "video",
        "title": "ignored",
        "body": "ignored",
        "links": [
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/watch?v=aaaaaaaaaaa"
        ]
    });
    let (status, body) = send(&app, submit_request(Some(&author), links)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = json(&body)["id"].as_i64().expect("id");

    // Stored as two embeds, with no invented title.
    let post = store::by_id(&harness.site, id)
        .await
        .expect("lookup")
        .expect("present");
    assert_eq!(
        post.videos,
        vec!["dQw4w9WgXcQ".to_string(), "aaaaaaaaaaa".to_string()]
    );
    assert!(post.title.is_empty(), "a title is never invented");
    assert!(post.body_html.is_empty());
}

#[tokio::test]
async fn too_many_videos_is_refused() {
    let harness = Harness::new().await;
    let app = harness.router(None, PostingMode::Open);
    let author = harness.session(7, "Agent07").await;

    let links: Vec<String> = (0..=webapi::news::MAX_VIDEOS)
        .map(|i| format!("https://youtu.be/video{i:04}"))
        .collect();
    let payload = serde_json::json!({ "kind": "video", "links": links });
    let (status, body) = send(&app, submit_request(Some(&author), payload)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn an_author_can_edit_their_own_entry_while_it_waits() {
    let harness = Harness::new().await;
    let app = harness.router(None, PostingMode::Open);
    let author = harness.session(7, "Agent07").await;

    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let id = json(&body)["id"].as_i64().expect("id");

    let edited = serde_json::json!({
        "kind": "article",
        "title": "Surfside bunker, solo — corrected",
        "body": "## Prep\n\nBring **three** medkits now."
    });
    let request = Request::builder()
        .method("PATCH")
        .uri(format!("/api/news/{id}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, format!("cw_session={author}"))
        .body(Body::from(edited.to_string()))
        .expect("request");
    let (status, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let post = store::by_id(&harness.site, id)
        .await
        .expect("lookup")
        .expect("present");
    assert_eq!(post.title, "Surfside bunker, solo — corrected");
    assert_eq!(
        post.status,
        webapi::news::Status::Pending,
        "still unpublished"
    );
    assert!(
        post.body_html.contains("<strong>three</strong>"),
        "re-rendered"
    );

    // The edit is on the record, attributed to the author.
    let trail = store::audit_for(&harness.site, id).await.expect("audit");
    assert_eq!(
        trail.last().map(|entry| entry.action.as_str()),
        Some("edit")
    );

    // And it is still not public.
    let (status, _) = send(&app, get(None, &format!("/api/news/{id}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_published_entry_cannot_be_edited_by_its_author() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);
    let author = harness.session(7, "Agent07").await;
    let moderator = harness.session(1, "ModOne").await;

    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let id = json(&body)["id"].as_i64().expect("id");
    send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/approve"),
            serde_json::json!({}),
        ),
    )
    .await;

    let request = Request::builder()
        .method("PATCH")
        .uri(format!("/api/news/{id}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, format!("cw_session={author}"))
        .body(Body::from(post_payload().to_string()))
        .expect("request");
    let (status, body) = send(&app, request).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "publishing edited text without review would make the queue advisory: {body}"
    );
}

#[tokio::test]
async fn nobody_edits_somebody_elses_entry() {
    let harness = Harness::new().await;
    let app = harness.router(None, PostingMode::Open);
    let author = harness.session(7, "Agent07").await;
    let stranger = harness.session(8, "Agent08").await;

    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let id = json(&body)["id"].as_i64().expect("id");

    let request = Request::builder()
        .method("PATCH")
        .uri(format!("/api/news/{id}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, format!("cw_session={stranger}"))
        .body(Body::from(post_payload().to_string()))
        .expect("request");
    let (status, _) = send(&app, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_moderator_can_unpublish_a_post_so_its_author_can_fix_it() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);
    let author = harness.session(7, "Agent07").await;
    let moderator = harness.session(1, "ModOne").await;

    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let id = json(&body)["id"].as_i64().expect("id");
    send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/approve"),
            serde_json::json!({}),
        ),
    )
    .await;

    // A note is required: it is the only thing telling the author what to fix.
    let (status, body) = send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/unapprove"),
            serde_json::json!({ "note": "  " }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, body) = send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/unapprove"),
            serde_json::json!({ "note": "Typo in the first line: it says two medkits, should be three." }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Out of the public feed, back in the author's queue, with the reason.
    let (_, feed) = send(&app, get(None, "/api/news")).await;
    assert_eq!(json(&feed)["posts"].as_array().expect("posts").len(), 0);
    let (_, mine) = send(&app, get(Some(&author), "/api/news/mine")).await;
    let mine = json(&mine);
    assert_eq!(mine["posts"][0]["status"], "pending");
    assert_eq!(
        mine["posts"][0]["moderation_note"],
        "Typo in the first line: it says two medkits, should be three."
    );

    // And the author can now edit it — which is the whole point.
    let request = Request::builder()
        .method("PATCH")
        .uri(format!("/api/news/{id}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, format!("cw_session={author}"))
        .body(Body::from(
            serde_json::json!({
                "kind": "article",
                "title": "How to solo the Surfside bunker",
                "body": "## Prep\n\nBring **three** medkits."
            })
            .to_string(),
        ))
        .expect("request");
    let (status, body) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let post = store::by_id(&harness.site, id)
        .await
        .expect("lookup")
        .expect("kept");
    assert!(post.body_markdown.contains("three"));

    // The trail records both decisions and the edit.
    let trail = store::audit_for(&harness.site, id).await.expect("audit");
    let actions: Vec<&str> = trail.iter().map(|entry| entry.action.as_str()).collect();
    assert_eq!(actions, vec!["approve", "unapprove", "edit"]);
}

#[tokio::test]
async fn a_note_can_be_added_without_publishing_anything() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);
    let author = harness.session(7, "Agent07").await;
    let moderator = harness.session(1, "ModOne").await;

    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let id = json(&body)["id"].as_i64().expect("id");

    // A note with nothing in it is not a note.
    let (status, body) = send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/note"),
            serde_json::json!({ "note": "  " }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, body) = send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/note"),
            serde_json::json!({ "note": "The date is wrong: it was the 19th, not the 12th." }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Nothing was published, nothing was rejected: it is still in the queue.
    let post = store::by_id(&harness.site, id)
        .await
        .expect("lookup")
        .expect("kept");
    assert_eq!(post.status, webapi::news::Status::Pending);
    assert_eq!(
        post.moderation_note.as_deref(),
        Some("The date is wrong: it was the 19th, not the 12th.")
    );
    let (_, feed) = send(&app, get(None, "/api/news")).await;
    assert_eq!(json(&feed)["posts"].as_array().expect("posts").len(), 0);
    let (_, queue) = send(&app, get(Some(&moderator), "/api/news/queue")).await;
    assert_eq!(json(&queue)["posts"].as_array().expect("queue").len(), 1);

    // The author sees the reason and can act on it.
    let (_, mine) = send(&app, get(Some(&author), "/api/news/mine")).await;
    let mine = json(&mine);
    assert_eq!(
        mine["posts"][0]["moderation_note"],
        "The date is wrong: it was the 19th, not the 12th."
    );

    // Editing keeps the note until the entry is approved.
    let request = Request::builder()
        .method("PATCH")
        .uri(format!("/api/news/{id}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, format!("cw_session={author}"))
        .body(Body::from(
            serde_json::json!({
                "kind": "article",
                "title": "How to solo the Surfside bunker",
                "body": "## Prep\n\nBring **two** medkits."
            })
            .to_string(),
        ))
        .expect("request");
    send(&app, request).await;

    // Approving it clears the note: a published entry must not still be asking
    // for a change.
    let (status, _) = send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/approve"),
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let post = store::by_id(&harness.site, id)
        .await
        .expect("lookup")
        .expect("kept");
    assert_eq!(post.status, webapi::news::Status::Approved);
    assert_eq!(
        post.moderation_note, None,
        "the note went with the approval"
    );

    // The trail has the note, the edit and the approval.
    let trail = store::audit_for(&harness.site, id).await.expect("audit");
    let actions: Vec<&str> = trail.iter().map(|entry| entry.action.as_str()).collect();
    assert_eq!(actions, vec!["note", "edit", "approve"]);
}

#[tokio::test]
async fn only_a_moderator_can_add_a_note_and_only_to_a_pending_entry() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);
    let author = harness.session(7, "Agent07").await;
    let moderator = harness.session(1, "ModOne").await;

    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let id = json(&body)["id"].as_i64().expect("id");

    let (status, _) = send(
        &app,
        action_request(
            &author,
            &format!("/api/news/{id}/note"),
            serde_json::json!({ "note": "annotating my own post" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Approved entries are handled by unpublishing, not by annotating.
    send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/approve"),
            serde_json::json!({}),
        ),
    )
    .await;
    let (status, _) = send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/note"),
            serde_json::json!({ "note": "too late" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_a_moderator_can_unpublish() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);
    let author = harness.session(7, "Agent07").await;
    let moderator = harness.session(1, "ModOne").await;

    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let id = json(&body)["id"].as_i64().expect("id");
    send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/approve"),
            serde_json::json!({}),
        ),
    )
    .await;

    let (status, _) = send(
        &app,
        action_request(
            &author,
            &format!("/api/news/{id}/unapprove"),
            serde_json::json!({ "note": "I want to change it" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A post already in the queue is not "published", so there is nothing to undo.
    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let pending_id = json(&body)["id"].as_i64().expect("id");
    let (status, _) = send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{pending_id}/unapprove"),
            serde_json::json!({ "note": "nope" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn thumbs_up_toggles_and_counts_per_account() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);

    let author = harness.session(7, "Agent07").await;
    let reader = harness.session(8, "Agent08").await;
    let moderator = harness.session(1, "ModOne").await;

    let (_, body) = send(&app, submit_request(Some(&author), post_payload())).await;
    let id = json(&body)["id"].as_i64().expect("id");
    send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/approve"),
            serde_json::json!({}),
        ),
    )
    .await;

    let uri = format!("/api/news/{id}/thumb");
    let (status, body) = send(
        &app,
        action_request(&reader, &uri, serde_json::json!({ "active": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(json(&body)["thumbs_up"], 1);

    // Clicking again is idempotent rather than +1.
    let (_, body) = send(
        &app,
        action_request(&reader, &uri, serde_json::json!({ "active": true })),
    )
    .await;
    assert_eq!(json(&body)["thumbs_up"], 1);

    let (_, body) = send(
        &app,
        action_request(&author, &uri, serde_json::json!({ "active": true })),
    )
    .await;
    assert_eq!(json(&body)["thumbs_up"], 2);

    // Taking it back.
    let (_, body) = send(
        &app,
        action_request(&reader, &uri, serde_json::json!({ "active": false })),
    )
    .await;
    assert_eq!(json(&body)["thumbs_up"], 1);

    // The feed tells a signed-in reader what they already reacted to.
    let (_, body) = send(&app, get(Some(&author), "/api/news")).await;
    let posts = json(&body);
    assert_eq!(posts["posts"][0]["thumbs_up"], 1);
    assert_eq!(posts["posts"][0]["reacted"], true);

    // A post that does not exist cannot be reacted to.
    let (status, _) = send(
        &app,
        action_request(
            &reader,
            "/api/news/4242/thumb",
            serde_json::json!({ "active": true }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_preview_matches_what_will_be_stored() {
    let harness = Harness::new().await;
    let live = harness
        .live_with_roles(&[(1, access::ROLE_MODERATOR)])
        .await;
    let app = harness.router(Some(live), PostingMode::Open);
    let author = harness.session(7, "Agent07").await;
    let moderator = harness.session(1, "ModOne").await;

    let body = "## Heading\n\n**bold** and [a link](https://example.org)";
    let request = Request::builder()
        .method("POST")
        .uri("/api/news/preview")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::json!({ "body": body }).to_string()))
        .expect("request");
    let (status, preview) = send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
    let preview_html = json(&preview)["html"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    let mut payload = post_payload();
    payload["body"] = serde_json::json!(body);
    let (_, created) = send(&app, submit_request(Some(&author), payload)).await;
    let id = json(&created)["id"].as_i64().expect("id");
    send(
        &app,
        action_request(
            &moderator,
            &format!("/api/news/{id}/approve"),
            serde_json::json!({}),
        ),
    )
    .await;

    let (_, served) = send(&app, get(None, &format!("/api/news/{id}"))).await;
    assert_eq!(
        json(&served)["body_html"].as_str().unwrap_or_default(),
        preview_html,
        "the preview and the published post must render identically"
    );
}

#[tokio::test]
async fn the_feed_can_be_narrowed_to_one_kind() {
    let harness = Harness::new().await;
    let moderator = harness.session(70, "mod").await;
    let app = harness.router(
        Some(harness.live_with_roles(&[(70, "news_moderator")]).await),
        PostingMode::Open,
    );
    let author = harness.session(71, "author").await;

    for (kind, title) in [
        ("news", "A news item"),
        ("update", "An update about suits"),
        ("article", "A long article"),
    ] {
        let (status, body) = send(&app, submit_request(Some(&author), of_kind(kind, title))).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let id: i64 = serde_json::from_str::<serde_json::Value>(&body).expect("json")["id"]
            .as_i64()
            .expect("id");
        send(
            &app,
            Request::builder()
                .method("POST")
                .uri(format!("/api/news/{id}/approve"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, format!("cw_session={moderator}"))
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await;
    }

    // The whole feed holds all three ...
    let (_, all) = send(&app, get(None, "/api/news")).await;
    assert!(all.contains("\"total\":3"), "{all}");

    // ... and the filtered one holds only its kind, including in the total.
    let (_, updates) = send(&app, get(None, "/api/news?kind=update")).await;
    assert!(updates.contains("An update about suits"), "{updates}");
    assert!(!updates.contains("A long article"), "{updates}");
    assert!(updates.contains("\"total\":1"), "{updates}");
    assert!(updates.contains("\"kind\":\"update\""), "{updates}");

    // A stale or invented kind shows the whole feed rather than an error page.
    let (status, unknown) = send(&app, get(None, "/api/news?kind=blog")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(unknown.contains("\"total\":3"), "{unknown}");
}
