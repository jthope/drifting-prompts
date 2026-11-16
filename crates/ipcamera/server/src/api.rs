//! The server's JSON: the library for the web build, and the camera list for the player page.
//!
//! - `GET /api/library` is the whole library as a [`ClientCopy`]: every camera, group and view,
//!   with the cameras' passwords taken out and a list of which have one; and beside it `player`,
//!   where a tile's player comes from: this server under this machine's other name than the one
//!   the request used (`lib.rs` says why).
//! - `GET /api/status` is what a browser checks about once a second ([`Status`]): the library's
//!   revision, which goes up with every change, numbers that change with the snapshots and the
//!   recordings, which cameras record and what their PTZ says, and what watched cameras see.
//! - `POST /api/changes` takes one [`Change`] as JSON and makes it, with every rule the library
//!   keeps, answering with what it added ([`Applied`]). A refused change is a 400 whose body is
//!   the sentence that says why, in the desktop's words.
//! - `POST /api/snapshots` saves the newest picture of each camera it lists ([`SnapshotRequest`]),
//!   answering how many ([`SnapshotsTaken`]).
//! - `POST /api/recording` records the cameras it lists ([`RecordRequest`]), and
//!   `DELETE /api/recording` stops every recording; both answer how many ([`Recorded`]).
//! - `POST /api/cameras/<id>/ptz` moves the camera ([`PtzMove`]) for a second, unless sent again,
//!   and `POST /api/cameras/<id>/ptz/stop` stops it (`ptz.rs`).
//! - `POST /api/cameras/<id>/spotlight` switches a watched camera's spotlight
//!   ([`SpotlightRequest`]), through its watch (`streams.rs`).
//! - `POST /api/cameras/<id>/connect` tries a camera again whose session gave up for good: the
//!   web's Connect, pressed by someone, as the desktop's is (`streams.rs`).
//! - `POST /api/detect` tells which camera is at an address ([`DetectRequest`], [`DetectReply`]),
//!   as the desktop's camera form does, with the saved password of the camera the form edits.
//! - `GET /api/discover` searches the server's network for cameras (ONVIF's discovery, and
//!   devices that answer in RTSP), logging in to
//!   none, and answers those that answered (`FoundCamera`s), for the camera form's network search.
//! - `GET /api/cameras` is the cameras' ids and names, for the player page's list.
//!
//! Every refusal is a sentence that says what to do, which the web build shows as it is.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use ipcamera_core::{
    Applied, CameraConfig, CameraId, Change, ClientCopy, StoreError,
    api::{
        CameraStatus, DetectReply, DetectRequest, PtzMove, RecordRequest, Recorded,
        SnapshotRequest, SnapshotsTaken, SpotlightRequest, Status,
    },
    ptz::PtzVelocity,
};
use ipcamera_storage::{recording, snapshot};
use serde::Serialize;

use crate::Server;

/// What a request naming a camera that isn't there gets.
const NO_CAMERA: &str = "There is no camera with that id. Reload the page.";

/// A camera as the player lists it: what to ask `/live/<id>` for and what to call it.
#[derive(Serialize)]
pub(crate) struct Camera {
    id: String,
    name: String,
}

/// `GET /api/cameras`: every camera, in the library's order.
pub(crate) async fn cameras(State(server): State<Server>) -> Json<Vec<Camera>> {
    Json(
        server
            .library
            .cameras()
            .into_iter()
            .map(|camera| Camera {
                id: camera.id.to_string(),
                name: camera.name,
            })
            .collect(),
    )
}

/// The library as the web build gets it.
#[derive(Serialize)]
pub(crate) struct Library {
    #[serde(flatten)]
    copy: ClientCopy,
    /// Where a tile's player comes from, such as `http://127.0.0.1:8770`.
    player: String,
}

/// `GET /api/library`: the whole library, with no passwords, and where the players come from.
pub(crate) async fn library(State(server): State<Server>, headers: HeaderMap) -> Json<Library> {
    Json(Library {
        copy: server.library.snapshot().for_client(),
        player: player_address(&headers, server.port),
    })
}

/// This server under the other name for this machine than the one `headers` asked by, so a page
/// of the app opened as `localhost` shows players from `127.0.0.1`, and the other way round.
fn player_address(headers: &HeaderMap, port: u16) -> String {
    let asked_as_localhost = headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .is_some_and(|host| host.starts_with("localhost"));
    let name = if asked_as_localhost {
        "127.0.0.1"
    } else {
        "localhost"
    };
    format!("http://{name}:{port}")
}

/// `text` when it says something, `None` when it is empty: a status with nothing to report is
/// left out of the JSON.
fn said(text: String) -> Option<String> {
    Some(text).filter(|text| !text.is_empty())
}

/// `GET /api/status`: the revisions to check, and each camera's recording and PTZ. The folders'
/// listings are read off the runtime's threads.
pub(crate) async fn status(State(server): State<Server>) -> Response {
    let revision = server.library.revision();
    let (snapshots, videos) = (server.config.snapshots_dir(), server.config.videos_dir());
    let Ok((snapshots, videos)) = tokio::task::spawn_blocking(move || {
        (snapshot::revision(&snapshots), recording::revision(&videos))
    })
    .await
    else {
        return stopped();
    };

    let mut cameras: HashMap<CameraId, CameraStatus> = HashMap::new();
    for (id, recording) in server.streams.recordings() {
        let camera = cameras.entry(id.clone()).or_insert_with(|| CameraStatus {
            id,
            ..CameraStatus::default()
        });
        camera.recording = recording.on;
        camera.record_problem = said(recording.problem);
    }
    for (id, problem) in server.streams.states() {
        let camera = cameras.entry(id.clone()).or_insert_with(|| CameraStatus {
            id,
            ..CameraStatus::default()
        });
        camera.playing = problem.is_none();
        camera.stream_problem = problem;
    }
    for (id, ptz_status) in server.ptz.statuses() {
        cameras
            .entry(id.clone())
            .or_insert_with(|| CameraStatus {
                id,
                ..CameraStatus::default()
            })
            .ptz_status = said(ptz_status);
    }
    for (id, watched) in server.streams.watched() {
        cameras
            .entry(id.clone())
            .or_insert_with(|| CameraStatus {
                id,
                ..CameraStatus::default()
            })
            .watch = Some(watched.status());
    }
    let mut cameras: Vec<CameraStatus> = cameras.into_values().collect();
    cameras.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    Json(Status {
        revision,
        snapshots,
        videos,
        cameras,
    })
    .into_response()
}

/// `POST /api/snapshots`: saves the newest picture of each listed camera that can, connecting to
/// one nobody watches, and answers how many, as recording does. When none can, the first one's
/// reason is the answer. Each waits for a picture and decodes it off the runtime's threads, all
/// of them at once, so the pictures are of the same moment and the answer doesn't wait for them
/// one by one.
pub(crate) async fn snapshots(
    State(server): State<Server>,
    Json(request): Json<SnapshotRequest>,
) -> Response {
    let folder = server.config.snapshots_dir();
    let takers: Vec<_> = request
        .cameras
        .iter()
        .map(|id| {
            let camera = server.library.camera(id);
            let streams = server.streams.clone();
            let folder = folder.clone();
            tokio::task::spawn_blocking(move || {
                let camera = camera.ok_or_else(|| NO_CAMERA.to_string())?;
                streams.snapshot(&camera, folder)
            })
        })
        .collect();

    let mut taken = 0;
    let mut first_problem = None;
    for taker in takers {
        match taker.await {
            Ok(Ok(_)) => taken += 1,
            Ok(Err(why)) => {
                first_problem.get_or_insert(why);
            }
            Err(_) => return stopped(),
        }
    }
    if taken > 0 {
        Json(SnapshotsTaken { count: taken }).into_response()
    } else {
        let why = first_problem
            .unwrap_or_else(|| "There is no camera on screen to take a snapshot of.".into());
        (StatusCode::BAD_REQUEST, why).into_response()
    }
}

/// `POST /api/recording`: records each listed camera that can, answering how many. When none
/// can, the first one's reason is the answer.
pub(crate) async fn record(
    State(server): State<Server>,
    Json(request): Json<RecordRequest>,
) -> Response {
    let mut started = 0;
    let mut first_problem = None;
    for id in &request.cameras {
        let recorded = server
            .library
            .camera(id)
            .ok_or_else(|| NO_CAMERA.to_string())
            .and_then(|camera| server.streams.record(&camera));
        match recorded {
            Ok(()) => started += 1,
            Err(why) => {
                first_problem.get_or_insert(why);
            }
        }
    }
    if started > 0 {
        Json(Recorded { count: started }).into_response()
    } else {
        let why = first_problem.unwrap_or_else(|| "There is no camera on screen to record.".into());
        (StatusCode::BAD_REQUEST, why).into_response()
    }
}

/// `DELETE /api/recording`: stops every recording, answering how many there were.
pub(crate) async fn stop_recording(State(server): State<Server>) -> Json<Recorded> {
    Json(Recorded {
        count: server.streams.stop_recording(),
    })
}

/// `POST /api/cameras/<id>/ptz`: moves the camera for a second, unless sent again.
pub(crate) async fn ptz_move(
    State(server): State<Server>,
    Path(id): Path<String>,
    Json(request): Json<PtzMove>,
) -> Response {
    let Some(camera) = camera(&server, &id) else {
        return (StatusCode::NOT_FOUND, NO_CAMERA).into_response();
    };
    let speed = request.speed.clamp(0.0, 1.0);
    let velocity = PtzVelocity::new(
        request.pan.clamp(-1.0, 1.0) * speed,
        request.tilt.clamp(-1.0, 1.0) * speed,
        request.zoom.clamp(-1.0, 1.0) * speed,
    );
    match server.ptz.move_by(&camera, velocity, Instant::now()) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(why) => (StatusCode::BAD_REQUEST, why).into_response(),
    }
}

/// `POST /api/cameras/<id>/ptz/stop`: stops the camera's move.
pub(crate) async fn ptz_stop(State(server): State<Server>, Path(id): Path<String>) -> StatusCode {
    server.ptz.stop(&CameraId::from(id));
    StatusCode::NO_CONTENT
}

/// `POST /api/cameras/<id>/connect`: lets the camera be tried again, after its session gave up or
/// though it has no saved password. Its viewers connect again by themselves.
pub(crate) async fn connect(State(server): State<Server>, Path(id): Path<String>) -> Response {
    let Some(camera) = camera(&server, &id) else {
        return (StatusCode::NOT_FOUND, NO_CAMERA).into_response();
    };
    server.streams.try_again(&camera);
    StatusCode::NO_CONTENT.into_response()
}

/// How long a spotlight switch may take: a login and a request, each of which can take ten
/// seconds.
const SPOTLIGHT_ANSWER: Duration = Duration::from_secs(25);

/// `POST /api/cameras/<id>/spotlight`: switches the camera's spotlight through its watch, which
/// runs while its stream does.
pub(crate) async fn spotlight(
    State(server): State<Server>,
    Path(id): Path<String>,
    Json(request): Json<SpotlightRequest>,
) -> Response {
    if camera(&server, &id).is_none() {
        return (StatusCode::NOT_FOUND, NO_CAMERA).into_response();
    }
    let answer = match server
        .streams
        .set_spotlight(&CameraId::from(id), request.on)
    {
        Ok(answer) => answer,
        Err(why) => return (StatusCode::BAD_REQUEST, why).into_response(),
    };
    let done = tokio::task::spawn_blocking(move || {
        answer
            .recv_timeout(SPOTLIGHT_ANSWER)
            .map_err(|_| "The camera didn't answer. Try again.".to_string())
            .and_then(|done| done)
    })
    .await;
    match done {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(why)) => (StatusCode::BAD_REQUEST, why).into_response(),
        Err(_) => stopped(),
    }
}

/// `POST /api/detect`: which camera is at the request's address, asked as the desktop's form asks.
/// A form that edits a saved camera and leaves its password empty, as the web build's does, logs
/// in with the saved one. It takes a few seconds, off the runtime's threads.
pub(crate) async fn detect(
    State(server): State<Server>,
    Json(request): Json<DetectRequest>,
) -> Response {
    let mut setting = request.setting;
    if setting.password.is_empty()
        && let Some(saved) = request.camera.and_then(|id| server.library.camera(&id))
    {
        setting.password = saved.password;
    }
    let found = tokio::task::spawn_blocking(move || {
        ipcamera_core::detect(&setting).map(|mut found| {
            // Then the size of each RTSP stream nothing above sized, which Auto picks by.
            let asked = CameraConfig {
                brand: found.brand,
                ..setting.clone()
            };
            let sized = ipcamera_media::sized_streams(&asked, &found.streams);
            ipcamera_core::CameraStream::merge(&mut found.streams, sized);
            reply_of(found, &setting)
        })
    })
    .await;
    match found {
        Ok(Ok(reply)) => Json(reply).into_response(),
        Ok(Err(why)) => (StatusCode::BAD_REQUEST, why).into_response(),
        Err(_) => stopped(),
    }
}

/// What `POST /api/detect` answers with, from what Detect found asking with `setting`.
fn reply_of(found: ipcamera_core::Detected, setting: &CameraConfig) -> DetectReply {
    DetectReply {
        brand: found.brand,
        api_port: found.api.map(|(port, _)| port),
        api_https: found.api.map(|(_, https)| https),
        http_api: found.http_api,
        device: found.device,
        features: found.features,
        asked_with_password: !setting.password.is_empty(),
        onvif_events: found.onvif_events,
        onvif_port: found.onvif_port,
        onvif_problem: found.onvif_problem,
        streams: found.streams,
        onvif_refused: found.onvif_refused,
    }
}

/// `GET /api/discover`: the cameras on the server's network, as the desktop's camera form
/// finds them on its own. It takes about three seconds, off the runtime's threads.
pub(crate) async fn discover() -> Response {
    match tokio::task::spawn_blocking(ipcamera_core::find_cameras).await {
        Ok(found) => Json(found).into_response(),
        Err(_) => stopped(),
    }
}

/// The camera called `id`, as saved now.
fn camera(server: &Server, id: &str) -> Option<CameraConfig> {
    server.library.camera(&CameraId::from(id))
}

/// What a request gets when the server stopped while it was being answered.
fn stopped() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "The server stopped before it was done. Try it again.",
    )
        .into_response()
}

/// `POST /api/changes`: makes one change. A camera it changes or deletes stops streaming, so its
/// viewers reconnect to it as it is now, and its PTZ connection closes; one that records carries
/// on into a new file.
pub(crate) async fn change(State(server): State<Server>, Json(change): Json<Change>) -> Response {
    // Saving writes a file, so it waits off the runtime's threads.
    let library = server.library.clone();
    let made = {
        let change = change.clone();
        tokio::task::spawn_blocking(move || library.apply(&change)).await
    };
    match made {
        Ok(Ok(applied)) => {
            let changed: Vec<CameraId> = match &change {
                Change::UpdateCameras { cameras } => {
                    cameras.iter().map(|update| update.id.clone()).collect()
                }
                Change::DeleteCamera { id } => vec![id.clone()],
                _ => Vec::new(),
            };
            for id in changed {
                server.ptz.forget(&id);
                server
                    .streams
                    .restart(&id, server.library.camera(&id).as_ref());
            }
            Json::<Applied>(applied).into_response()
        }
        Ok(Err(error)) => refused(&error).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "The server stopped while saving the change. Reload, and try it again.",
        )
            .into_response(),
    }
}

/// A refused change: the user's mistake is a 400, a file that couldn't be written a 500, and the
/// body says why either way.
fn refused(error: &StoreError) -> (StatusCode, String) {
    let status = match error {
        StoreError::NotFound(_) | StoreError::Invalid(_) => StatusCode::BAD_REQUEST,
        StoreError::ReadOnly { .. } | StoreError::Write(_) | StoreError::Serialize(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    (status, error.to_string())
}
