//! The saved snapshots and recordings, served for the web build's pages, and their folders.
//!
//! Only a file of its kind directly in its folder is served, so a name never reaches another.

use std::path::PathBuf;

use axum::{
    Json,
    body::Body,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use ipcamera_core::{
    CameraId,
    api::{
        PixelSize, SnapshotEntry, Storage, StorageEntry, VideoEntry, path_segment as url_encoded,
    },
};
use ipcamera_storage::{
    StorageError, recording,
    saved_file::{FileKind, SNAPSHOT, VIDEO, now_ms},
    snapshot,
    space::space_under,
    thumbnail,
};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::Server;

const STILL_RECORDING: &str = "That video is still recording. Stop recording first.";

pub(crate) async fn snapshots(State(server): State<Server>) -> Response {
    let folder = server.config.snapshots_dir();
    blocking(move || {
        let listed = snapshot::list(&folder);
        if listed.iter().any(|saved| saved.thumbnail.is_none()) {
            snapshot::fill_thumbnails_soon(folder);
        }
        listed
            .into_iter()
            .map(|saved| SnapshotEntry {
                url: format!("/files/snapshots/{}", url_encoded(&saved.name)),
                thumbnail: thumbnail_url("snapshots", saved.thumbnail.as_deref()),
                camera: saved.camera.map(CameraId::from),
                tags: saved.tags,
                taken_ms: saved.taken_ms,
                bytes: saved.bytes,
                size: saved
                    .size
                    .map(|(width, height)| PixelSize { width, height }),
                name: saved.name,
            })
            .collect::<Vec<_>>()
    })
    .await
}

fn thumbnail_url(kind: &str, path: Option<&std::path::Path>) -> Option<String> {
    let name = path?.file_name()?;
    Some(format!(
        "/files/{kind}/thumbnails/{}",
        url_encoded(&name.to_string_lossy())
    ))
}

pub(crate) async fn videos(State(server): State<Server>) -> Response {
    let folder = server.config.videos_dir();
    let writing = server.streams.writing();
    blocking(move || {
        let now = now_ms();
        recording::list(&folder)
            .into_iter()
            .map(|saved| {
                let recording = writing.contains(&saved.name);
                VideoEntry {
                    url: format!("/files/videos/{}", url_encoded(&saved.name)),
                    thumbnail: thumbnail_url("videos", saved.thumbnail.as_deref()),
                    // One still being written runs to now; the file's time can lag behind.
                    length_ms: if recording {
                        now.saturating_sub(saved.started_ms)
                    } else {
                        saved.length_ms()
                    },
                    recording,
                    camera: saved.camera.map(CameraId::from),
                    tags: saved.tags,
                    started_ms: saved.started_ms,
                    bytes: saved.bytes,
                    name: saved.name,
                }
            })
            .collect::<Vec<_>>()
    })
    .await
}

pub(crate) async fn snapshot_file(
    State(server): State<Server>,
    Path(name): Path<String>,
    request: Request,
) -> Response {
    serve(&SNAPSHOT, server.config.snapshots_dir(), &name, request).await
}

pub(crate) async fn video_file(
    State(server): State<Server>,
    Path(name): Path<String>,
    request: Request,
) -> Response {
    serve(&VIDEO, server.config.videos_dir(), &name, request).await
}

pub(crate) async fn snapshot_thumbnail(
    State(server): State<Server>,
    Path(name): Path<String>,
    request: Request,
) -> Response {
    serve_thumbnail(server.config.snapshots_dir(), &name, request).await
}

pub(crate) async fn video_thumbnail(
    State(server): State<Server>,
    Path(name): Path<String>,
    request: Request,
) -> Response {
    serve_thumbnail(server.config.videos_dir(), &name, request).await
}

async fn serve_thumbnail(folder: PathBuf, name: &str, request: Request) -> Response {
    match thumbnail::locate(&folder, name) {
        Ok(path) => send(path, request).await,
        Err(why) => (StatusCode::NOT_FOUND, why.to_string()).into_response(),
    }
}

async fn serve(kind: &FileKind, folder: PathBuf, name: &str, request: Request) -> Response {
    match kind.locate(&folder, name) {
        Ok(path) => send(path, request).await,
        Err(why) => (StatusCode::NOT_FOUND, why.to_string()).into_response(),
    }
}

async fn send(path: PathBuf, request: Request) -> Response {
    match ServeFile::new(path).oneshot(request).await {
        Ok(response) => response.map(Body::new),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

pub(crate) async fn delete_snapshot(
    State(server): State<Server>,
    Path(name): Path<String>,
) -> Response {
    let folder = server.config.snapshots_dir();
    deleted(blocking_result(move || snapshot::delete(&folder, &name)).await)
}

pub(crate) async fn delete_video(
    State(server): State<Server>,
    Path(name): Path<String>,
) -> Response {
    if server.streams.writing().contains(&name) {
        return (StatusCode::CONFLICT, STILL_RECORDING).into_response();
    }
    let folder = server.config.videos_dir();
    deleted(blocking_result(move || recording::delete(&folder, &name)).await)
}

fn deleted(result: Option<Result<(), StorageError>>) -> Response {
    match result {
        Some(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Some(Err(why)) => (StatusCode::BAD_REQUEST, why.to_string()).into_response(),
        None => stopped().into_response(),
    }
}

pub(crate) async fn storage(State(server): State<Server>) -> Response {
    let config = server.config.clone();
    blocking(move || Storage {
        snapshots: entry(config.snapshots_dir(), config.default_snapshots_dir()),
        videos: entry(config.videos_dir(), config.default_videos_dir()),
    })
    .await
}

fn entry(folder: PathBuf, default: PathBuf) -> StorageEntry {
    let mut entry = StorageEntry {
        is_default: folder == default,
        path: ipcamera_storage::saved_file::absolute(&folder)
            .display()
            .to_string(),
        ..StorageEntry::default()
    };
    match space_under(&folder) {
        Ok(space) => {
            entry.drive = space.drive;
            entry.free = space.free;
            entry.total = space.total;
        }
        Err(why) => entry.problem = Some(why.to_string()),
    }
    entry
}

async fn blocking<T: serde::Serialize + Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Response {
    match tokio::task::spawn_blocking(work).await {
        Ok(answer) => Json(answer).into_response(),
        Err(_) => stopped().into_response(),
    }
}

/// `None` when the server stopped before it was done.
async fn blocking_result(
    work: impl FnOnce() -> Result<(), StorageError> + Send + 'static,
) -> Option<Result<(), StorageError>> {
    tokio::task::spawn_blocking(work).await.ok()
}

fn stopped() -> (StatusCode, &'static str) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "The server stopped before it was done. Try it again.",
    )
}
