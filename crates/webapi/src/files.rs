//! `/files/*` — the download directory.
//!
//! The launcher pulls restored-map archives from these URLs, so the paths are a
//! public contract inherited from the previous site: `/files/<name>` and
//! `/files/<dir>/<name>`, exactly as before.
//!
//! Everything is read from disk on every request with no in-memory cache, so
//! files can be added or replaced while the server runs — that is the whole
//! point of this route.

use std::path::PathBuf;

use axum::Router;
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_http::services::ServeDir;

/// Build the `/files/*` service.
///
/// `ServeDir` already resolves paths safely, streams the body and supports
/// range requests (resumable downloads for the launcher); the guards here add
/// the two things it does not do: reject dot-segments outright, and stop the
/// browser from reusing a stale copy of a file we have since replaced.
pub fn service(dir: PathBuf) -> Router {
    // Files are replaced in place, so responses always revalidate: an unchanged
    // file costs one 304, and range headers still make downloads resumable.
    let files = ServeDir::new(&dir).append_index_html_on_directories(false);

    Router::new()
        .fallback_service(files)
        .layer(axum::middleware::from_fn(reject_unsafe_paths))
}

/// Refuse anything the previous site refused: dotfiles, dot-segments (which is
/// how `..` arrives), NUL bytes and backslashes.
async fn reject_unsafe_paths(request: Request, next: Next) -> Response {
    let path = request.uri().path();

    // Percent-decoded comparison: axum hands us the raw path, so decode before
    // deciding. A path that does not decode cleanly is not a file we serve.
    let decoded = match percent_decode(path) {
        Some(value) => value,
        None => return not_found(),
    };

    if decoded.contains('\0') || decoded.contains('\\') {
        return not_found();
    }

    let unsafe_segment = decoded
        .split('/')
        .any(|segment| segment.starts_with('.') && segment != "." && !segment.is_empty());

    if unsafe_segment {
        return not_found();
    }

    let mut response = next.run(request).await;
    if response.status() == StatusCode::OK || response.status() == StatusCode::PARTIAL_CONTENT {
        // `no-cache` (not `no-store`): revalidate before reuse, because the
        // file behind a URL can change without the URL changing.
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-cache"),
        );
        if !response.headers().contains_key(header::ACCEPT_RANGES) {
            response.headers_mut().insert(
                header::ACCEPT_RANGES,
                header::HeaderValue::from_static("bytes"),
            );
        }
    }
    response
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let value = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
            out.push(value);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

/// Where the files live, resolved once at startup. The directory is not cached:
/// every request hits the filesystem.
pub fn resolve_dir(configured: Option<PathBuf>) -> PathBuf {
    configured.unwrap_or_else(|| PathBuf::from("files"))
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn percent_decoding_covers_the_awkward_cases() {
        assert_eq!(
            percent_decode("/files/a%20b.zip").as_deref(),
            Some("/files/a b.zip")
        );
        assert_eq!(
            percent_decode("/files/%2e%2e/x").as_deref(),
            Some("/files/../x")
        );
        // Truncated escapes are not silently accepted.
        assert_eq!(percent_decode("/files/%2"), None);
        assert_eq!(percent_decode("/files/%zz"), None);
    }
}
