//! The IP Camera server: the cameras' library and their live video, for a browser.
//!
//! It answers only this machine and its own pages (against DNS rebinding), and no camera's password
//! ever leaves it.

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

const SWEEP_EVERY: Duration = Duration::from_secs(5);

/// Often enough that a lost stop costs little more than [`ptz::MOVE_LASTS`].
const PTZ_CHECK_EVERY: Duration = Duration::from_millis(200);

/// A camera that doesn't answer must not keep the server from ending.
const CLOSE_WITHIN: Duration = Duration::from_secs(8);

/// How the server is run, beyond its library and its port.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The packaged web build, served at `/`; without its `index.html`, the camera list is.
    pub app: Option<PathBuf>,
    /// More origins whose pages may use the server (CORS), such as `http://localhost:8787`.
    pub allow_origins: Vec<String>,
}

#[derive(Clone)]
struct Server {
    library: Arc<Library>,
    config: Arc<ConfigStore>,
    streams: Arc<Streams>,
    ptz: Arc<Ptz>,
    access: Arc<Access>,
    port: u16,
    allow_origins: Arc<[String]>,
}

/// Serves `library` on `listener` until `shutdown` finishes. `listener` must be on 127.0.0.1: the
/// password, when there is one, is not sent in a way fit for a network.
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
                // The desktop app may share the files and change them.
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

    // The routes above `route_layer` need a session when the server has a password.
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
    // Ending without waiting would leave a camera turning or a recording with no length. Wait off
    // the runtime's threads, which the viewers still need to let go of their streams.
    let closing = server.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let until = Instant::now() + CLOSE_WITHIN;
        closing.ptz.stopped_by(until);
        closing.streams.stopped_by(until);
    })
    .await;
    served
}

async fn this_machine_only(State(server): State<Server>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    if host_here(headers, server.port)
        && origin_allowed(headers, server.port, &server.allow_origins)
    {
        let mut response = next.run(request).await;
        // A session's key can be in a page's address, which must not travel on in a `Referer`.
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

fn this_machine(host: &str, port: u16) -> bool {
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

fn host_here(headers: &HeaderMap, port: u16) -> bool {
    headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .is_some_and(|host| this_machine(host, port))
}

/// A missing `Origin` is fine: browsers leave it out when a page simply loads.
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
