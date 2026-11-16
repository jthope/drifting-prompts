//! The IP Camera server: the cameras, their groups and views, and their live video, for a
//! browser, with no desktop app.
//!
//! A browser can't open the TCP connections RTSP needs, so the server does it for them. It keeps
//! the library of cameras (`core`'s [`Library`], the same files the desktop app keeps in
//! `data\settings`), relays each camera that someone watches with a relaying
//! [`MediaSession`](ipcamera_media::MediaSession), and passes the video on over a WebSocket. It
//! decodes nothing: the browser's own WebCodecs decoder does, in the player page the server serves.
//! The web build of the app is its client: it reads the library from here and sends every change
//! here, and its tiles show the player. In C# terms, this is a small ASP.NET Core host over the
//! app's class libraries.
//!
//! It also does what the desktop does with a camera beyond showing it, in the same files and by
//! the same rules: it takes snapshots and records, into the folders its `settings.json` names, as
//! the desktop app's does, and moves PTZ cameras.
//!
//! What it answers:
//!
//! - `GET /`: the web build of the app, when one is in the app folder ([`Options::app`]); the
//!   camera list otherwise.
//! - `GET /watch`: the player page, which lists the cameras, or with `?camera=<id>` plays one, and
//!   with `&stream=<key>` the stream a tile picks in place of the camera's own.
//! - `GET /player.js`: the player page's script.
//! - `GET /api/library`, `GET /api/status`, `POST /api/changes`, `GET /api/cameras`: the library
//!   and what has changed, as `api.rs` says. No password ever leaves the server.
//! - `POST /api/snapshots`, `POST` and `DELETE /api/recording`, and `POST /api/cameras/<id>/ptz`
//!   and `…/ptz/stop`: snapshots, recording and PTZ (`api.rs`, `streams.rs`, `ptz.rs`).
//! - `GET /api/snapshots`, `GET /api/videos`, `DELETE` either's `/<name>`, `GET /files/…` and
//!   `GET /api/storage`: the saved files and their folders (`files.rs`).
//! - `GET /live/<id>`: a WebSocket of one camera's video, or with `?stream=<key>` of another of
//!   its streams. `live.rs` says what goes over it.
//! - `POST /api/cameras/<id>/connect`: tries a camera again whose session gave up for good.
//! - `GET`, `POST` and `DELETE /api/session`: signing in, for a server with a password
//!   (`access.rs`).
//!
//! A camera's session starts when its first viewer comes, a snapshot is asked for or a recording
//! starts, and stops a while after nothing needs it (`streams.rs`), so a camera nobody watches is
//! never connected to. One with no saved password is never connected to at all.
//!
//! As the server ends it stops every camera that is moving and finishes every recording, and
//! waits for both before it returns (`closing.rs`).
//!
//! **Sharing the desktop app's files.** `--settings` at the desktop app's `settings.json` has both
//! work on the same cameras, groups and views. Each reads a file again before it changes it, when
//! the other has written it since (`core`'s `file_store.rs`), so neither overwrites the other's
//! change, and the server looks every few seconds for what the desktop changed.
//!
//! **Only this machine.** The server listens on 127.0.0.1 and turns away any request that names
//! another host, or that comes from a page of another origin than its own and those
//! [`Options::allow_origins`] names. That shuts out other machines, and a web page elsewhere that
//! tries to reach the server through the browser (DNS rebinding).
//!
//! **A password, when one is set.** `ipcamera-server --set-password` gives the server a password,
//! after which it answers nothing but its pages' own files until someone signs in (`access.rs`):
//! that keeps out the other programs and users of this machine. With none set it asks for none.
//! Listening on the network still needs HTTPS first, since the password would cross it in the
//! clear.
//!
//! **The player in a tile.** The web build shows each tile's player in a frame. nsh takes the
//! origin away from a frame that shows a page of the app's own origin, and the browser then
//! refuses that frame's connections to this machine. So the player comes from this machine under
//! its other name: `127.0.0.1` when the app was opened as `localhost`, and the other way round
//! (`api.rs`). The frame keeps that origin, which is this server's own.

mod access;
mod api;
mod closing;
mod files;
mod live;
mod page;
mod ptz;
mod streams;

use std::{
    future::Future,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use ipcamera_core::{Library, config::ConfigStore};
use tokio::net::TcpListener;
use tower_http::{cors::CorsLayer, services::ServeDir};

pub use crate::access::hash_password;
use crate::{access::Access, ptz::Ptz, streams::Streams};

/// How often the sessions nobody watches are looked for.
const SWEEP_EVERY: Duration = Duration::from_secs(5);

/// How often PTZ moves that have run out are stopped: often enough that a lost stop costs little
/// more than [`ptz::MOVE_LASTS`].
const PTZ_CHECK_EVERY: Duration = Duration::from_millis(200);

/// How long the server waits, as it ends, for the cameras' PTZ stops to go out and their
/// recordings' files to finish. Both take a moment with a camera that answers; this is for one
/// that doesn't, which must not keep the server from ending.
const CLOSE_WITHIN: Duration = Duration::from_secs(8);

/// How the server is run, beyond its library and its port.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The packaged web build of the app (`nsh package --target web`), served at `/`. `None`, or a
    /// folder with no `index.html`, serves the camera list there instead.
    pub app: Option<PathBuf>,
    /// More origins whose pages may use the server, such as `http://localhost:8787`, where
    /// `nsh run --target web` serves the app while it is worked on. The server answers them
    /// across origins (CORS); every other origin's pages are turned away.
    pub allow_origins: Vec<String>,
}

/// What every request handler shares.
#[derive(Clone)]
struct Server {
    library: Arc<Library>,
    /// `settings.json`: where snapshots and recordings go.
    config: Arc<ConfigStore>,
    streams: Arc<Streams>,
    ptz: Arc<Ptz>,
    /// The server's password, when it has one, and who has signed in.
    access: Arc<Access>,
    /// The port the server listens on, which a request's `Host` has to name.
    port: u16,
    /// The other origins whose pages may use the server.
    allow_origins: Arc<[String]>,
}

/// Serves the cameras of `library` on `listener`, as `options` say, until `shutdown` finishes.
/// Snapshots and recordings go where `settings` (a `settings.json`) says, as the desktop app's do.
///
/// `listener` should be on 127.0.0.1: the server answers only requests addressed to this machine,
/// and its password, when `settings` holds one, is not sent in a way fit for a network.
pub async fn serve(
    listener: TcpListener,
    library: Library,
    config: ConfigStore,
    options: Options,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let config = Arc::new(config);
    let server = Server {
        library: Arc::new(library),
        streams: Arc::new(Streams::new(config.clone())),
        config: config.clone(),
        ptz: Arc::new(Ptz::default()),
        access: Arc::new(Access::new(config.current().server.password)),
        port: listener.local_addr()?.port(),
        allow_origins: options.allow_origins.clone().into(),
    };

    let sweeper = {
        let server = server.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(SWEEP_EVERY);
            loop {
                tick.tick().await;
                // The desktop app may share the files: what it changed since the last look is
                // taken here, and the cameras it changed stop, as after a change made here.
                if server.library.refresh() {
                    server.ptz.follow(&server.library);
                    server.streams.follow(&server.library);
                }
                server.streams.sweep(&server.library);
            }
        })
    };
    let ptz_checker = {
        let ptz = server.ptz.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(PTZ_CHECK_EVERY);
            loop {
                tick.tick().await;
                ptz.expire(Instant::now());
            }
        })
    };

    // Everything that tells or changes something needs a session, when the server has a password.
    let routes = Router::new()
        .route("/api/cameras", get(api::cameras))
        .route("/api/library", get(api::library))
        .route("/api/status", get(api::status))
        .route("/api/changes", post(api::change))
        .route("/api/cameras/{id}/ptz", post(api::ptz_move))
        .route("/api/cameras/{id}/ptz/stop", post(api::ptz_stop))
        .route("/api/cameras/{id}/spotlight", post(api::spotlight))
        .route("/api/cameras/{id}/connect", post(api::connect))
        .route("/api/detect", post(api::detect))
        .route("/api/discover", get(api::discover))
        .route(
            "/api/recording",
            post(api::record).delete(api::stop_recording),
        )
        .route("/api/snapshots", get(files::snapshots).post(api::snapshots))
        .route("/api/snapshots/{name}", delete(files::delete_snapshot))
        .route("/api/videos", get(files::videos))
        .route("/api/videos/{name}", delete(files::delete_video))
        .route("/api/storage", get(files::storage))
        .route("/files/snapshots/{name}", get(files::snapshot_file))
        .route(
            "/files/snapshots/thumbnails/{name}",
            get(files::snapshot_thumbnail),
        )
        .route("/files/videos/{name}", get(files::video_file))
        .route(
            "/files/videos/thumbnails/{name}",
            get(files::video_thumbnail),
        )
        .route("/live/{id}", get(live::live))
        .route_layer(middleware::from_fn_with_state(
            server.clone(),
            access::signed_in_only,
        ))
        // The pages' own files tell nothing, and signing in has to be reachable before it.
        .route("/watch", get(page::index))
        .route("/player.js", get(page::script))
        .route(
            "/api/session",
            get(access::session)
                .post(access::sign_in)
                .delete(access::sign_out),
        );
    let routes = match options.app.filter(|app| app.join("index.html").is_file()) {
        Some(app) => routes.fallback_service(ServeDir::new(app)),
        None => routes.route("/", get(page::index)),
    };
    let mut app = routes.with_state(server.clone());
    let origins: Vec<HeaderValue> = options
        .allow_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();
    if !origins.is_empty() {
        app = app.layer(
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods([Method::GET, Method::POST, Method::DELETE])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]),
        );
    }
    // Outermost, so a request from anywhere else is turned away before anything answers it.
    let app = app.layer(middleware::from_fn_with_state(
        server.clone(),
        this_machine_only,
    ));

    let served = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await;

    sweeper.abort();
    ptz_checker.abort();
    server.ptz.stop_all();
    server.streams.stop_all();
    // Each of those closes on a thread of its own, and the process ending would take the threads
    // with it: a camera left turning, a recording with no length. So the server waits for them,
    // off the runtime's threads, which the viewers still need to let go of their streams.
    let closing = server.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let until = Instant::now() + CLOSE_WITHIN;
        closing.ptz.stopped_by(until);
        closing.streams.stopped_by(until);
    })
    .await;
    served
}

/// Turns away a request that names a host other than this machine's address and port, or that
/// comes from a page another server served.
async fn this_machine_only(State(server): State<Server>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    if host_here(headers, server.port)
        && origin_allowed(headers, server.port, &server.allow_origins)
    {
        let mut response = next.run(request).await;
        // A session's key can be in a page's address (`access.rs`), which must not travel on in a
        // `Referer` to wherever the page leads.
        response.headers_mut().insert(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        );
        response
    } else {
        (
            StatusCode::FORBIDDEN,
            "This server answers only its own pages, on this machine.",
        )
            .into_response()
    }
}

/// Whether `host` (`Host`'s value, or an origin's after `http://`) is this machine on `port`.
fn this_machine(host: &str, port: u16) -> bool {
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

/// Whether `Host` names this machine on `port`.
fn host_here(headers: &HeaderMap, port: u16) -> bool {
    headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .is_some_and(|host| this_machine(host, port))
}

/// Whether any `Origin` is this server's own pages or one of `allowed`.
///
/// Browsers send `Origin` with every WebSocket and with a request from another origin's page, but
/// not when a page simply loads, so a missing one is fine.
fn origin_allowed(headers: &HeaderMap, port: u16, allowed: &[String]) -> bool {
    headers.get(header::ORIGIN).is_none_or(|origin| {
        origin.to_str().ok().is_some_and(|origin| {
            allowed.iter().any(|allowed| allowed == origin)
                || origin
                    .strip_prefix("http://")
                    .is_some_and(|host| this_machine(host, port))
        })
    })
}

/// Whether `Host` names this machine on `port`, and any `Origin` is this server's own pages or
/// one of `allowed`.
#[cfg(test)]
fn addressed_here(headers: &HeaderMap, port: u16, allowed: &[String]) -> bool {
    host_here(headers, port) && origin_allowed(headers, port, allowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(name.clone(), HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    #[test]
    fn only_this_machine_and_its_own_pages_get_in() {
        let port = 8770;
        let allowed = [
            headers(&[(header::HOST, "127.0.0.1:8770")]),
            headers(&[(header::HOST, "localhost:8770")]),
            headers(&[
                (header::HOST, "127.0.0.1:8770"),
                (header::ORIGIN, "http://127.0.0.1:8770"),
            ]),
        ];
        for allowed in allowed {
            assert!(addressed_here(&allowed, port, &[]), "{allowed:?}");
        }
        let refused = [
            headers(&[]),
            headers(&[(header::HOST, "camera.example:8770")]),
            headers(&[(header::HOST, "127.0.0.1:9000")]),
            headers(&[
                (header::HOST, "127.0.0.1:8770"),
                (header::ORIGIN, "http://camera.example"),
            ]),
            headers(&[(header::HOST, "127.0.0.1:8770"), (header::ORIGIN, "null")]),
            headers(&[
                (header::HOST, "127.0.0.1:8770"),
                (header::ORIGIN, "http://127.0.0.1:8787"),
            ]),
        ];
        for refused in refused {
            assert!(!addressed_here(&refused, port, &[]), "{refused:?}");
        }
    }

    #[test]
    fn the_allowed_origin_gets_in_and_no_other() {
        let allowed = ["http://127.0.0.1:8787".to_string()];
        let from =
            |origin: &str| headers(&[(header::HOST, "127.0.0.1:8770"), (header::ORIGIN, origin)]);
        assert!(addressed_here(
            &from("http://127.0.0.1:8787"),
            8770,
            &allowed
        ));
        assert!(!addressed_here(
            &from("http://127.0.0.1:8788"),
            8770,
            &allowed
        ));
        assert!(!addressed_here(
            &from("http://camera.example"),
            8770,
            &allowed
        ));
    }
}
