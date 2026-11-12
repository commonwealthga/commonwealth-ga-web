//! The server-rendered pages and the sitemap.
//!
//! These exist because of search engines, so the assertions are about what a
//! crawler sees: a complete document, a canonical URL, structured data, one
//! `<h1>`, and a sitemap that lists what exists and nothing else.

use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tower::ServiceExt;
use webapi::config::Config;
use webapi::news::{PostingMode, Status, store};
use webapi::{AppState, app, auth, db};

struct Harness {
    dir: TempDir,
    site: SqlitePool,
}

impl Harness {
    async fn new() -> Self {
        let dir = TempDir::new().expect("temp dir");
        let site = db::open_read_write(&dir.path().join("site.db"), 1)
            .await
            .expect("site db");
        auth::migrate(&site).await.expect("sessions");
        store::migrate(&site).await.expect("news tables");
        Self { dir, site }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn router(&self) -> Router {
        let cfg = Config {
            bind: "127.0.0.1:0".parse().expect("addr"),
            live_db: None,
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
            site_url: "https://example.test".to_string(),
            news_posting_mode: PostingMode::Open,
        };
        app(AppState::new(cfg, None, None, Some(self.site.clone())))
    }

    /// A published post, written straight to the site database.
    async fn published(&self, title: &str, now: i64) -> i64 {
        let entry = webapi::news::validate(
            "article",
            title,
            "A summary for the snippet.",
            "## Body",
            &[],
        )
        .expect("valid");
        let id = store::create(&self.site, 7, "Agent07", &entry, now)
            .await
            .expect("create");
        store::approve(&self.site, id, 1, "ModOne", now + 60)
            .await
            .expect("approve");
        id
    }
}

async fn get(
    app: &Router,
    uri: &str,
    cookie: Option<&str>,
) -> (StatusCode, Option<String>, String) {
    let mut builder = Request::builder().uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, format!("cw_session={cookie}"));
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = axum::body::to_bytes(response.into_body(), 512 * 1024)
        .await
        .expect("body");
    (
        status,
        content_type,
        String::from_utf8_lossy(&body).to_string(),
    )
}

#[tokio::test]
async fn the_feed_is_a_complete_page_a_crawler_can_read() {
    let harness = Harness::new().await;
    let id = harness
        .published("Surfside Atoll is live", 1_764_000_000)
        .await;
    let app = harness.router();

    let (status, content_type, html) = get(&app, "/news", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("text/html; charset=utf-8"));
    assert!(html.starts_with("<!doctype html>"));
    assert!(
        html.contains("<title>News · The Commonwealth</title>"),
        "{html}"
    );
    // The entry is the page: no teaser, no link to a detail view.
    assert!(html.contains(&format!("id=\"post-{id}\"")), "{html}");
    assert!(html.contains("Surfside Atoll is live"), "{html}");
    assert!(html.contains("posted an article"), "{html}");
    assert!(
        !html.contains(&format!("href=\"/news/{id}")),
        "no detail-page link: {html}"
    );
    assert!(
        html.contains("<link rel=\"canonical\" href=\"https://example.test/news\">"),
        "{html}"
    );
    // Server-rendered content, not an empty app shell: the title is in the HTML.
    assert!(html.contains("Surfside Atoll is live"));
}

#[tokio::test]
async fn a_post_url_lands_on_the_entry_in_the_feed() {
    let harness = Harness::new().await;
    let id = harness
        .published("Surfside Atoll is live", 1_764_000_000)
        .await;
    let app = harness.router();

    for uri in [
        format!("/news/{id}-surfside-atoll-is-live"),
        // An address from before a title edit still leads somewhere sensible.
        format!("/news/{id}-a-title-from-before-an-edit"),
    ] {
        let (status, _, _) = get(&app, &uri, None).await;
        assert_eq!(status, StatusCode::MOVED_PERMANENTLY, "{uri}");
    }
}

#[tokio::test]
async fn the_entry_itself_is_rendered_in_full_on_the_feed() {
    let harness = Harness::new().await;
    let id = harness
        .published("Surfside Atoll is live", 1_764_000_000)
        .await;
    let app = harness.router();

    let (status, _, html) = get(&app, "/news", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains(&format!("id=\"post-{id}\"")),
        "the anchor: {html}"
    );
    assert!(
        html.contains("posted an article"),
        "the header line: {html}"
    );
    assert!(html.contains("Agent07"), "who posted it");
    assert!(html.contains("<h2>Body</h2>"), "the body in full: {html}");
    assert!(
        html.contains("data-thumb-post"),
        "a thumbs-up per entry: {html}"
    );
    assert!(
        !html.contains(&format!("href=\"/news/{id}")),
        "and no link to a detail page: {html}"
    );
}

#[tokio::test]
async fn pending_posts_are_still_not_public_but_redirect_their_author() {
    let harness = Harness::new().await;
    let entry = webapi::news::validate("news", "Still in review", "", "Body", &[]).expect("valid");
    let pending = store::create(&harness.site, 7, "Agent07", &entry, 1_764_000_000)
        .await
        .expect("create");
    let app = harness.router();

    let (status, _, _) = get(&app, &format!("/news/{pending}-still-in-review"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let author = auth::create_session(&harness.site, 7, "Agent07")
        .await
        .expect("session");
    let (status, _, _) = get(
        &app,
        &format!("/news/{pending}-still-in-review"),
        Some(&author),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::MOVED_PERMANENTLY,
        "their own entry is in their feed"
    );

    store::reject(&harness.site, pending, 1, "ModOne", "spam", 1_764_000_100)
        .await
        .expect("reject");
    let (status, _, _) = get(
        &app,
        &format!("/news/{pending}-still-in-review"),
        Some(&author),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a rejected post is addressable by nobody"
    );
}

#[tokio::test]
async fn unknown_news_paths_fall_through_to_the_frontend_or_404() {
    let harness = Harness::new().await;
    std::fs::create_dir_all(harness.path("static/news")).expect("dir");
    std::fs::write(
        harness.path("static/news/submit.html"),
        "<!doctype html><title>the form</title>",
    )
    .expect("file");
    let app = harness.router();

    // A prerendered page the router intercepted on its way past.
    let (status, content_type, html) = get(&app, "/news/submit", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("text/html; charset=utf-8"));
    assert!(html.contains("the form"));

    // Neither a post nor a page.
    let (status, _, _) = get(&app, "/news/nonsense", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A numeric id with no post behind it.
    let (status, _, _) = get(&app, "/news/9999-no-such-post", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_sitemap_lists_pages_not_posts() {
    let harness = Harness::new().await;
    let published = harness.published("A published post", 1_764_000_000).await;
    let app = harness.router();

    let (status, content_type, xml) = get(&app, "/sitemap.xml", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        content_type.as_deref(),
        Some("application/xml; charset=utf-8"),
        "a sitemap must be served as XML, not text/html"
    );
    assert!(xml.contains("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">"));
    assert!(
        xml.contains("<loc>https://example.test/news</loc>"),
        "{xml}"
    );
    assert!(
        !xml.contains(&format!("/news/{published}")),
        "a post URL redirects, so listing it would waste crawl budget: {xml}"
    );
}

#[tokio::test]
async fn the_sitemap_survives_without_a_site_database() {
    // Losing the writable database should cost the news URLs, not the sitemap.
    let harness = Harness::new().await;
    let mut cfg = Config {
        bind: "127.0.0.1:0".parse().expect("addr"),
        live_db: None,
        snapshot_db: None,
        site_db: None,
        static_dir: harness.path("static"),
        files_dir: harness.path("static"),
        status_ttl: Duration::from_secs(60),
        live_tick: Duration::from_secs(15),
        match_ttl: Duration::from_secs(30),
        history_ttl: Duration::from_secs(3600),
        status_override: None,
        control_api_url: None,
        control_api_token: None,
        db_export_token: None,
        site_url: "https://example.test".to_string(),
        news_posting_mode: PostingMode::Open,
    };
    cfg.site_db = None;
    let app = app(AppState::new(cfg, None, None, None));

    let (status, _, xml) = get(&app, "/sitemap.xml", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(xml.contains("<loc>https://example.test/play</loc>"));
}

#[tokio::test]
async fn a_moderator_moderates_on_the_feed_itself() {
    let harness = Harness::new().await;
    let entry = webapi::news::validate("news", "Waiting here", "", "Body", &[]).expect("valid");
    let pending = store::create(&harness.site, 7, "Agent07", &entry, 1_764_000_000)
        .await
        .expect("create");

    // A moderator grant in the game database, read-only from here.
    let live_path = harness.path("server.db");
    let writer = db::open_read_write(&live_path, 1).await.expect("live");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS ga_user_roles (
             id INTEGER PRIMARY KEY AUTOINCREMENT, user_id INTEGER NOT NULL,
             role TEXT NOT NULL, granted_at INTEGER NOT NULL DEFAULT 0,
             UNIQUE(user_id, role))",
    )
    .execute(&writer)
    .await
    .expect("schema");
    sqlx::query("INSERT INTO ga_user_roles (user_id, role) VALUES (1, 'news_moderator')")
        .execute(&writer)
        .await
        .expect("grant");
    writer.close().await;

    let live = db::open_read_only(&live_path, 1).await.expect("pool");
    let cfg = Config {
        bind: "127.0.0.1:0".parse().expect("addr"),
        live_db: Some(live_path.clone()),
        snapshot_db: None,
        site_db: Some(harness.path("site.db")),
        static_dir: harness.path("static"),
        files_dir: harness.path("static"),
        status_ttl: Duration::from_secs(60),
        live_tick: Duration::from_secs(15),
        match_ttl: Duration::from_secs(30),
        history_ttl: Duration::from_secs(3600),
        status_override: None,
        control_api_url: None,
        control_api_token: None,
        db_export_token: None,
        site_url: "https://example.test".to_string(),
        news_posting_mode: PostingMode::Open,
    };
    let app = app(AppState::new(
        cfg,
        Some(live),
        None,
        Some(harness.site.clone()),
    ));

    let moderator = auth::create_session(&harness.site, 1, "ModOne")
        .await
        .expect("session");
    let (status, _, html) = get(&app, "/news", Some(&moderator)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Waiting for review"), "{html}");
    assert!(
        html.contains("Waiting here"),
        "the pending post is on the feed"
    );
    assert!(
        html.contains(&format!("id=\"post-{pending}\"")),
        "the pending entry is on the feed as a card: {html}"
    );
    assert!(
        html.contains("data-action=\"approve\""),
        "with controls: {html}"
    );

    // A stranger sees none of it.
    let (status, _, html) = get(&app, "/news", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !html.contains("Waiting here"),
        "not published, not public: {html}"
    );
    assert!(!html.contains("data-action"));
}

#[tokio::test]
async fn an_author_sees_their_own_pending_post_on_the_feed() {
    let harness = Harness::new().await;
    let entry =
        webapi::news::validate("news", "My draft", "A summary", "Body", &[]).expect("valid");
    store::create(&harness.site, 7, "Agent07", &entry, 1_764_000_000)
        .await
        .expect("create");
    let app = harness.router();

    let author = auth::create_session(&harness.site, 7, "Agent07")
        .await
        .expect("session");
    let (status, _, html) = get(&app, "/news", Some(&author)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("My draft"), "their pending post is visible");
    assert!(html.contains("Waiting for review"), "and labelled as such");
    assert!(
        !html.contains("data-action=\"approve\""),
        "but they cannot approve it themselves: {html}"
    );

    // Someone else's feed does not show it.
    let other = auth::create_session(&harness.site, 8, "Agent08")
        .await
        .expect("session");
    let (_, _, html) = get(&app, "/news", Some(&other)).await;
    assert!(!html.contains("My draft"), "{html}");
}

#[tokio::test]
async fn a_rejected_post_is_on_nobody_s_feed() {
    let harness = Harness::new().await;
    let entry = webapi::news::validate("news", "Removed post", "", "Body", &[]).expect("valid");
    let id = store::create(&harness.site, 7, "Agent07", &entry, 1_764_000_000)
        .await
        .expect("create");
    store::reject(&harness.site, id, 1, "ModOne", "spam", 1_764_000_100)
        .await
        .expect("reject");
    let app = harness.router();

    let author = auth::create_session(&harness.site, 7, "Agent07")
        .await
        .expect("session");
    for cookie in [None, Some(&author)] {
        let (_, _, html) = get(&app, "/news", cookie.map(String::as_str)).await;
        assert!(
            !html.contains("Removed post"),
            "rejected is invisible: {html}"
        );
        assert!(!html.contains("Waiting for review"));
    }
}

#[tokio::test]
async fn the_feed_arrives_a_page_at_a_time() {
    let harness = Harness::new().await;
    // 12 entries: more than one page, with a partial last page.
    for i in 0..12 {
        harness
            .published(&format!("Entry number {i}"), 1_764_000_000 + i * 60)
            .await;
    }
    let app = harness.router();

    let (status, _, html) = get(&app, "/news", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        html.matches("<li id=\"post-").count(),
        10,
        "the first page holds ten entries"
    );
    assert!(
        html.contains("data-news-more"),
        "and offers the rest: {html}"
    );
    assert!(html.contains("Showing 10 of 12"));
    assert!(
        html.contains("Entry number 11"),
        "the newest entry comes first: {html}"
    );
    assert!(
        !html.contains("Entry number 0"),
        "the oldest is not in the first page: {html}"
    );
}

#[tokio::test]
async fn the_next_batch_is_the_same_markup_and_says_when_it_ends() {
    let harness = Harness::new().await;
    for i in 0..12 {
        harness
            .published(&format!("Entry number {i}"), 1_764_000_000 + i * 60)
            .await;
    }
    let app = harness.router();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/news/cards?offset=10")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-news-more")
            .and_then(|value| value.to_str().ok()),
        Some("false"),
        "nothing left after this batch"
    );
    let body = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("body");
    let html = String::from_utf8_lossy(&body);

    assert_eq!(html.matches("<li id=\"post-").count(), 2);
    assert!(
        html.contains("data-thumb-post"),
        "the batch carries its thumbs-up buttons: {html}"
    );
    assert!(
        !html.contains("<!doctype html>"),
        "a fragment, not a page: {html}"
    );

    // The first batch reports that there is more.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/news/cards?offset=0")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response
            .headers()
            .get("x-news-more")
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
}

#[tokio::test]
async fn a_short_feed_has_no_load_more_control() {
    let harness = Harness::new().await;
    harness.published("Only entry", 1_764_000_000).await;
    let app = harness.router();

    let (_, _, html) = get(&app, "/news", None).await;
    assert!(!html.contains("data-news-more"), "{html}");
    assert!(!html.contains("Load older entries"));
}

#[tokio::test]
async fn the_queue_page_is_moderator_only() {
    let harness = Harness::new().await;
    let entry = webapi::news::validate("news", "In review", "", "Body", &[]).expect("valid");
    store::create(&harness.site, 7, "Agent07", &entry, 1_764_000_000)
        .await
        .expect("create");
    let app = harness.router();
    let author = auth::create_session(&harness.site, 7, "Agent07")
        .await
        .expect("session");

    let (status, _, _) = get(&app, "/api/news/queue", Some(&author)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = get(&app, "/api/news/queue", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn pending_posts_keep_their_status_visible_to_their_author() {
    let harness = Harness::new().await;
    let entry = webapi::news::validate("news", "Mine", "", "Body", &[]).expect("valid");
    store::create(&harness.site, 7, "Agent07", &entry, 1_764_000_000)
        .await
        .expect("create");
    let app = harness.router();
    let author = auth::create_session(&harness.site, 7, "Agent07")
        .await
        .expect("session");

    let (status, _, body) = get(&app, "/api/news/mine", Some(&author)).await;
    assert_eq!(status, StatusCode::OK);
    let mine: serde_json::Value = serde_json::from_str(&body).expect("json");
    let posts = mine["posts"].as_array().expect("posts");
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["status"], Status::Pending.as_str());
}
