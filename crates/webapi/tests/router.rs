//! What this process answers, and what it deliberately does not.
//!
//! The API is one of two processes: SvelteKit's Node server serves the pages and
//! the edge routes between them. So the property worth locking down is that this
//! router never pretends to be the site — no HTML, no app shell, no 200 for a
//! path it does not know — while the paths it does own keep working.

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tempfile::TempDir;
use tower::ServiceExt;
use webapi::config::Config;
use webapi::{AppState, app};

fn router(dir: &TempDir) -> Router {
    let files = dir.path().join("files");
    std::fs::create_dir_all(&files).expect("files dir");
    std::fs::write(files.join("map.zip"), b"zip-bytes").expect("download");

    let cfg = Config {
        bind: "127.0.0.1:0".parse().expect("addr"),
        live_db: None,
        snapshot_db: None,
        site_db: None,
        files_dir: files,
        status_ttl: Duration::from_secs(0),
        live_tick: Duration::from_secs(15),
        match_ttl: Duration::from_secs(0),
        history_ttl: Duration::from_secs(0),
        status_override: None,
        control_api_url: None,
        control_api_token: None,
        db_export_token: None,
        news_posting_mode: webapi::news::PostingMode::Open,
        site_url: "https://example.test".to_string(),
    };
    app(AppState::new(cfg, None, None, None))
}

async fn get(router: &Router, path: &str) -> (StatusCode, String, String) {
    let request = Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("request");
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    (
        status,
        content_type,
        String::from_utf8_lossy(&body).to_string(),
    )
}

#[tokio::test]
async fn the_root_is_not_a_page_here() {
    // The site is the other process. Answering HTML at `/` would mean two
    // renderers again, which is what this whole arrangement exists to avoid.
    let dir = TempDir::new().expect("temp");
    let (status, content_type, body) = get(&router(&dir), "/").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        content_type.starts_with("application/json"),
        "{content_type}"
    );
    assert!(!body.contains("<html"), "{body}");
}

#[tokio::test]
async fn an_unknown_path_is_a_json_404() {
    let dir = TempDir::new().expect("temp");
    for path in ["/nowhere", "/server", "/agents/1", "/leaderboard"] {
        let (status, content_type, body) = get(&router(&dir), path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(
            content_type.starts_with("application/json"),
            "{path} answered {content_type}"
        );
        assert!(body.contains("not found"), "{path}: {body}");
    }
}

#[tokio::test]
async fn unknown_api_paths_are_json_404s() {
    let dir = TempDir::new().expect("temp");
    let (status, content_type, _) = get(&router(&dir), "/api/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        content_type.starts_with("application/json"),
        "{content_type}"
    );
}

#[tokio::test]
async fn downloads_come_from_the_files_directory() {
    let dir = TempDir::new().expect("temp");
    let (status, _, body) = get(&router(&dir), "/files/map.zip").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "zip-bytes");

    let (missing, _, _) = get(&router(&dir), "/files/absent.zip").await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_export_route_never_falls_through() {
    // A wrong token must read as "no such export", not as a page or a hint.
    let dir = TempDir::new().expect("temp");
    for path in ["/db-export", "/db-export/", "/db-export/wrong-token"] {
        let (status, _, body) = get(&router(&dir), path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(!body.contains("<html"), "{path}: {body}");
    }
}

#[tokio::test]
async fn the_sitemap_is_served_here() {
    // The page list travels with the release that knows it, and the edge sends
    // this path to the API rather than to the site.
    let dir = TempDir::new().expect("temp");
    let (status, content_type, body) = get(&router(&dir), "/sitemap.xml").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        content_type.starts_with("application/xml"),
        "{content_type}"
    );
    assert!(body.contains("<urlset"), "{body}");
    assert!(body.contains("https://example.test/news"), "{body}");
}
