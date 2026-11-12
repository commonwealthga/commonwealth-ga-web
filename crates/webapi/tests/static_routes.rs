//! What a request gets when the file it asked for does not exist.
//!
//! The site is a prerendered SvelteKit build plus client-rendered routes. Both
//! kinds of page must answer 200; anything else must answer 404, because a
//! blanket app-shell fallback answers 200 to every typo and lets a crawler
//! index infinite garbage.

use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tempfile::TempDir;
use tower::ServiceExt;
use webapi::config::Config;
use webapi::{AppState, app};

const SHELL: &str = "<!doctype html><html><body><div id=\"shell\">app shell</div></body></html>";

fn router(dir: &TempDir) -> Router {
    let static_dir = dir.path().join("static");
    std::fs::create_dir_all(&static_dir).expect("static dir");
    // The two shapes adapter-static emits: a prerendered page, and the shell.
    std::fs::write(static_dir.join("index.html"), "<html>home</html>").expect("index");
    std::fs::write(
        static_dir.join("server.html"),
        "<html>prerendered status</html>",
    )
    .expect("server");
    std::fs::write(static_dir.join("200.html"), SHELL).expect("shell");
    std::fs::write(static_dir.join("og.jpg"), b"jpeg-bytes").expect("image");

    let cfg = Config {
        bind: "127.0.0.1:0".parse().expect("addr"),
        live_db: None,
        snapshot_db: None,
        site_db: None,
        static_dir: PathBuf::from(&static_dir),
        files_dir: dir.path().join("files"),
        status_ttl: Duration::from_secs(0),
        live_tick: Duration::from_secs(15),
        match_ttl: Duration::from_secs(0),
        history_ttl: Duration::from_secs(0),
        status_override: None,
        control_api_url: None,
        control_api_token: None,
        db_export_token: None,
        news_posting_mode: webapi::news::PostingMode::Open,
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
async fn a_prerendered_page_is_served_as_a_file() {
    let dir = TempDir::new().expect("temp");
    let router = router(&dir);

    let (status, content_type, body) = get(&router, "/server").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/html"));
    assert!(body.contains("prerendered status"), "got: {body}");
}

#[tokio::test]
async fn the_landing_page_is_the_prerendered_index() {
    let dir = TempDir::new().expect("temp");
    let router = router(&dir);

    let (status, _, body) = get(&router, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("home"));
}

#[tokio::test]
async fn client_rendered_routes_get_the_shell_with_a_200() {
    let dir = TempDir::new().expect("temp");
    let router = router(&dir);

    for path in [
        "/agents/1607",
        "/missions/12400",
        "/leaderboard",
        "/me",
        "/settings",
    ] {
        let (status, _, body) = get(&router, path).await;
        assert_eq!(status, StatusCode::OK, "{path} should render");
        assert!(body.contains("app shell"), "{path} should get the shell");
    }
}

#[tokio::test]
async fn an_unknown_path_is_a_real_404_and_not_a_second_homepage() {
    let dir = TempDir::new().expect("temp");
    let router = router(&dir);

    for path in [
        "/nope",
        "/agents-are-not-real",
        "/server/extra/deep",
        "/wp-admin",
    ] {
        let (status, _, body) = get(&router, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path} must be a 404");
        // The shell still ships so the app can draw its error page; the status
        // is what crawlers and monitoring read.
        assert!(body.contains("app shell"));
    }
}

#[tokio::test]
async fn a_missing_asset_stays_a_404_without_the_shell() {
    let dir = TempDir::new().expect("temp");
    let router = router(&dir);

    let (status, _, body) = get(&router, "/missing.png").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.contains("app shell"), "assets must not receive HTML");

    let (status, content_type, _) = get(&router, "/og.jpg").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "image/jpeg");
}

#[tokio::test]
async fn unknown_api_paths_are_json_404s() {
    let dir = TempDir::new().expect("temp");
    let router = router(&dir);

    let (status, content_type, body) = get(&router, "/api/nothing-here").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        content_type.starts_with("application/json"),
        "got {content_type}"
    );
    assert!(body.contains("error"), "got {body}");
}

#[tokio::test]
async fn the_export_route_never_falls_through_to_the_shell() {
    let dir = TempDir::new().expect("temp");
    let router = router(&dir);

    for path in ["/db-export", "/db-export/", "/db-export/whatever"] {
        let (status, _, body) = get(&router, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert!(!body.contains("app shell"), "{path} must not serve HTML");
    }
}
