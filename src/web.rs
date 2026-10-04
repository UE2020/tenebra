//! Serves the vendored [Lux](https://github.com/BlueCannonBall/lux) web client
//! from the `web/` directory at the crate root.
//!
//! This handler is intended to be installed as a [`Router::fallback`], so that
//! every request not matched by a more specific route (notably `/offer`) is
//! answered with the web client. Because fallback routes carry no captured path
//! parameters, it takes the raw [`Uri`] rather than an [`axum::extract::Path`];
//! the request URI is the only source of the path to serve.
//!
//! [`Router::fallback`]: axum::Router::fallback

use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::Response,
};

/// Vendored client assets, as `(path, bytes, Content-Type)`.
///
/// Paths are relative to `web/` and are matched against the normalised request
/// path. The bytes are embedded at compile time so the resulting binary is
/// self-contained.
const ASSETS: &[(&str, &[u8], &str)] = &[
    (
        "index.html",
        include_bytes!("../web/index.html"),
        "text/html; charset=utf-8",
    ),
    (
        "index.js",
        include_bytes!("../web/index.js"),
        "text/javascript; charset=utf-8",
    ),
    (
        "service-worker.js",
        include_bytes!("../web/service-worker.js"),
        "text/javascript; charset=utf-8",
    ),
    (
        "manifest.json",
        include_bytes!("../web/manifest.json"),
        "application/json",
    ),
    (
        "pico.classless.min.css",
        include_bytes!("../web/pico.classless.min.css"),
        "text/css; charset=utf-8",
    ),
    (
        "favicon.ico",
        include_bytes!("../web/favicon.ico"),
        "image/x-icon",
    ),
    ("icon.png", include_bytes!("../web/icon.png"), "image/png"),
    (
        "apple-touch-icon.png",
        include_bytes!("../web/apple-touch-icon.png"),
        "image/png",
    ),
    ("mouse.png", include_bytes!("../web/mouse.png"), "image/png"),
];

/// Serves one embedded web client asset.
///
/// The request path is normalised by trimming any leading slashes; an empty
/// path (the `/` root) maps to `index.html`. Unknown paths receive a `404 Not
/// Found`.
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    // The literal root path `/`, and any other slash-only path, normalises to
    // the empty string and serves the app shell.
    let name = if path.is_empty() { "index.html" } else { path };

    if let Some((_, bytes, content_type)) = ASSETS.iter().find(|(asset, ..)| *asset == name) {
        // The client ships its own service worker, so responses are explicitly
        // not cached by intermediaries.
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, *content_type)
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(*bytes))
            .unwrap_or_else(|_| internal_server_error());
    }

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from("Not found"))
        .unwrap_or_else(|_| internal_server_error())
}

/// Fallback response for the (practically impossible) case where building a
/// response fails. Constructed without the builder so it cannot fail itself.
fn internal_server_error() -> Response {
    let mut response = Response::new(Body::from("Internal server error"));
    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    response
}
