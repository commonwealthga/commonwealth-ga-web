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
            static_dir: self.path("static"),
            files_dir: self.path("static"),
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
        "kind": "guide",
        "title": "How to solo the Surfside bunker",
        "summary": "A route that works with a starter suit.",
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
    assert_eq!(posts[0]["kind_label"], "Guide");
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
    assert!(body.contains("a day"), "{body}");
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
async fn a_video_entry_needs_a_youtube_link() {
    let harness = Harness::new().await;
    let app = harness.router(None, PostingMode::Open);
    let author = harness.session(7, "Agent07").await;

    let mut payload = post_payload();
    payload["kind"] = serde_json::json!("video");
    payload["body"] = serde_json::json!("");

    let (status, body) = send(&app, submit_request(Some(&author), payload.clone())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    payload["link"] = serde_json::json!("https://example.org/not-youtube");
    let (status, body) = send(&app, submit_request(Some(&author), payload.clone())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("YouTube"), "{body}");

    payload["link"] = serde_json::json!("https://youtu.be/dQw4w9WgXcQ");
    let (status, body) = send(&app, submit_request(Some(&author), payload)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
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
