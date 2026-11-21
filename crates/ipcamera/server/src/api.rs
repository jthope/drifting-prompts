//! The server's JSON: the library, its status and changes, snapshots, recording, PTZ and Detect.
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

const NO_CAMERA: &str = "There is no camera with that id. Reload the page.";

#[derive(Serialize)]
pub(crate) struct Camera {
    id: String,
    name: String,
}

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

#[derive(Serialize)]
pub(crate) struct Library {
    #[serde(flatten)]
    copy: ClientCopy,
    player: String,
}

pub(crate) async fn library(State(server): State<Server>, headers: HeaderMap) -> Json<Library> {
    Json(Library {
        copy: server.library.snapshot().for_client(),
        player: player_address(&headers, server.port),
    })
}

/// The other name for this machine than the request used: a frame showing a page of the app's own
/// origin loses its origin, and the browser then refuses its connections to this machine.
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

fn said(text: String) -> Option<String> {
    Some(text).filter(|text| !text.is_empty())
}

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

/// Decodes every camera's picture off the runtime's threads at once, so they show the same moment.
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

pub(crate) async fn stop_recording(State(server): State<Server>) -> Json<Recorded> {
    Json(Recorded {
        count: server.streams.stop_recording(),
    })
}

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

pub(crate) async fn ptz_stop(State(server): State<Server>, Path(id): Path<String>) -> StatusCode {
    server.ptz.stop(&CameraId::from(id));
    StatusCode::NO_CONTENT
}

pub(crate) async fn connect(State(server): State<Server>, Path(id): Path<String>) -> Response {
    let Some(camera) = camera(&server, &id) else {
        return (StatusCode::NOT_FOUND, NO_CAMERA).into_response();
    };
    server.streams.try_again(&camera);
    StatusCode::NO_CONTENT.into_response()
}

/// A login and a request, each of which can take ten seconds.
const SPOTLIGHT_ANSWER: Duration = Duration::from_secs(25);

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

/// An empty password on a saved camera's form logs in with the saved one.
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

pub(crate) async fn discover() -> Response {
    match tokio::task::spawn_blocking(ipcamera_core::find_cameras).await {
        Ok(found) => Json(found).into_response(),
        Err(_) => stopped(),
    }
}

fn camera(server: &Server, id: &str) -> Option<CameraConfig> {
    server.library.camera(&CameraId::from(id))
}

fn stopped() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "The server stopped before it was done. Try it again.",
    )
        .into_response()
}

pub(crate) async fn change(State(server): State<Server>, Json(change): Json<Change>) -> Response {
    // Saving writes a file, so off the runtime's threads.
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

fn refused(error: &StoreError) -> (StatusCode, String) {
    let status = match error {
        StoreError::NotFound(_) | StoreError::Invalid(_) => StatusCode::BAD_REQUEST,
        StoreError::ReadOnly { .. } | StoreError::Write(_) | StoreError::Serialize(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    (status, error.to_string())
}
