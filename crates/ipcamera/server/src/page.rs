//! The player page and its script, built into the server.
//!
//! `no-cache`, so a new server's page is never hidden behind an old copy.

use axum::{
    http::header,
    response::{Html, IntoResponse},
};

pub(crate) async fn index() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-cache")],
        Html(include_str!("../player/index.html")),
    )
}

pub(crate) async fn script() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("../player/player.js"),
    )
}
