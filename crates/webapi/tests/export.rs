//! The redacted-database download.
//!
//! A third party (the cards registry) pulls this. It is a whole copy of the
//! archive behind a path token, so the interesting cases are the refusals: no
//! token configured, wrong token, and no snapshot published yet.

use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tempfile::TempDir;
use tower::ServiceExt;
use webapi::config::Config;
use webapi::{AppState, app, db};

struct Harness {
    dir: TempDir,
}

impl Harness {
    fn new() -> Self {
        Self {
            dir: TempDir::new().expect("temp dir"),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    async fn router(&self, token: Option<&str>, with_snapshot: bool) -> Router {
        let live_path = self.path("live.db");
        let live = db::open_read_write(&live_path, 1).await.expect("live db");
        sqlx::query("CREATE TABLE ga_users (id INTEGER PRIMARY KEY, username TEXT)")
            .execute(&live)
            .await
            .expect("ddl");
        live.close().await;

        let snapshot_path = self.path("snapshot.db");
        if with_snapshot {
            // Contents are irrelevant to the route; only the transfer matters.
            std::fs::write(
                &snapshot_path,
                b"SQLite format 3\0pretend this is the archive",
            )
            .expect("write snapshot");
        }

        let pool = db::open_read_only(&live_path, 1).await.expect("open");

        let cfg = Config {
            bind: "127.0.0.1:0".parse().expect("addr"),
            live_db: Some(live_path),
            snapshot_db: Some(snapshot_path),
            site_db: None,
            static_dir: self.path("static"),
            files_dir: self.path("files"),
            status_ttl: Duration::from_secs(0),
            live_tick: Duration::from_secs(15),
            match_ttl: Duration::from_secs(0),
            history_ttl: Duration::from_secs(0),
            status_override: None,
            control_api_url: None,
            control_api_token: None,
            db_export_token: token.map(str::to_string),
            news_posting_mode: webapi::news::PostingMode::Open,
        };
        app(AppState::new(cfg, Some(pool), None, None))
    }
}

async fn get(router: &Router, uri: &str) -> (StatusCode, Vec<u8>, Option<String>) {
    let request = Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request");
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    (status, body.to_vec(), content_type)
}

#[tokio::test]
async fn the_archive_is_served_to_the_right_token() {
    let harness = Harness::new();
    let router = harness.router(Some("s3cret-token"), true).await;

    let (status, body, content_type) = get(&router, "/db-export/s3cret-token").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("application/vnd.sqlite3"));
    assert!(
        body.starts_with(b"SQLite format 3"),
        "the file, not a JSON body"
    );
}

#[tokio::test]
async fn a_wrong_or_missing_token_is_indistinguishable_from_no_page() {
    let harness = Harness::new();
    let router = harness.router(Some("s3cret-token"), true).await;

    for uri in [
        "/db-export/wrong",
        "/db-export/s3cret-tokenX",
        "/db-export/s3cret",
        "/db-export/",
    ] {
        let (status, body, _) = get(&router, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        // Never the SPA shell: a stranger must not get HTML for a bad token.
        assert!(!body.starts_with(b"<!doctype"), "{uri}");
    }
}

#[tokio::test]
async fn without_a_configured_token_the_route_does_not_exist() {
    let harness = Harness::new();
    let router = harness.router(None, true).await;

    let (status, _, _) = get(&router, "/db-export/anything").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_token_with_no_published_snapshot_is_a_404() {
    let harness = Harness::new();
    let router = harness.router(Some("s3cret-token"), false).await;

    let (status, _, _) = get(&router, "/db-export/s3cret-token").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
