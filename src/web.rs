//! Small, buildless frontend embedded in the binary for single-file deployment.
use axum::{
    http::header::{CACHE_CONTROL, CONTENT_TYPE},
    response::{Html, IntoResponse},
};

pub async fn index() -> impl IntoResponse {
    (
        [(CACHE_CONTROL, "no-cache")],
        Html(include_str!("../web/index.html")),
    )
}

pub async fn stylesheet() -> impl IntoResponse {
    (
        [
            (CONTENT_TYPE, "text/css; charset=utf-8"),
            (CACHE_CONTROL, "no-cache"),
        ],
        include_str!("../web/app.css"),
    )
}

pub async fn javascript() -> impl IntoResponse {
    (
        [
            (CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (CACHE_CONTROL, "no-cache"),
        ],
        include_str!("../web/app.js"),
    )
}

pub async fn favicon() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "image/svg+xml"), (CACHE_CONTROL, "no-cache")],
        include_str!("../web/favicon.svg"),
    )
}
