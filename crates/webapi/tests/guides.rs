//! Guides and builds API tests.
//!
//! The rules worth pinning down, because they are the ones that would be
//! expensive to get wrong in production: nothing is visible before approval,
//! only a moderator decides, a rejection keeps the record but removes it from
//! view, filters and ordering actually filter and order, and an uploaded image
//! is re-encoded rather than stored.

use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tower::ServiceExt;
use webapi::config::Config;
use webapi::guides::store;
use webapi::news::PostingMode;
use webapi::{AppState, app, auth, db};

struct Harness {
    dir: TempDir,
    site: SqlitePool,
    live_path: PathBuf,
}

impl Harness {
    async fn new() -> Self {
        let dir = TempDir::new().expect("temp dir");
        let site = db::open_read_write(&dir.path().join("site.db"), 1)
            .await
            .expect("site db");
        auth::migrate(&site).await.expect("sessions table");
        store::migrate(&site).await.expect("guides tables");
        // Approving an entry announces it in the news feed, so the news tables
        // are part of what this section needs — exactly as at startup.
        webapi::news::store::migrate(&site)
            .await
            .expect("news tables");
        Self {
            site,
            live_path: dir.path().join("server.db"),
            dir,
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

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

    fn router(&self, live: Option<SqlitePool>) -> Router {
        let cfg = Config {
            bind: "127.0.0.1:0".parse().expect("addr"),
            live_db: Some(self.live_path.clone()),
            snapshot_db: None,
            site_db: Some(self.path("site.db")),
            images_dir: self.path("images"),
            files_dir: self.path("files"),
            status_ttl: Duration::from_secs(60),
            live_tick: Duration::from_secs(15),
            match_ttl: Duration::from_secs(30),
            history_ttl: Duration::from_secs(3600),
            status_override: None,
            control_api_url: None,
            control_api_token: None,
            db_export_token: None,
            news_posting_mode: PostingMode::Open,
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
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    (status, String::from_utf8_lossy(&body).to_string())
}

fn json_request(
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: serde_json::Value,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, format!("cw_session={cookie}"));
    }
    builder
        .body(Body::from(serde_json::to_vec(&body).expect("json")))
        .expect("request")
}

fn get(cookie: Option<&str>, uri: &str) -> Request<Body> {
    let mut builder = Request::builder().uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, format!("cw_session={cookie}"));
    }
    builder.body(Body::empty()).expect("request")
}

fn guide_body(title: &str, tags: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "category": "guide",
        "title": title,
        "summary": "A short summary",
        "body": "## Heading\n\nSome *advice*.",
        "tags": tags,
    })
}

/// A tiny PNG, built here so the tests do not carry a fixture file.
fn png(width: u32, height: u32) -> Vec<u8> {
    use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage};
    let mut image = RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgba([(x % 256) as u8, (y % 256) as u8, 90, 255]);
    }
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(image.as_raw(), width, height, ExtendedColorType::Rgba8)
        .expect("encode");
    out
}

async fn upload(app: &Router, cookie: &str, bytes: Vec<u8>) -> (StatusCode, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/api/guides/images")
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::COOKIE, format!("cw_session={cookie}"))
        .body(Body::from(bytes))
        .expect("request");
    send(app, request).await
}

#[tokio::test]
async fn a_submission_is_pending_until_a_moderator_approves_it() {
    let harness = Harness::new().await;
    let live = harness.live_with_roles(&[]).await;
    let app = harness.router(Some(live));
    let cookie = harness.session(1, "author").await;

    let (status, body) = send(
        &app,
        json_request(
            "POST",
            "/api/guides",
            Some(&cookie),
            guide_body("Assault basics", &["basics"]),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // Not in the public list ...
    let (_, feed) = send(&app, get(None, "/api/guides")).await;
    assert!(!feed.contains("Assault basics"), "{feed}");

    // ... but visible to its author, marked as theirs to edit.
    let (_, mine) = send(&app, get(Some(&cookie), "/api/guides")).await;
    assert!(mine.contains("Assault basics"), "{mine}");
    assert!(mine.contains("\"can_edit\":true"), "{mine}");

    // A passer-by who guesses the id gets nothing.
    let (hidden, _) = send(&app, get(None, "/api/guides/1")).await;
    assert_eq!(hidden, StatusCode::NOT_FOUND);

    // Only a moderator may publish it.
    let (forbidden, _) = send(
        &app,
        json_request(
            "POST",
            "/api/guides/1/approve",
            Some(&cookie),
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);

    let moderator = harness.session(2, "mod").await;
    let moderator_app = harness.router(Some(
        harness.live_with_roles(&[(2, "news_moderator")]).await,
    ));
    let (status, _) = send(
        &moderator_app,
        json_request(
            "POST",
            "/api/guides/1/approve",
            Some(&moderator),
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, feed) = send(&moderator_app, get(None, "/api/guides")).await;
    assert!(feed.contains("Assault basics"), "{feed}");
    assert!(feed.contains("\"status\":\"approved\""), "{feed}");
}

#[tokio::test]
async fn a_rejection_keeps_the_row_and_hides_the_entry() {
    let harness = Harness::new().await;
    let moderator = harness.session(9, "mod").await;
    let app = harness.router(Some(
        harness.live_with_roles(&[(9, "news_moderator")]).await,
    ));
    let author = harness.session(3, "author").await;

    let (_, body) = send(
        &app,
        json_request(
            "POST",
            "/api/guides",
            Some(&author),
            guide_body("Bad guide", &["strategy"]),
        ),
    )
    .await;
    let id = id_of(&body);

    // A note is required: a rejection with no reason is not a decision the
    // author can act on.
    let (status, _) = send(
        &app,
        json_request(
            "POST",
            &format!("/api/guides/{id}/reject"),
            Some(&moderator),
            serde_json::json!({ "note": "  " }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send(
        &app,
        json_request(
            "POST",
            &format!("/api/guides/{id}/reject"),
            Some(&moderator),
            serde_json::json!({ "note": "Not a guide, this is a build." }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Gone from the feed, and gone from the author's list too.
    let (_, feed) = send(&app, get(None, "/api/guides")).await;
    assert!(!feed.contains("Bad guide"), "{feed}");
    let (_, mine) = send(&app, get(Some(&author), "/api/guides")).await;
    assert!(!mine.contains("Bad guide"), "{mine}");

    // The record survives, which is the point of a soft delete.
    let kept = store::by_id(&harness.site, id)
        .await
        .expect("row")
        .expect("present");
    assert_eq!(kept.status, "rejected");
    assert_eq!(
        kept.moderation_note.as_deref(),
        Some("Not a guide, this is a build.")
    );
}

#[tokio::test]
async fn filters_and_ordering_do_what_they_say() {
    let harness = Harness::new().await;
    let moderator = harness.session(4, "mod").await;
    let app = harness.router(Some(
        harness.live_with_roles(&[(4, "news_moderator")]).await,
    ));
    let author = harness.session(5, "author").await;

    for (title, tags) in [
        ("Positioning primer", &["strategy"][..]),
        ("Medic in depth", &["class", "strategy"][..]),
        ("First match", &["basics"][..]),
    ] {
        let (_, body) = send(
            &app,
            json_request(
                "POST",
                "/api/guides",
                Some(&author),
                guide_body(title, tags),
            ),
        )
        .await;
        let id = id_of(&body);
        send(
            &app,
            json_request(
                "POST",
                &format!("/api/guides/{id}/approve"),
                Some(&moderator),
                serde_json::json!({}),
            ),
        )
        .await;
    }

    let (_, only_basics) = send(&app, get(None, "/api/guides?tag=basics")).await;
    assert!(only_basics.contains("First match"), "{only_basics}");
    assert!(!only_basics.contains("Medic in depth"), "{only_basics}");
    assert!(only_basics.contains("\"total\":1"), "{only_basics}");

    let (_, searched) = send(&app, get(None, "/api/guides?q=medic")).await;
    assert!(searched.contains("Medic in depth"), "{searched}");
    assert!(!searched.contains("First match"), "{searched}");

    // Newest first puts the last submission on top; helpful puts the thumbed one
    // there regardless of age.
    let (_, newest) = send(&app, get(None, "/api/guides?sort=newest")).await;
    assert!(
        newest.find("First match").expect("present")
            < newest.find("Positioning primer").expect("present"),
        "{newest}"
    );

    send(
        &app,
        json_request(
            "POST",
            "/api/guides/1/thumb",
            Some(&author),
            serde_json::json!({ "active": true }),
        ),
    )
    .await;
    let (_, helpful) = send(&app, get(None, "/api/guides")).await;
    assert!(
        helpful.find("Positioning primer").expect("present")
            < helpful.find("First match").expect("present"),
        "{helpful}"
    );
}

#[tokio::test]
async fn a_build_needs_an_image_and_only_its_uploader_may_attach_it() {
    let harness = Harness::new().await;
    let app = harness.router(Some(harness.live_with_roles(&[]).await));
    let author = harness.session(6, "builder").await;
    let other = harness.session(7, "someone").await;

    // No image: refused, and the refusal names the missing piece.
    let (status, body) = send(
        &app,
        json_request(
            "POST",
            "/api/guides",
            Some(&author),
            serde_json::json!({
                "category": "build",
                "title": "Burst assault",
                "class": "assault",
                "modes": ["pvp"],
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("screenshot"), "{body}");

    // Upload, and get back a derivative with a content-addressed name.
    let (status, uploaded) = upload(&app, &author, png(400, 300)).await;
    assert_eq!(status, StatusCode::OK, "{uploaded}");
    assert!(uploaded.contains(".webp"), "{uploaded}");
    assert!(!uploaded.contains("image/png"), "{uploaded}");
    let image_id: i64 =
        serde_json::from_str::<serde_json::Value>(&uploaded).expect("json")["image_id"]
            .as_i64()
            .expect("id");

    // Someone else cannot borrow that upload.
    let (status, _) = send(
        &app,
        json_request(
            "POST",
            "/api/guides",
            Some(&other),
            serde_json::json!({
                "category": "build",
                "title": "Not mine",
                "class": "medic",
                "modes": ["pve"],
                "image_id": image_id,
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Its owner can, and the entry carries the link.
    let (status, body) = send(
        &app,
        json_request(
            "POST",
            "/api/guides",
            Some(&author),
            serde_json::json!({
                "category": "build",
                "title": "Burst assault",
                "class": "assault",
                "modes": ["pvp", "pve"],
                "discord": "https://discord.com/channels/1/2/3",
                "image_id": image_id,
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (_, builds) = send(&app, get(Some(&author), "/api/guides?category=build")).await;
    assert!(builds.contains("/images/"), "{builds}");
    assert!(builds.contains("discord.com/channels/1/2/3"), "{builds}");
}

/// A feed thumbnail must not pull a full-size screenshot down the wire: 480px
/// tiles are a fifth of the bytes, and `/news` was showing the 1600px original.
#[tokio::test]
async fn the_news_feed_points_at_the_tile_rather_than_the_full_screenshot() {
    let harness = Harness::new().await;
    let moderator = harness.session(8, "mod").await;
    let app = harness.router(Some(
        harness.live_with_roles(&[(8, "news_moderator")]).await,
    ));
    let author = harness.session(9, "builder").await;

    let (_, uploaded) = upload(&app, &author, png(400, 300)).await;
    let upload: serde_json::Value = serde_json::from_str(&uploaded).expect("json");
    let image_id = upload["image_id"].as_i64().expect("id");
    let tile_url = upload["tile_url"].as_str().expect("tile url").to_string();
    let full_url = upload["url"].as_str().expect("url").to_string();

    let (_, body) = send(
        &app,
        json_request(
            "POST",
            "/api/guides",
            Some(&author),
            serde_json::json!({
                "category": "build",
                "title": "Burst assault",
                "class": "assault",
                "modes": ["pvp"],
                "image_id": image_id,
            }),
        ),
    )
    .await;
    let id = id_of(&body);
    send(
        &app,
        json_request(
            "POST",
            &format!("/api/guides/{id}/approve"),
            Some(&moderator),
            serde_json::json!({}),
        ),
    )
    .await;

    let (_, feed) = send(&app, get(None, "/api/news?limit=10")).await;
    assert!(
        feed.contains("Burst assault"),
        "announcement missing: {feed}"
    );
    assert!(
        feed.contains(&format!("\"image_tile_url\":\"{tile_url}\"")),
        "feed should offer {tile_url}: {feed}"
    );
    // The full-size URL is still there for the detail page to use.
    assert!(
        feed.contains(&format!("\"image_url\":\"{full_url}\"")),
        "{feed}"
    );
}

#[tokio::test]
async fn an_uploaded_image_is_reencoded_and_never_served_as_uploaded() {
    let harness = Harness::new().await;
    let app = harness.router(Some(harness.live_with_roles(&[]).await));
    let author = harness.session(8, "builder").await;

    // A PNG with a payload after its end marker: the classic polyglot.
    let mut source = png(64, 64);
    source.extend_from_slice(b"<?php echo 'pwned'; ?>");
    let (status, uploaded) = upload(&app, &author, source).await;
    assert_eq!(status, StatusCode::OK, "{uploaded}");

    let name = serde_json::from_str::<serde_json::Value>(&uploaded).expect("json")["url"]
        .as_str()
        .expect("url")
        .trim_start_matches("/images/")
        .to_string();

    let (status, body) = send(&app, get(None, &format!("/images/{name}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("<?php"), "the payload reached the client");

    // Only names this process wrote are servable.
    for bad in [
        "/images/..%2f..%2fsite.db",
        "/images/site.db",
        "/images/nope.webp",
    ] {
        let (status, _) = send(&app, get(None, bad)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{bad}");
    }
}

#[tokio::test]
async fn a_text_file_is_refused_as_an_image() {
    let harness = Harness::new().await;
    let app = harness.router(Some(harness.live_with_roles(&[]).await));
    let author = harness.session(10, "builder").await;

    for (what, bytes) in [
        (
            "svg",
            br#"<svg xmlns="http://www.w3.org/2000/svg"><script/></svg>"#.to_vec(),
        ),
        ("text", b"this is not an image".to_vec()),
        ("empty", Vec::new()),
    ] {
        let (status, body) = upload(&app, &author, bytes).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: {body}");
    }
}

#[tokio::test]
async fn an_author_may_fix_a_pending_entry_but_not_a_published_one() {
    let harness = Harness::new().await;
    let moderator = harness.session(11, "mod").await;
    let app = harness.router(Some(
        harness.live_with_roles(&[(11, "news_moderator")]).await,
    ));
    let author = harness.session(12, "author").await;

    let (_, body) = send(
        &app,
        json_request(
            "POST",
            "/api/guides",
            Some(&author),
            guide_body("Draft title", &["basics"]),
        ),
    )
    .await;
    let id = id_of(&body);

    let (status, _) = send(
        &app,
        json_request(
            "PATCH",
            &format!("/api/guides/{id}"),
            Some(&author),
            guide_body("Fixed title", &["basics", "strategy"]),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let entry = store::by_id(&harness.site, id)
        .await
        .expect("row")
        .expect("present");
    assert_eq!(entry.title, "Fixed title");
    assert_eq!(entry.tags.len(), 2);

    // Someone else's edit is refused even while it is pending.
    let other = harness.session(13, "other").await;
    let (status, _) = send(
        &app,
        json_request(
            "PATCH",
            &format!("/api/guides/{id}"),
            Some(&other),
            guide_body("Hijacked", &["basics"]),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    send(
        &app,
        json_request(
            "POST",
            &format!("/api/guides/{id}/approve"),
            Some(&moderator),
            serde_json::json!({}),
        ),
    )
    .await;

    let (status, _) = send(
        &app,
        json_request(
            "PATCH",
            &format!("/api/guides/{id}"),
            Some(&author),
            guide_body("Edited after approval", &["basics"]),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn likes_are_per_account_and_toggle() {
    let harness = Harness::new().await;
    let moderator = harness.session(14, "mod").await;
    let app = harness.router(Some(
        harness.live_with_roles(&[(14, "news_moderator")]).await,
    ));
    let author = harness.session(15, "author").await;
    let reader = harness.session(16, "reader").await;

    let (_, body) = send(
        &app,
        json_request(
            "POST",
            "/api/guides",
            Some(&author),
            guide_body("Thumbable", &["basics"]),
        ),
    )
    .await;
    let id = id_of(&body);
    send(
        &app,
        json_request(
            "POST",
            &format!("/api/guides/{id}/approve"),
            Some(&moderator),
            serde_json::json!({}),
        ),
    )
    .await;

    for (active, expected) in [(true, 1), (true, 1), (false, 0)] {
        let (_, response) = send(
            &app,
            json_request(
                "POST",
                &format!("/api/guides/{id}/thumb"),
                Some(&reader),
                serde_json::json!({ "active": active }),
            ),
        )
        .await;
        let value: serde_json::Value = serde_json::from_str(&response).expect("json");
        assert_eq!(value["thumbs_up"].as_i64(), Some(expected), "{response}");
    }

    // Anonymous readers cannot like anything.
    let (status, _) = send(
        &app,
        json_request(
            "POST",
            &format!("/api/guides/{id}/thumb"),
            None,
            serde_json::json!({ "active": true }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_options_endpoint_describes_the_vocabulary() {
    let harness = Harness::new().await;
    let app = harness.router(Some(harness.live_with_roles(&[]).await));

    let (status, body) = send(&app, get(None, "/api/guides/options")).await;
    assert_eq!(status, StatusCode::OK);

    let value: serde_json::Value = serde_json::from_str(&body).expect("json");
    let tags: Vec<&str> = value["tags"]
        .as_array()
        .expect("tags")
        .iter()
        .map(|tag| tag["value"].as_str().expect("value"))
        .collect();
    assert_eq!(
        tags,
        vec!["basics", "strategy", "class", "mode", "pve", "pvp"]
    );

    let classes: Vec<&str> = value["classes"]
        .as_array()
        .expect("classes")
        .iter()
        .map(|class| class["value"].as_str().expect("value"))
        .collect();
    assert_eq!(classes, vec!["assault", "medic", "recon", "robotic"]);

    // Posting rules are part of the same answer, so the form cannot disagree
    // with the endpoint that enforces them.
    assert_eq!(value["posting"]["signed_in"].as_bool(), Some(false));
    assert_eq!(value["posting"]["may_post"].as_bool(), Some(false));
}

#[tokio::test]
async fn approving_announces_the_entry_in_the_feed_and_unpublishing_withdraws_it() {
    let harness = Harness::new().await;
    let moderator = harness.session(20, "mod").await;
    let app = harness.router(Some(
        harness.live_with_roles(&[(20, "news_moderator")]).await,
    ));
    let author = harness.session(21, "announcer").await;

    let (_, body) = send(
        &app,
        json_request(
            "POST",
            "/api/guides",
            Some(&author),
            guide_body("Lane control", &["strategy"]),
        ),
    )
    .await;
    let id = id_of(&body);

    // Nothing in the feed before approval.
    let (_, before) = send(&app, get(None, "/api/news")).await;
    assert!(!before.contains("Lane control"), "{before}");

    send(
        &app,
        json_request(
            "POST",
            &format!("/api/guides/{id}/approve"),
            Some(&moderator),
            serde_json::json!({}),
        ),
    )
    .await;

    // The announcement is a feed post that points at the entry's page.
    let (_, after) = send(&app, get(None, "/api/news")).await;
    assert!(after.contains("has posted a guide"), "{after}");
    assert!(after.contains("Lane control"), "{after}");
    // The announcement links at the entry's page, not at the post.
    assert!(
        after.contains(&format!("/guides/{id}-lane-control")),
        "{after}"
    );
    assert!(after.contains(r#""guide":{"#), "{after}");

    // Unpublishing it takes the announcement with it.
    send(
        &app,
        json_request(
            "POST",
            &format!("/api/guides/{id}/unapprove"),
            Some(&moderator),
            serde_json::json!({ "note": "Needs a screenshot of the lane." }),
        ),
    )
    .await;
    let (_, withdrawn) = send(&app, get(None, "/api/news")).await;
    assert!(!withdrawn.contains("Lane control"), "{withdrawn}");

    // And re-approving announces it exactly once, not twice: the title appears
    // in the post and again in the pointer the card is drawn from, so count the
    // pointers.
    send(
        &app,
        json_request(
            "POST",
            &format!("/api/guides/{id}/approve"),
            Some(&moderator),
            serde_json::json!({}),
        ),
    )
    .await;
    let (_, again) = send(&app, get(None, "/api/news")).await;
    assert_eq!(again.matches(r#""guide":{"#).count(), 1, "{again}");
}

#[tokio::test]
async fn published_entries_reach_the_sitemap() {
    let harness = Harness::new().await;
    let moderator = harness.session(30, "mod").await;
    let app = harness.router(Some(
        harness.live_with_roles(&[(30, "news_moderator")]).await,
    ));
    let author = harness.session(31, "author").await;

    let (_, body) = send(
        &app,
        json_request(
            "POST",
            "/api/guides",
            Some(&author),
            guide_body("Sitemap me", &["basics"]),
        ),
    )
    .await;
    let id = id_of(&body);

    // A pending entry is not a page yet, so it is not advertised.
    let (_, before) = send(&app, get(None, "/sitemap.xml")).await;
    assert!(!before.contains("sitemap-me"), "{before}");

    send(
        &app,
        json_request(
            "POST",
            &format!("/api/guides/{id}/approve"),
            Some(&moderator),
            serde_json::json!({}),
        ),
    )
    .await;

    let (_, after) = send(&app, get(None, "/sitemap.xml")).await;
    assert!(
        after.contains(&format!("/guides/{id}-sitemap-me")),
        "{after}"
    );
    // The section's own pages are listed once, not once per entry.
    assert_eq!(
        after
            .matches("<loc>https://example.test/guides</loc>")
            .count(),
        1,
        "{after}"
    );
}

fn id_of(body: &str) -> i64 {
    serde_json::from_str::<serde_json::Value>(body).expect("json")["id"]
        .as_i64()
        .expect("id")
}
