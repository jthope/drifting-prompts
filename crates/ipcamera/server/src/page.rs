//! The player page and its script, built into the server so there is nothing beside it to lose.
//!
//! They are in `player/`: plain HTML and JavaScript, with no build step, that decode with the
//! browser's own WebCodecs. `no-cache` makes a browser ask again each time, so a new server's page
//! is never hidden behind an old copy.

use axum::{
    http::header,
    response::{Html, IntoResponse},
};

/// `GET /`: the player page.
pub(crate) async fn index() -> impl IntoResponse {
    (
        [(header::CACHE_CONTROL, "no-cache")],
        Html(include_str!("../player/index.html")),
    )
}

/// `GET /player.js`: the page's script.
pub(crate) async fn script() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("../player/player.js"),
    )
}
