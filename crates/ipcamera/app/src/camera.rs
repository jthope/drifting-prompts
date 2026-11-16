//! The cameras: their streams, PTZ, status and settings, each found by its id.
//!
//! This is the work behind `bridge` on Windows. `bridge` is the only way TSX reaches it, and
//! `video` reads frames from it, so a session belongs to its [`Camera`] rather than to whatever is
//! showing it. A hot reload or a change of layout can therefore never drop the stream.
//!
//! The functions here are what `bridge` calls, each a few lines over the parts below:
//!
//! - `hub`: [`Hub`](hub::Hub), every camera the app holds, which of them stream, and the event log
//!   their watches fill.
//! - `device`: [`Camera`], one camera's settings, feed, PTZ, sound and recording.
//! - `feed`: where a connected camera's frames come from, a media session or the test picture.
//! - `status`, `stats`, `pattern`, `storage`, `explorer`: a stream's status and stats as text, the
//!   test picture, the folders files are saved in, and showing a file in Explorer.
//!
//! The cameras come from `data\settings\cameras.json`, through `ipcamera_core`'s [`Library`], which
//! also keeps the groups and the views (`crate::library` hands those to TSX, through
//! [`with_library`]). The hub holds one [`Camera`] per saved camera, in the same order, and adds
//! and removes them as the user does.
//!
//! Which cameras stream is decided here, from what is on screen. TSX says which streams its tiles
//! show, which it keeps, and which one is selected ([`set_on_screen`]): a camera's own stream, or
//! another a tile plays (`ipcamera_core::TileStream`), which is a session of its own. A tile on Auto
//! is never sent as such: TSX has picked its stream by its size first. A stream shown
//! or kept connects by itself when its camera has a password, unless the user disconnected the
//! camera. One that is neither disconnects, so a dozen saved cameras don't all stream at once. A
//! kept stream is one the window comes back to: the last view's, and the cameras opened on their
//! own since. One no tile shows stays connected and its status current, but nothing is decoded or
//! drawn until a tile shows it again (`MediaSession::set_shown`). Only the selected tile's stream
//! is heard.
//!
//! A camera that records stays connected when it leaves the screen, so looking at another view or
//! at the videos doesn't end a recording. Only stopping the recording, or Disconnect, does. A
//! recording is of the camera's own stream, which connects for it when the tiles show only another.
//!
//! The rule for the locks: nothing slow runs while `inner`, `frames` or the hub's lists are held,
//! because the window reads them while it draws. Starting and stopping a pipeline wait on another
//! thread, so those take `transition` instead, which only other transitions wait for.

mod device;
mod explorer;
mod feed;
mod hub;
mod pattern;
mod stats;
mod status;
mod storage;
#[cfg(test)]
mod tests;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use dp_ui::WindowPlace;
use ipcamera_core::{
    Applied, CameraEvent, Change, Library,
    config::{Automation, EventCapture},
};
use ipcamera_storage::{
    recording,
    saved_file::{self, SNAPSHOT, VIDEO},
    snapshot,
    space::space_under,
    thumbnail,
};
use nsh_core::prelude::blocking;

pub use self::device::{Camera, Output, StreamKey};
pub use self::storage::{
    choose_storage, open_storage, reset_storage, storage_abilities, storage_folder, storage_space,
};
use self::{
    device::file_name,
    hub::{HUB, StreamOf},
};
use crate::{
    bridge::{
        AutomationSettings, CameraChanges, CameraForm, CameraState, CameraSummary, DetectedCamera,
        EventCaptureInfo, EventInfo, FormProblem, FoundCamera, MediaUsage, ShownStream,
        SnapshotInfo, UiPreferences, VideoInfo,
    },
    form, form_check,
};

/// The camera `id`.
pub fn find(id: &str) -> Result<Arc<Camera>, String> {
    HUB.camera(id)
}

/// Every camera, in the order they are shown.
pub fn list() -> Vec<CameraSummary> {
    HUB.all().iter().map(|camera| camera.summary()).collect()
}

/// What every camera is doing right now, in the order they are shown.
pub fn states() -> Vec<CameraState> {
    HUB.all().iter().map(|camera| camera.state()).collect()
}

/// Whether camera `id` may connect by itself.
pub fn ready(id: &str) -> Result<bool, String> {
    Ok(find(id)?.ready())
}

/// Starts camera `id`'s streams on screen because the user asked, or its own when none is;
/// nothing happens to one already running.
pub async fn connect(id: String) -> Result<(), String> {
    off_thread(id, |camera| {
        camera.held.store(false, Ordering::SeqCst);
        camera.connect_to(&HUB.wanted_streams(&camera.id))
    })
    .await
}

/// Stops camera `id`'s stream because the user asked. It stays stopped, on screen or not, until
/// the user connects it again.
pub async fn disconnect(id: String) -> Result<(), String> {
    off_thread(id, |camera| {
        camera.held.store(true, Ordering::SeqCst);
        camera.disconnect();
        Ok(())
    })
    .await
}

/// Says which streams are on screen, which stay connected although no tile shows them, and which
/// one is selected, then connects and disconnects them to match.
pub async fn set_on_screen(
    shown: Vec<ShownStream>,
    kept: Vec<ShownStream>,
    selected: ShownStream,
) -> Result<(), String> {
    let streams = |list: Vec<ShownStream>| list.into_iter().filter_map(stream_of).collect();
    HUB.set_on_screen(streams(shown), streams(kept), stream_of(selected));
    off_main(|| HUB.reconcile()).await
}

/// The stream TSX names; `None` for none, as the selection is when an empty tile is selected.
fn stream_of(shown: ShownStream) -> Option<StreamOf> {
    (!shown.camera.is_empty()).then(|| StreamOf {
        stream: ipcamera_core::TileStream::played(&shown.stream),
        ..StreamOf::own(&shown.camera)
    })
}

/// Saves the frame on screen of every camera in `ids` that is streaming, returning how many. The
/// others are left out; only when none can does this say why.
pub async fn snapshot(ids: Vec<String>) -> Result<u32, String> {
    off_main(move || HUB.snapshot(&ids)).await?
}

/// Starts recording every camera in `ids` that is streaming, returning how many. The others are
/// left out; only when none can record does this say why.
pub async fn record(ids: Vec<String>) -> Result<u32, String> {
    off_main(move || HUB.record(&ids)).await?
}

/// Stops every recording, returning how many there were. A camera kept connected only to record
/// then disconnects, if it is off screen.
pub async fn stop_recording() -> Result<u32, String> {
    off_main(|| {
        let stopped = HUB
            .all()
            .iter()
            .filter(|camera| camera.stop_recording())
            .count();
        HUB.reconcile();
        stopped as u32
    })
    .await
}

/// Every recording in the folder, newest first, each saying whether it is still being written.
pub async fn videos() -> Result<Vec<VideoInfo>, String> {
    let folder = HUB.config.videos_dir();
    let writing: HashSet<String> = HUB
        .all()
        .iter()
        .filter_map(|camera| camera.recording_file())
        .map(|path| file_name(&path))
        .collect();
    off_main(move || {
        let now_ms = saved_file::now_ms();
        let saved_by = HUB.events.saved_files();
        recording::list(&folder)
            .into_iter()
            .map(|saved| VideoInfo {
                event: saved_by.get(&saved.name).cloned().unwrap_or_default(),
                recording: writing.contains(&saved.name),
                // One still being written runs to now; the file's time can lag behind.
                seconds: if writing.contains(&saved.name) {
                    now_ms.saturating_sub(saved.started_ms)
                } else {
                    saved.length_ms()
                } as f64
                    / 1000.0,
                thumbnail: saved
                    .thumbnail
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                path: saved.path.display().to_string(),
                camera: saved.camera.unwrap_or_default(),
                tags: saved.tags,
                started_at: saved.started_ms as f64,
                bytes: saved.bytes as f64,
                name: saved.name,
            })
            .collect()
    })
    .await
}

/// Changes whenever a recording is added, deleted or written to, by this app or anything else.
/// It lists the folder, which a slow drive or a network share can keep waiting, so off the main
/// thread; `None` when the listing couldn't be read this time.
pub async fn videos_revision() -> Option<u64> {
    let folder = HUB.config.videos_dir();
    off_main(move || recording::revision(&folder)).await.ok()
}

/// Deletes the recording called `name` for good, off the main thread. One still being written
/// stays.
pub async fn delete_video(name: String) -> Result<(), String> {
    let writing = HUB
        .all()
        .iter()
        .filter_map(|camera| camera.recording_file())
        .any(|path| file_name(&path) == name);
    if writing {
        return Err(STILL_RECORDING.to_string());
    }
    let folder = HUB.config.videos_dir();
    off_main(move || recording::delete(&folder, &name))
        .await?
        .map_err(|error| error.to_string())
}

/// Why a recording can't be deleted yet, as the server says it too.
const STILL_RECORDING: &str = "That video is still recording. Stop recording first.";

/// Shows the recording called `name` in Explorer.
pub fn reveal_video(name: &str) -> Result<(), String> {
    explorer::reveal(&VIDEO, &HUB.config.videos_dir(), name)
}

/// Opens the videos folder in Explorer.
pub fn open_videos_folder() -> Result<(), String> {
    explorer::open_folder(&HUB.config.videos_dir())
}

/// Every snapshot in the folder, newest first. Each file's header is read for its size, so this
/// runs off the main thread. Thumbnails a snapshot lacks are made on a thread of their own, and
/// change the folder's revision as they land, so the gallery reads it again.
pub async fn snapshots() -> Result<Vec<SnapshotInfo>, String> {
    let folder = HUB.config.snapshots_dir();
    off_main(move || {
        let listed = snapshot::list(&folder);
        if listed.iter().any(|saved| saved.thumbnail.is_none()) {
            snapshot::fill_thumbnails_soon(folder);
        }
        let saved_by = HUB.events.saved_files();
        listed
            .into_iter()
            .map(|saved| SnapshotInfo {
                event: saved_by.get(&saved.name).cloned().unwrap_or_default(),
                tags: saved.tags,
                name: saved.name,
                path: saved.path.display().to_string(),
                thumbnail: saved
                    .thumbnail
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
                camera: saved.camera.unwrap_or_default(),
                taken_at: saved.taken_ms as f64,
                bytes: saved.bytes as f64,
                width: saved.size.map_or(0, |(width, _)| width),
                height: saved.size.map_or(0, |(_, height)| height),
            })
            .collect()
    })
    .await
}

/// Changes whenever a snapshot is added, deleted or rewritten, by this app or anything else.
/// It lists the folder, as [`videos_revision`] does, so off the main thread.
pub async fn snapshots_revision() -> Option<u64> {
    let folder = HUB.config.snapshots_dir();
    off_main(move || snapshot::revision(&folder)).await.ok()
}

/// Deletes the snapshot called `name` for good, off the main thread.
pub async fn delete_snapshot(name: String) -> Result<(), String> {
    let folder = HUB.config.snapshots_dir();
    off_main(move || snapshot::delete(&folder, &name))
        .await?
        .map_err(|error| error.to_string())
}

/// Shows the snapshot called `name` in Explorer.
pub fn reveal_snapshot(name: &str) -> Result<(), String> {
    explorer::reveal(&SNAPSHOT, &HUB.config.snapshots_dir(), name)
}

/// Opens the snapshots folder in Explorer.
pub fn open_snapshots_folder() -> Result<(), String> {
    explorer::open_folder(&HUB.config.snapshots_dir())
}

/// Whether the app keeps an event log: on the desktop it does, watching every camera it can.
pub fn events_kept() -> bool {
    true
}

/// Every event the cameras saw, newest first, off the main thread: each one's files are looked
/// for on disk, since they can be deleted from their own pages, or in Explorer.
pub async fn events() -> Result<Vec<EventInfo>, String> {
    off_main(|| {
        let snapshots = HUB.config.snapshots_dir();
        let videos = HUB.config.videos_dir();
        HUB.events
            .events()
            .into_iter()
            .map(|event| event_info(&snapshots, &videos, event))
            .collect()
    })
    .await
}

/// `event` as TSX shows it, with those of its files still in `snapshots` and `videos`.
fn event_info(snapshots: &Path, videos: &Path, event: CameraEvent) -> EventInfo {
    let snapshot = event
        .snapshot
        .as_ref()
        .map(|name| (name, snapshots.join(name)))
        .filter(|(_, path)| path.is_file());
    let shown = |path: Option<PathBuf>| {
        path.map(|path: PathBuf| saved_file::absolute(&path).display().to_string())
            .unwrap_or_default()
    };
    EventInfo {
        snapshot: snapshot
            .as_ref()
            .map(|(name, _)| name.to_string())
            .unwrap_or_default(),
        picture: shown(snapshot.as_ref().map(|(_, path)| path.clone())),
        thumbnail: shown(
            snapshot
                .as_ref()
                .and_then(|(_, path)| thumbnail::existing(path)),
        ),
        videos: event
            .videos
            .iter()
            .filter(|name| videos.join(name).is_file())
            .cloned()
            .collect(),
        sightings: event.sightings,
        ongoing: event.ongoing(),
        id: event.id,
        camera: event.camera.to_string(),
        started_at: event.started as f64,
        ended_at: event.ended.unwrap_or(0) as f64,
        motion: event.motion,
        detected: event
            .detected
            .iter()
            .map(|detection| detection.key().to_string())
            .collect(),
    }
}

/// Goes up whenever an event starts, changes, ends or is deleted.
pub fn events_revision() -> u64 {
    HUB.events.revision()
}

/// Deletes each of the events `ids` that the camera doesn't see now, and what each saved, off
/// the main thread; resolves with how many. One the camera sees now stays.
pub async fn delete_events(ids: Vec<String>) -> Result<u32, String> {
    off_main(move || {
        let gone = HUB
            .events
            .delete(&ids)
            .map_err(|error| format!("Could not delete the events: {error}"))?;
        HUB.delete_saved(&gone);
        Ok(gone.len() as u32)
    })
    .await?
}

/// Deletes every event the cameras don't see now, and what they saved, off the main thread;
/// resolves with how many.
pub async fn clear_events() -> Result<u32, String> {
    off_main(|| {
        let gone = HUB
            .events
            .clear()
            .map_err(|error| format!("Could not clear the events: {error}"))?;
        HUB.delete_saved(&gone);
        Ok(gone.len() as u32)
    })
    .await?
}

/// Whether the app does anything by itself: on the desktop it does.
pub fn automation_available() -> bool {
    true
}

/// The saved automation settings.
pub fn automation() -> AutomationSettings {
    let automation = HUB.config.current().automation;
    AutomationSettings {
        log_events: automation.log_events,
        event_capture: match automation.event_capture {
            EventCapture::Nothing => EventCaptureInfo::Nothing,
            EventCapture::Picture => EventCaptureInfo::Picture,
            EventCapture::Video => EventCaptureInfo::Video,
            EventCapture::PictureAndVideo => EventCaptureInfo::PictureAndVideo,
        },
        event_window_minutes: automation.event_window_minutes,
        media_limit_mb: automation.media_limit_mb as f64,
        video_limit_mb: automation.video_limit_mb as f64,
        overwrite_oldest: automation.overwrite_oldest,
        event_video: form::event_video_settings(&automation.event_video.within_limits()),
    }
}

/// `form` as the desktop shows it: a camera that follows Settings → Automation's event video
/// settings shows those, to start its own from.
fn with_app_event_video(mut form: CameraForm) -> CameraForm {
    if !form.own_event_video {
        form.event_video = form::event_video_settings(&HUB.config.current().automation.event_video);
    }
    form
}

/// Saves the automation settings, off the main thread. The hub reads them as it needs them, so
/// they apply at once.
pub async fn save_automation(settings: AutomationSettings) -> Result<(), String> {
    // A size that isn't a whole, positive number of megabytes is no limit.
    let megabytes = |size: f64| {
        if size.is_finite() && size >= 1.0 {
            size as u64
        } else {
            0
        }
    };
    let automation = Automation {
        log_events: settings.log_events,
        event_capture: match settings.event_capture {
            EventCaptureInfo::Nothing => EventCapture::Nothing,
            EventCaptureInfo::Picture => EventCapture::Picture,
            EventCaptureInfo::Video => EventCapture::Video,
            EventCaptureInfo::PictureAndVideo => EventCapture::PictureAndVideo,
        },
        event_window_minutes: settings.event_window_minutes,
        media_limit_mb: megabytes(settings.media_limit_mb),
        video_limit_mb: megabytes(settings.video_limit_mb),
        overwrite_oldest: settings.overwrite_oldest,
        event_video: form::event_video_of(&settings.event_video),
    };
    off_main(move || {
        HUB.config
            .update(|config| config.automation = automation)
            .map_err(|error| format!("Could not save the automation settings: {error}"))
    })
    .await?
}

/// What the snapshots and videos take, the room left on the videos' drive and the media limit,
/// off the main thread. It reads the folder listings, and the files cameras are writing.
pub async fn media_usage() -> Result<MediaUsage, String> {
    off_main(|| {
        let (videos, snapshots) = HUB.media_sizes();
        let space = space_under(&HUB.config.videos_dir()).ok();
        let automation = HUB.config.current().automation;
        MediaUsage {
            videos: videos as f64,
            snapshots: snapshots as f64,
            free: space.as_ref().map_or(0.0, |space| space.free as f64),
            drive: space.map(|space| space.drive).unwrap_or_default(),
            limit: automation.media_limit().unwrap_or(0) as f64,
            make_room: automation.overwrite_oldest,
        }
    })
    .await
}

/// What the snapshots and videos hold together now, in bytes, off the main thread.
pub async fn media_size() -> Result<f64, String> {
    off_main(|| HUB.media_size() as f64).await
}

/// Sets camera `id`'s audio. `volume` is 0..100.
pub fn set_audio(id: &str, muted: bool, volume: f64) -> Result<(), String> {
    find(id)?.set_audio(muted, volume as f32);
    Ok(())
}

/// Starts or changes a PTZ move, each axis -1..1 and `speed` 0..1. It only queues the move for
/// PTZ's own thread, so it never waits on the camera.
pub async fn ptz_move(
    id: String,
    pan: f64,
    tilt: f64,
    zoom: f64,
    speed: f64,
) -> Result<(), String> {
    find(&id)?.ptz_move(pan, tilt, zoom, speed);
    Ok(())
}

/// Stops a PTZ move, queued as the move is.
pub async fn ptz_stop(id: String) -> Result<(), String> {
    find(&id)?.ptz_stop();
    Ok(())
}

/// How long a spotlight switch may take before the user hears that the camera didn't answer: a
/// login and a request, each of which can take ten seconds.
const SPOTLIGHT_ANSWER: Duration = Duration::from_secs(25);

/// Switches camera `id`'s spotlight, through its watch's thread, waiting for the answer off the
/// main thread.
pub async fn set_spotlight(id: String, on: bool) -> Result<(), String> {
    let answer = find(&id)?.set_spotlight(on)?;
    off_main(move || {
        answer
            .recv_timeout(SPOTLIGHT_ANSWER)
            .map_err(|_| "The camera didn't answer. Try again.".to_string())
            .and_then(|done| done)
    })
    .await?
}

/// Searches this machine's networks for cameras, off the main thread.
pub async fn discover() -> Result<Vec<FoundCamera>, String> {
    // On the test picture, or in the test runner, nothing may reach a real camera. The test
    // picture lists `IPCAMERA_FOUND`'s cameras instead, where it names a file.
    if HUB.source != hub::Source::Stream {
        let stand_ins = (HUB.source == hub::Source::Pattern)
            .then(pattern::found_cameras)
            .flatten();
        return match stand_ins {
            Some(found) => found.map(form::found_cameras),
            None => Err(
                "This run of the app reaches no camera, so there is nothing to find.".to_string(),
            ),
        };
    }
    off_main(|| form::found_cameras(ipcamera_core::find_cameras())).await
}

/// Asks the camera at the form's address which brand it is, and a Reolink what it has, off the
/// main thread. The desktop's form holds the password itself, so `id` isn't needed here.
pub async fn detect(_id: String, form: CameraForm) -> Result<DetectedCamera, String> {
    // On the test picture, or in the test runner, nothing may reach a real camera.
    if HUB.source != hub::Source::Stream {
        return Ok(form::nothing_to_ask(
            &form,
            "This run of the app reaches no camera, so there is nothing to connect to.",
        ));
    }
    let camera = form::to_config(&form);
    off_main(move || {
        let mut found = ipcamera_core::detect(&camera)?;
        // Then the size of each RTSP stream nothing above sized, which Auto picks by.
        let asked = ipcamera_core::CameraConfig {
            brand: found.brand,
            ..camera.clone()
        };
        let sized = ipcamera_media::sized_streams(&asked, &found.streams);
        ipcamera_core::CameraStream::merge(&mut found.streams, sized);
        Ok(form::detected(
            &form,
            form::Found {
                brand: found.brand,
                api: found.api,
                device: found.device.as_ref(),
                features: found.features.as_ref(),
                asked_with_password: !camera.password.is_empty(),
                http_api: found.http_api,
                onvif_events: found.onvif_events,
                onvif_port: found.onvif_port,
                onvif_problem: found.onvif_problem.as_deref(),
                streams: &found.streams,
                onvif_refused: found.onvif_refused,
            },
        ))
    })
    .await?
}

/// Camera `id`'s settings, as the form shows them.
pub fn form(id: &str) -> Result<CameraForm, String> {
    Ok(with_app_event_video(find(id)?.form()))
}

/// The form for a camera that is not saved yet.
pub fn blank_form() -> CameraForm {
    with_app_event_video(form::blank())
}

/// What stops the form being saved.
pub fn form_problems(form: &CameraForm) -> Vec<FormProblem> {
    form::problems(form)
}

/// Saves the form. A running stream reconnects with it.
pub async fn save_form(id: String, form: CameraForm) -> Result<(), String> {
    off_main(move || HUB.save(&id, &form)).await?
}

/// What stops `changes` being given to several cameras.
pub fn change_problems(changes: &CameraChanges) -> Vec<FormProblem> {
    form_check::change_problems(changes)
}

/// Gives every camera in `ids` the changes in one save, returning how many changed. Their running
/// streams reconnect with them.
pub async fn save_changes(ids: Vec<String>, changes: CameraChanges) -> Result<u32, String> {
    off_main(move || HUB.save_changes(&ids, &changes)).await?
}

/// Saves a new camera from the form, resolving with its id.
pub async fn add_camera(form: CameraForm) -> Result<String, String> {
    if let Some(problem) = form::problems(&form).first() {
        return Err(problem.message.clone());
    }
    off_main(move || HUB.add(form::to_config(&form))).await?
}

/// Makes a change to the groups or the views in `data\settings`, off the main thread, since it
/// writes a file.
pub async fn apply(change: Change) -> Result<Applied, String> {
    off_main(move || {
        HUB.library
            .apply(&change)
            .map_err(|error| error.to_string())
    })
    .await?
}

/// Bumped by every saved change to the cameras, groups or views.
pub fn revision() -> u64 {
    HUB.library.revision()
}

/// The desktop keeps its cameras itself: there is no server to sign in to.
pub fn sign_in_needed() -> bool {
    false
}

/// Never, on the desktop: see [`sign_in_needed`].
pub fn signed_in() -> bool {
    false
}

/// Refused on the desktop, which has no server.
pub async fn sign_in(_password: String) -> Result<(), String> {
    Err("The desktop app has no server to sign in to.".to_string())
}

/// Nothing to end on the desktop.
pub async fn sign_out() -> Result<(), String> {
    Ok(())
}

/// The desktop draws every camera's picture itself, so no stream has a page to show.
pub fn live_url(_id: &str, _stream: &str) -> String {
    String::new()
}

/// Deletes camera `id` from the list, every group and every view, stopping its stream.
pub async fn delete_camera(id: String) -> Result<(), String> {
    off_main(move || HUB.delete(&id)).await?
}

/// Moves camera `id` to `index` among the cameras, off the main thread, since it writes a file.
pub async fn move_camera(id: String, index: u32) -> Result<(), String> {
    off_main(move || HUB.move_camera(&id, index)).await?
}

/// Calls `use_it` with the cameras, groups and views saved in `data\settings`, for `library`'s
/// functions.
pub fn with_library<R>(use_it: impl FnOnce(&Library) -> R) -> R {
    use_it(&HUB.library)
}

/// What went wrong opening the files, for the app to tell the user once: `settings.json` first,
/// then the cameras, groups and views.
pub fn library_problems() -> Vec<String> {
    HUB.config
        .problem()
        .map(str::to_string)
        .into_iter()
        .chain(HUB.library.problems())
        .collect()
}

/// The saved window preferences.
pub fn preferences() -> UiPreferences {
    let ui = HUB.config.current().ui;
    UiPreferences {
        rail_collapsed: ui.rail_collapsed,
        rail_width: f64::from(ui.rail_width),
        sidebar_width: f64::from(ui.sidebar_width),
        appearance: ui.appearance.into(),
    }
}

/// Saves the window preferences, off the main thread. A preference that cannot be written must
/// never interrupt what the user is doing, so a failure is silent. They must not restart the stream
/// either.
pub async fn save_preferences(preferences: UiPreferences) {
    let _ = off_main(move || {
        HUB.config.update(|config| {
            config.ui.rail_collapsed = preferences.rail_collapsed;
            config.ui.rail_width = preferences.rail_width as f32;
            config.ui.sidebar_width = preferences.sidebar_width as f32;
            config.ui.appearance = preferences.appearance.into();
        })
    })
    .await;
}

/// The page the window showed last, as `settings.json`'s `page` keeps it.
pub fn last_page() -> String {
    HUB.config.current().ui.page
}

/// The newest page save handed to [`save_page`], so an older one that writes after it leaves it be.
static NEWEST_PAGE: AtomicU64 = AtomicU64::new(0);

/// Saves the page the window shows, on a thread of its own. Two saves can finish in either order,
/// so each takes a ticket, and one that is no longer the newest writes the file without its page:
/// the newer save's, already in the settings or about to be. The check runs inside `update`,
/// which writes one change at a time. A failure is silent, as every preference's is.
pub async fn save_page(page: String) {
    let ticket = NEWEST_PAGE.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = off_main(move || {
        HUB.config.update(|config| {
            if NEWEST_PAGE.load(Ordering::SeqCst) == ticket {
                config.ui.page = page;
            }
        })
    })
    .await;
}

/// Whether the app shows a message when a camera starts seeing something.
pub fn alerts() -> bool {
    HUB.config.current().ui.alerts
}

/// Saves whether the app shows those messages, off the main thread. A failure is silent, as
/// every preference's is.
pub async fn save_alerts(on: bool) {
    let _ = off_main(move || HUB.config.update(|config| config.ui.alerts = on)).await;
}

/// How wide the Events page's table was last dragged, in points; none until it is.
pub fn events_table_width() -> Option<f64> {
    HUB.config.current().ui.events_table_width.map(f64::from)
}

/// Saves how wide the Events page's table was dragged, off the main thread. A failure is silent,
/// as every preference's is.
pub async fn save_events_table_width(width: f64) {
    let _ = off_main(move || {
        HUB.config
            .update(|config| config.ui.events_table_width = Some(width as f32))
    })
    .await;
}

/// Where the window was last left.
pub fn saved_window() -> WindowPlace {
    HUB.config.current().window.into()
}

/// Saves where the window settled, off the main thread. Like every preference, a failure is silent
/// and it must not restart the stream, or moving the window would reconnect the camera.
pub async fn save_window(place: WindowPlace) {
    let _ = off_main(move || HUB.config.update(|config| config.window = place.into())).await;
}

/// Runs `work` on a thread of its own, because a TSX call must not wait on a camera, a disk or a
/// dialog. It fails only when `work` panicked.
async fn off_main<R: Send + 'static>(
    work: impl FnOnce() -> R + Send + 'static,
) -> Result<R, String> {
    blocking(work).await.map_err(|error| error.to_string())
}

/// Runs `work` on camera `id`, on a thread of its own, because a TSX call must not wait for a
/// pipeline.
async fn off_thread<R: Send + 'static>(
    id: String,
    work: impl FnOnce(&Camera) -> Result<R, String> + Send + 'static,
) -> Result<R, String> {
    let camera = find(&id)?;
    off_main(move || work(&camera)).await?
}
