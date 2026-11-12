//! `/files/*` contract: the launcher downloads restored content from these URLs,
//! so the paths, the bytes and the "change it without a restart" behaviour are
//! all load-bearing.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tempfile::TempDir;
use tower::ServiceExt;
use webapi::config::Config;
use webapi::{AppState, app};

async fn harness(dir: &TempDir) -> (axum::Router, PathBuf) {
    let files = dir.path().join("files");
    std::fs::create_dir_all(files.join("maez")).expect("mkdir");
    let cfg = Config {
        bind: "127.0.0.1:0".parse().expect("addr"),
        live_db: None,
        snapshot_db: None,
        site_db: None,
        files_dir: files.clone(),
        images_dir: files.clone(),
        status_ttl: std::time::Duration::from_secs(60),
        live_tick: std::time::Duration::from_secs(15),
        match_ttl: std::time::Duration::from_secs(30),
        history_ttl: std::time::Duration::from_secs(3600),
        status_override: None,
        control_api_url: None,
        control_api_token: None,
        db_export_token: None,
        news_posting_mode: webapi::news::PostingMode::Open,
        site_url: "https://example.test".to_string(),
    };
    let state = AppState::new(cfg, None, None, None);
    (app(state), files)
}

async fn get(router: &axum::Router, path: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let request = Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("request");
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), 16 << 20)
        .await
        .expect("body")
        .to_vec();
    (status, headers, body)
}

#[tokio::test]
async fn serves_nested_files_at_their_historical_paths() {
    let dir = TempDir::new().expect("tempdir");
    let (router, files) = harness(&dir).await;
    std::fs::write(files.join("maez/maze-linux.zip"), b"PK\x03\x04linux").expect("write");

    let (status, headers, body) = get(&router, "/files/maez/maze-linux.zip").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"PK\x03\x04linux");
    assert_eq!(headers[header::CONTENT_TYPE], "application/zip");
    assert_eq!(headers[header::CACHE_CONTROL], "no-cache");
    assert_eq!(headers[header::ACCEPT_RANGES], "bytes");
}

#[tokio::test]
async fn replaced_files_are_picked_up_without_a_restart() {
    let dir = TempDir::new().expect("tempdir");
    let (router, files) = harness(&dir).await;
    let target = files.join("carbon-capture.zip");

    std::fs::write(&target, b"version one").expect("write");
    let (_, _, first) = get(&router, "/files/carbon-capture.zip").await;
    assert_eq!(first, b"version one");

    // Same path, new bytes, same running process — and a new size, so any
    // cached length would show up here too.
    std::fs::write(&target, b"version two, longer").expect("rewrite");
    let (_, _, second) = get(&router, "/files/carbon-capture.zip").await;
    assert_eq!(second, b"version two, longer");

    // A brand-new file appears the same way.
    std::fs::write(files.join("new-map.zip"), b"fresh").expect("write");
    let (status, _, body) = get(&router, "/files/new-map.zip").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"fresh");
}

#[tokio::test]
async fn range_requests_let_downloads_resume() {
    let dir = TempDir::new().expect("tempdir");
    let (router, files) = harness(&dir).await;
    std::fs::write(files.join("big.zip"), b"0123456789").expect("write");

    let request = Request::builder()
        .uri("/files/big.zip")
        .header(header::RANGE, "bytes=4-6")
        .body(Body::empty())
        .expect("request");
    let response = router.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 4-6/10");
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    assert_eq!(&body[..], b"456");
}

#[tokio::test]
async fn missing_and_unsafe_paths_are_404s_not_tracebacks() {
    let dir = TempDir::new().expect("tempdir");
    let (router, files) = harness(&dir).await;
    std::fs::write(files.join("real.zip"), b"x").expect("write");
    std::fs::write(files.join(".secret"), b"x").expect("write");

    for path in [
        "/files/nope.zip",
        "/files/.secret",
        "/files/%2esecret",
        "/files/../Cargo.toml",
        "/files/%2e%2e/Cargo.toml",
        "/files/maez/",
    ] {
        let (status, _, _) = get(&router, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path} should be 404");
    }
}

#[tokio::test]
async fn extensionless_file_names_are_not_rewritten_to_pages() {
    let dir = TempDir::new().expect("tempdir");
    let (router, files) = harness(&dir).await;
    // A patch archive with no extension is legitimate; the SPA page rewrite must
    // not turn it into `<name>.html`.
    std::fs::write(files.join("no-extension"), b"raw").expect("write");

    let (status, _, body) = get(&router, "/files/no-extension").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"raw");
}
