//! The web build's backend: a copy of `ipcamera-server`'s library for the bridge's synchronous
//! reads, changes sent to the server as `Change`s, and its player for live video. Until a server
//! answers, it is a preview over a pretend camera.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    time::Duration,
};

use dp_ui::WindowPlace;
use ipcamera_core::{
    Applied, CameraConfig, CameraId, CameraUpdate, Change, ClientCopy, Library,
    api::{
        self, CameraStatus, DetectReply, DetectRequest, PtzMove, RecordRequest, Recorded, SignIn,
        SignedIn, SnapshotEntry, SnapshotRequest, SnapshotsTaken, SpotlightRequest, Status,
        VideoEntry, path_segment,
    },
};
use nsh_core::prelude::{HttpRequest, fetch, open_url, spawn, timer};
use serde::de::DeserializeOwned;

use crate::{
    bridge::{
        AutomationSettings, CameraChanges, CameraForm, CameraState, CameraSummary, DetectedCamera,
        DriveSpace, EventCaptureInfo, EventInfo, FormProblem, FoundCamera, MediaUsage, ShownStream,
        SnapshotInfo, StatusTone, StorageAbilities, StorageFolder, UiPreferences, VideoInfo,
    },
    form, form_check, library,
};

/// The server when the page is served from elsewhere, set at build time.
const SERVER: Option<&str> = option_env!("IPCAMERA_SERVER");

const CHECK_EVERY: Duration = Duration::from_secs(1);

const OUT_OF_REACH_AFTER: u32 = 3;

const OUT_OF_REACH: &str = "The server isn't answering. Check that ipcamera-server is running.";

/// Under the second after which the server stops a move that isn't sent again.
const PTZ_RENEW_EVERY: Duration = Duration::from_millis(400);

const NO_PASSWORD: &str =
    "Add the camera's password in Settings. If it has none, press Connect to try it without one.";

const SIGN_IN_FIRST: u16 = 401;

const NAME: &str = "Preview camera";
const NO_ADDRESS: &str = "No address yet";
const NOT_HERE: &str = "That needs ipcamera-server or the desktop app.";
const NO_ANSWER: &str =
    "The server didn't answer. Check that ipcamera-server is running, then try again.";
const UNREADABLE: &str = "The server's answer couldn't be read. Reload the page.";
const PASSWORD_KEPT: &str = "Leave it empty to keep the saved password.";
const FOLDERS_ON_THE_SERVER: &str =
    "The server's folders are set in its settings.json, or from the desktop app.";

struct Web {
    library: Library,
    server: Option<String>,
    server_revision: Option<u64>,
    with_password: HashSet<CameraId>,
    player: String,
    /// Added to the copy's revision, which restarts at 0 with each copy, so no change goes unheard.
    base: u64,
    checking: bool,
    status: Status,
    storage: Option<api::Storage>,
    ptz_refused: HashMap<CameraId, String>,
    held: Option<Hold>,
    presses: u64,
    unanswered: u32,
    key: Option<String>,
    sign_in_needed: bool,
    usage: Option<((u64, u64), MediaUsage)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hold {
    press: u64,
    request: PtzMove,
}

impl Web {
    fn preview() -> Self {
        Self {
            library: Library::in_memory(vec![form::to_config(&CameraForm {
                name: NAME.to_string(),
                ..form::blank()
            })]),
            server: None,
            server_revision: None,
            with_password: HashSet::new(),
            player: String::new(),
            base: 0,
            checking: false,
            status: Status::default(),
            storage: None,
            ptz_refused: HashMap::new(),
            held: None,
            presses: 0,
            unanswered: 0,
            key: None,
            sign_in_needed: false,
            usage: None,
        }
    }

    /// The server turned away a request sent with `key`; a key replaced since is left alone.
    fn turned_away(&mut self, key: Option<&str>) {
        if self.key.as_deref() == key {
            self.key = None;
            self.sign_in_needed = true;
        }
    }

    fn checked(&mut self, answered: bool) {
        self.unanswered = if answered {
            0
        } else {
            self.unanswered.saturating_add(1)
        };
    }

    fn out_of_reach(&self) -> bool {
        self.server.is_some() && self.unanswered >= OUT_OF_REACH_AFTER
    }

    fn revision(&self) -> u64 {
        self.base + self.library.revision()
    }

    fn take(&mut self, server: String, copy: ClientCopy, player: String) {
        let revision = self.revision();
        self.player = player;
        self.server_revision = Some(copy.snapshot.revision);
        self.with_password = copy.with_password.into_iter().collect();
        self.library = Library::from_snapshot(copy.snapshot);
        self.base = revision + 1;
        self.server = Some(server);
    }
}

thread_local! {
    /// Thread-local, so tests running side by side each start from the preview.
    static WEB: RefCell<Web> = RefCell::new(Web::preview());
}

/// Calls `use_it` with the cameras, groups and views.
pub fn with_library<R>(use_it: impl FnOnce(&Library) -> R) -> R {
    WEB.with(|web| use_it(&web.borrow().library))
}

fn server() -> Option<String> {
    WEB.with(|web| web.borrow().server.clone())
}

/// Bumped by every change to the cameras, groups or views; reading it keeps the copy in step.
pub fn revision() -> u64 {
    keep_in_step();
    WEB.with(|web| web.borrow().revision())
}

/// Checks the server from a task that ends with the listener that read the revision.
fn keep_in_step() {
    let start = WEB.with(|web| !std::mem::replace(&mut web.borrow_mut().checking, true));
    if !start {
        return;
    }
    /// Lets the next listener start checking again once this task ends.
    struct Checking;
    impl Drop for Checking {
        fn drop(&mut self) {
            WEB.with(|web| web.borrow_mut().checking = false);
        }
    }
    // Dropping the handle doesn't cancel the task.
    drop(spawn(async {
        let _checking = Checking;
        loop {
            check().await;
            timer(CHECK_EVERY).await;
        }
    }));
}

fn server_address() -> String {
    SERVER.unwrap_or("").trim_end_matches('/').to_string()
}

async fn check() {
    let address = server_address();
    let Ok(status) = ask::<Status>(HttpRequest::get(format!("{address}/api/status"))).await else {
        // A server that asks to sign in has answered: it isn't out of reach.
        WEB.with(|web| {
            let mut web = web.borrow_mut();
            let answered = web.sign_in_needed;
            web.checked(answered);
        });
        return;
    };
    let stale = WEB.with(|web| {
        let mut web = web.borrow_mut();
        web.checked(true);
        let stale = web.server_revision != Some(status.revision);
        web.status = status;
        stale
    });
    if stale {
        let _ = refresh(&address).await;
    }
}

async fn refresh(address: &str) -> Result<(), String> {
    let body = call(HttpRequest::get(format!("{address}/api/library"))).await?;
    let unreadable = |_| "The server's library couldn't be read. Reload the page.".to_string();
    let answer: serde_json::Value = serde_json::from_slice(&body).map_err(unreadable)?;
    let player = answer["player"].as_str().unwrap_or(address).to_string();
    let copy: ClientCopy = serde_json::from_value(answer).map_err(unreadable)?;
    WEB.with(|web| web.borrow_mut().take(address.to_string(), copy, player));
    let _ = storage(address).await;
    Ok(())
}

async fn storage(address: &str) -> Result<api::Storage, String> {
    let storage: api::Storage = ask(HttpRequest::get(format!("{address}/api/storage"))).await?;
    WEB.with(|web| web.borrow_mut().storage = Some(storage.clone()));
    Ok(storage)
}

async fn ask<T: DeserializeOwned>(request: HttpRequest) -> Result<T, String> {
    let body = call(request).await?;
    serde_json::from_slice(&body).map_err(|_| UNREADABLE.to_string())
}

fn on_server(missing: &str) -> Result<String, String> {
    server().ok_or_else(|| missing.to_string())
}

async fn call(request: HttpRequest) -> Result<Vec<u8>, String> {
    let key = WEB.with(|web| web.borrow().key.clone());
    let request = match &key {
        Some(key) => request.header("Authorization", format!("Bearer {key}")),
        None => request,
    };
    let response = fetch(request).await.map_err(|_| NO_ANSWER.to_string())?;
    if response.ok() {
        return Ok(response.body);
    }
    if response.status() == SIGN_IN_FIRST {
        WEB.with(|web| web.borrow_mut().turned_away(key.as_deref()));
    }
    let reason = String::from_utf8_lossy(&response.body).trim().to_string();
    Err(if reason.is_empty() {
        format!("The server refused it ({}).", response.status())
    } else {
        reason
    })
}

/// `url` with the session's key in it, for what the browser fetches itself, where no header goes.
fn keyed(url: String) -> String {
    match WEB.with(|web| web.borrow().key.clone()) {
        Some(key) => {
            let joint = if url.contains('?') { '&' } else { '?' };
            format!("{url}{joint}key={key}")
        }
        None => url,
    }
}

/// Whether the server wants its password first; reading it keeps the status in step.
pub fn sign_in_needed() -> bool {
    keep_in_step();
    WEB.with(|web| web.borrow().sign_in_needed)
}

/// Whether someone has signed in to the server from this page.
pub fn signed_in() -> bool {
    WEB.with(|web| web.borrow().key.is_some())
}

/// Signs in to the server with its `password` and takes its library; the error is the server's.
pub async fn sign_in(password: String) -> Result<(), String> {
    let address = server_address();
    let signed_in: SignedIn = ask(json_post(
        format!("{address}/api/session"),
        &SignIn { password },
    )?)
    .await?;
    WEB.with(|web| {
        let mut web = web.borrow_mut();
        web.key = Some(signed_in.key);
        web.sign_in_needed = false;
        // The tiles' players take the new key with their next address.
        web.base += 1;
    });
    refresh(&address).await
}

/// Ends this page's session on the server, which then asks for its password again.
pub async fn sign_out() -> Result<(), String> {
    if !signed_in() {
        return Ok(());
    }
    let address = server_address();
    // The key is dropped here whether or not the server hears of it.
    let _ = call(HttpRequest::new("DELETE", format!("{address}/api/session"))).await;
    WEB.with(|web| {
        let mut web = web.borrow_mut();
        web.key = None;
        web.sign_in_needed = true;
        web.base += 1;
    });
    Ok(())
}

/// Makes a change: on the server, which then sends a fresh copy, or in the preview's memory.
pub async fn apply(change: Change) -> Result<Applied, String> {
    let Some(address) = server() else {
        return make_here(&change);
    };
    let applied: Applied = ask(json_post(format!("{address}/api/changes"), &change)?).await?;
    // The change is made either way; a failed copy comes with the next check.
    let _ = refresh(&address).await;
    Ok(applied)
}

fn json_post(url: String, body: &impl serde::Serialize) -> Result<HttpRequest, String> {
    let body = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    Ok(HttpRequest::post(url, body).header("Content-Type", "application/json"))
}

fn make_here(change: &Change) -> Result<Applied, String> {
    with_library(|saved| saved.apply(change)).map_err(|error| error.to_string())
}

fn camera(id: &str) -> Result<CameraConfig, String> {
    with_library(|saved| saved.camera(&CameraId::from(id)))
        .ok_or_else(|| format!("no camera `{id}`"))
}

fn known(id: &str) -> Result<(), String> {
    camera(id).map(drop)
}

fn has_password(id: &str) -> bool {
    WEB.with(|web| web.borrow().with_password.contains(&CameraId::from(id)))
}

fn stream_label(camera: &CameraConfig) -> String {
    if camera.host.is_empty() {
        NO_ADDRESS.to_string()
    } else {
        camera.stream_label()
    }
}

fn state_of(camera: &CameraConfig) -> CameraState {
    let (on_server, out_of_reach, said, refused) = WEB.with(|web| {
        let web = web.borrow();
        let said: Option<CameraStatus> = web
            .status
            .cameras
            .iter()
            .find(|each| each.id == camera.id)
            .cloned();
        (
            web.server.is_some(),
            web.out_of_reach(),
            said,
            web.ptz_refused.get(&camera.id).cloned(),
        )
    });
    if out_of_reach {
        return CameraState {
            id: camera.id.to_string(),
            name: camera.name.clone(),
            stream_label: stream_label(camera),
            connected: false,
            ready: has_password(camera.id.as_str()),
            status: OUT_OF_REACH.to_string(),
            tone: StatusTone::Warn,
            stats: String::new(),
            address: camera.host.clone(),
            streams: Vec::new(),
            ptz_status: OUT_OF_REACH.to_string(),
            muted: true,
            volume: 80.0,
            recording: false,
            record_problem: String::new(),
            motion: false,
            detected: Vec::new(),
            spotlight: false,
            spotlight_on: false,
            watch_status: String::new(),
        };
    }
    let said = said.unwrap_or_default();
    let watch = said.watch.clone().unwrap_or_default();
    let with_password = has_password(camera.id.as_str());
    let problem = said.stream_problem.clone().or_else(|| {
        (on_server && !with_password && !said.playing).then(|| NO_PASSWORD.to_string())
    });
    let ptz_status = refused.or(said.ptz_status).unwrap_or_else(|| {
        if on_server {
            "PTZ: not checked yet".to_string()
        } else {
            "PTZ needs ipcamera-server or the desktop app.".to_string()
        }
    });
    CameraState {
        id: camera.id.to_string(),
        name: camera.name.clone(),
        stream_label: stream_label(camera),
        connected: on_server && problem.is_none(),
        ready: on_server && with_password,
        tone: if problem.is_some() {
            StatusTone::Warn
        } else {
            StatusTone::Normal
        },
        status: match problem {
            Some(problem) => problem,
            None if on_server => "Plays through the server.".to_string(),
            None => "Preview only: start ipcamera-server for live video.".to_string(),
        },
        stats: String::new(),
        address: camera.host.clone(),
        streams: Vec::new(),
        ptz_status,
        muted: true,
        volume: 80.0,
        recording: on_server && said.recording,
        record_problem: said.record_problem.unwrap_or_default(),
        motion: watch.motion,
        detected: watch
            .detected
            .iter()
            .map(|detection| detection.key().to_string())
            .collect(),
        spotlight: watch.spotlight,
        spotlight_on: watch.spotlight_on,
        watch_status: watch.summary,
    }
}

pub fn list() -> Vec<CameraSummary> {
    with_library(Library::cameras)
        .iter()
        .map(|camera| CameraSummary {
            id: camera.id.to_string(),
            name: camera.name.clone(),
            stream_label: stream_label(camera),
        })
        .collect()
}

pub fn states() -> Vec<CameraState> {
    with_library(Library::cameras)
        .iter()
        .map(state_of)
        .collect()
}

/// The server's player for camera `id` on `stream`; `""` in the preview. Never this page's own
/// origin: nsh strips a same-origin frame's origin, and the browser then refuses its connections.
pub fn live_url(id: &str, stream: &str) -> String {
    if server().is_none() || camera(id).is_err() {
        return String::new();
    }
    let player = WEB.with(|web| web.borrow().player.clone());
    // A path carries its own `?` and `&`, so it goes escaped.
    let stream = match stream.trim() {
        "" => String::new(),
        stream => format!("&stream={}", path_segment(stream)),
    };
    keyed(format!("{player}/watch?camera={id}{stream}"))
}

pub fn ready(id: &str) -> Result<bool, String> {
    known(id)?;
    Ok(server().is_some() && has_password(id))
}

/// Has the server try camera `id` again: one that gave up, or one with no saved password.
pub async fn connect(id: String) -> Result<(), String> {
    known(&id)?;
    let address = on_server("Live video needs ipcamera-server or the desktop app.")?;
    let url = format!("{address}/api/cameras/{}/connect", path_segment(&id));
    call(HttpRequest::post(url, Vec::new())).await?;
    check().await;
    Ok(())
}

pub async fn disconnect(id: String) -> Result<(), String> {
    known(&id)?;
    if server().is_some() {
        Err("On the web, a camera plays while it is on screen.".to_string())
    } else {
        Ok(())
    }
}

/// Nothing to do: each tile connects to the server's player itself.
pub async fn set_on_screen(
    _shown: Vec<ShownStream>,
    _kept: Vec<ShownStream>,
    _selected: ShownStream,
) -> Result<(), String> {
    Ok(())
}

/// Has the server save the newest picture of each of `ids` that can; resolves with how many.
pub async fn snapshot(ids: Vec<String>) -> Result<u32, String> {
    let address = on_server("Snapshots need ipcamera-server or the desktop app.")?;
    let request = SnapshotRequest {
        cameras: ids.iter().map(|id| CameraId::from(id.as_str())).collect(),
    };
    let taken: SnapshotsTaken =
        ask(json_post(format!("{address}/api/snapshots"), &request)?).await?;
    check().await;
    Ok(taken.count)
}

/// The server's snapshots, newest first, each at its URL; none in the preview.
pub async fn snapshots() -> Result<Vec<SnapshotInfo>, String> {
    let Some(address) = server() else {
        return Ok(Vec::new());
    };
    let listed: Vec<SnapshotEntry> =
        ask(HttpRequest::get(format!("{address}/api/snapshots"))).await?;
    Ok(listed
        .into_iter()
        .map(|entry| snapshot_info(&address, entry))
        .collect())
}

fn served(address: &str, url: Option<&str>) -> String {
    url.map(|url| keyed(format!("{address}{url}")))
        .unwrap_or_default()
}

fn snapshot_info(address: &str, entry: SnapshotEntry) -> SnapshotInfo {
    SnapshotInfo {
        path: keyed(format!("{address}{}", entry.url)),
        thumbnail: served(address, entry.thumbnail.as_deref()),
        camera: entry.camera.map(|id| id.to_string()).unwrap_or_default(),
        tags: entry.tags,
        event: String::new(),
        taken_at: entry.taken_ms as f64,
        bytes: entry.bytes as f64,
        width: entry.size.map_or(0, |size| size.width),
        height: entry.size.map_or(0, |size| size.height),
        name: entry.name,
    }
}

/// Changes whenever the server's snapshots do; reading it keeps the status in step.
pub async fn snapshots_revision() -> Option<u64> {
    keep_in_step();
    Some(WEB.with(|web| web.borrow().status.snapshots))
}

const NO_EVENT_LOG: &str =
    "The server keeps no event log yet. The desktop app logs what the cameras see.";

/// The web build keeps no event log.
pub fn events_kept() -> bool {
    false
}

/// None: the web build has no event log.
pub async fn events() -> Result<Vec<EventInfo>, String> {
    Ok(Vec::new())
}

/// Never changes: the web build has no event log.
pub fn events_revision() -> u64 {
    0
}

/// Refused: the web build has no event log.
pub async fn delete_events(_ids: Vec<String>) -> Result<u32, String> {
    Err(NO_EVENT_LOG.to_string())
}

/// Refused: the web build has no event log.
pub async fn clear_events() -> Result<u32, String> {
    Err(NO_EVENT_LOG.to_string())
}

const NO_AUTOMATION: &str =
    "The server does nothing by itself yet. The desktop app records what the cameras see.";

/// The web build does nothing by itself.
pub fn automation_available() -> bool {
    false
}

/// As the desktop starts: the web build has no automation.
pub fn automation() -> AutomationSettings {
    AutomationSettings {
        log_events: true,
        event_capture: EventCaptureInfo::Picture,
        event_window_minutes: 5,
        media_limit_mb: 0.0,
        video_limit_mb: 0.0,
        overwrite_oldest: false,
        event_video: form::event_video_settings(&ipcamera_core::EventVideo::default()),
    }
}

/// Refused: the web build has no automation.
pub async fn save_automation(_settings: AutomationSettings) -> Result<(), String> {
    Err(NO_AUTOMATION.to_string())
}

/// What the server's snapshots and videos take, and the room left on its videos' drive.
pub async fn media_usage() -> Result<MediaUsage, String> {
    keep_in_step();
    let revisions = WEB.with(|web| {
        let web = web.borrow();
        (web.status.snapshots, web.status.videos)
    });
    let kept = WEB.with(|web| web.borrow().usage.clone());
    if let Some((then, usage)) = kept
        && then == revisions
    {
        return Ok(usage);
    }
    let videos: f64 = videos().await?.iter().map(|video| video.bytes).sum();
    let snapshots: f64 = snapshots()
        .await?
        .iter()
        .map(|snapshot| snapshot.bytes)
        .sum();
    let (free, drive) = match storage_space("videos".to_string()).await {
        Ok(space) => (space.free, space.drive),
        Err(_) => (0.0, String::new()),
    };
    let usage = MediaUsage {
        videos,
        snapshots,
        free,
        drive,
        limit: 0.0,
        make_room: false,
    };
    WEB.with(|web| web.borrow_mut().usage = Some((revisions, usage.clone())));
    Ok(usage)
}

/// Refused: the server's folders aren't added up for the web build.
pub async fn media_size() -> Result<f64, String> {
    Err(NO_AUTOMATION.to_string())
}

/// Deletes the server's snapshot called `name` for good.
pub async fn delete_snapshot(name: String) -> Result<(), String> {
    delete_file("snapshots", &name).await
}

/// Opens the snapshot in a new tab, where the browser can save it.
pub async fn reveal_snapshot(name: &str) -> Result<(), String> {
    open_file("snapshots", name)
}

pub async fn open_snapshots_folder() -> Result<(), String> {
    Err(FOLDERS_ON_THE_SERVER.to_string())
}

/// Has the server record each of `ids` that can; resolves with how many do.
pub async fn record(ids: Vec<String>) -> Result<u32, String> {
    let address = on_server("Recording needs ipcamera-server or the desktop app.")?;
    let request = RecordRequest {
        cameras: ids.iter().map(|id| CameraId::from(id.as_str())).collect(),
    };
    let recorded: Recorded = ask(json_post(format!("{address}/api/recording"), &request)?).await?;
    check().await;
    Ok(recorded.count)
}

/// Stops every recording on the server; resolves with how many there were.
pub async fn stop_recording() -> Result<u32, String> {
    let Some(address) = server() else {
        return Ok(0);
    };
    let stopped: Recorded = ask(HttpRequest::new(
        "DELETE",
        format!("{address}/api/recording"),
    ))
    .await?;
    check().await;
    Ok(stopped.count)
}

/// The server's recordings, newest first, each at its URL; none in the preview.
pub async fn videos() -> Result<Vec<VideoInfo>, String> {
    let Some(address) = server() else {
        return Ok(Vec::new());
    };
    let listed: Vec<VideoEntry> = ask(HttpRequest::get(format!("{address}/api/videos"))).await?;
    Ok(listed
        .into_iter()
        .map(|entry| video_info(&address, entry))
        .collect())
}

fn video_info(address: &str, entry: VideoEntry) -> VideoInfo {
    VideoInfo {
        path: keyed(format!("{address}{}", entry.url)),
        thumbnail: served(address, entry.thumbnail.as_deref()),
        camera: entry.camera.map(|id| id.to_string()).unwrap_or_default(),
        tags: entry.tags,
        event: String::new(),
        started_at: entry.started_ms as f64,
        seconds: entry.length_ms as f64 / 1000.0,
        bytes: entry.bytes as f64,
        recording: entry.recording,
        name: entry.name,
    }
}

/// Changes whenever the server's recordings do, one being written included.
pub async fn videos_revision() -> Option<u64> {
    keep_in_step();
    Some(WEB.with(|web| web.borrow().status.videos))
}

/// Deletes the server's recording called `name` for good; one still recording is refused.
pub async fn delete_video(name: String) -> Result<(), String> {
    delete_file("videos", &name).await
}

/// Opens the recording in a new tab, where the browser can play or save it.
pub async fn reveal_video(name: &str) -> Result<(), String> {
    open_file("videos", name)
}

pub async fn open_videos_folder() -> Result<(), String> {
    Err(FOLDERS_ON_THE_SERVER.to_string())
}

async fn delete_file(kind: &str, name: &str) -> Result<(), String> {
    let address = on_server(NOT_HERE)?;
    let url = format!("{address}/api/{kind}/{}", path_segment(name));
    call(HttpRequest::new("DELETE", url)).await?;
    check().await;
    Ok(())
}

/// Opens the server's file in a new tab from the player's address, which is always a whole URL.
fn open_file(kind: &str, name: &str) -> Result<(), String> {
    on_server(NOT_HERE)?;
    let player = WEB.with(|web| web.borrow().player.clone());
    // Whether the browser opened it doesn't change what the page shows.
    drop(open_url(keyed(format!(
        "{player}/files/{kind}/{}",
        path_segment(name)
    ))));
    Ok(())
}

/// The sound plays only on the desktop.
pub fn set_audio(id: &str, _muted: bool, _volume: f64) -> Result<(), String> {
    known(id)
}

/// Starts camera `id`'s PTZ move on the server and sends it again every `PTZ_RENEW_EVERY` until it
/// is let go: the server stops a move that isn't sent again.
pub async fn ptz_move(
    id: String,
    pan: f64,
    tilt: f64,
    zoom: f64,
    speed: f64,
) -> Result<(), String> {
    known(&id)?;
    let Some(address) = server() else {
        return Ok(());
    };
    let press = hold(PtzMove {
        pan,
        tilt,
        zoom,
        speed,
    });
    let camera = CameraId::from(id.as_str());
    let url = format!("{address}/api/cameras/{}/ptz", path_segment(&id));
    while let Some(request) = held(press) {
        let refused = match json_post(url.clone(), &request) {
            Ok(post) => call(post).await.err(),
            Err(why) => Some(why),
        };
        let stop = refused.is_some();
        WEB.with(|web| {
            let mut web = web.borrow_mut();
            match refused {
                Some(why) => web.ptz_refused.insert(camera.clone(), why),
                None => web.ptz_refused.remove(&camera),
            };
        });
        // A refused move won't go through by sending it again.
        if stop {
            release(press);
            return Ok(());
        }
        timer(PTZ_RENEW_EVERY).await;
    }
    Ok(())
}

/// Lets go of camera `id`'s PTZ move; a lost stop still ends it, as the server lets moves run out.
pub async fn ptz_stop(id: String) -> Result<(), String> {
    known(&id)?;
    let press = WEB.with(|web| web.borrow().held.map(|held| held.press));
    if let Some(press) = press {
        release(press);
    }
    let Some(address) = server() else {
        return Ok(());
    };
    let url = format!("{address}/api/cameras/{}/ptz/stop", path_segment(&id));
    let _ = call(HttpRequest::post(url, Vec::new())).await;
    Ok(())
}

fn hold(request: PtzMove) -> u64 {
    WEB.with(|web| {
        let mut web = web.borrow_mut();
        web.presses += 1;
        let press = web.presses;
        web.held = Some(Hold { press, request });
        press
    })
}

fn held(press: u64) -> Option<PtzMove> {
    WEB.with(|web| {
        web.borrow()
            .held
            .filter(|held| held.press == press)
            .map(|held| held.request)
    })
}

fn release(press: u64) {
    WEB.with(|web| {
        let mut web = web.borrow_mut();
        if web.held.is_some_and(|held| held.press == press) {
            web.held = None;
        }
    });
}

/// Switches camera `id`'s spotlight, through the server's watch of it.
pub async fn set_spotlight(id: String, on: bool) -> Result<(), String> {
    known(&id)?;
    let address = on_server(NOT_HERE)?;
    let url = format!("{address}/api/cameras/{}/spotlight", path_segment(&id));
    call(json_post(url, &SpotlightRequest { on })?)
        .await
        .map(drop)
}

/// Asks the server which cameras are on its network.
pub async fn discover() -> Result<Vec<FoundCamera>, String> {
    let address = on_server(NOT_HERE)?;
    let found: Vec<ipcamera_core::FoundCamera> =
        ask(HttpRequest::get(format!("{address}/api/discover"))).await?;
    Ok(form::found_cameras(found))
}

/// Asks the server which camera is at the form's address; an empty password uses the saved one.
pub async fn detect(id: String, form: CameraForm) -> Result<DetectedCamera, String> {
    let Some(address) = server() else {
        return Ok(form::nothing_to_ask(
            &form,
            "This preview reaches no camera, so there is nothing to connect to.",
        ));
    };
    let request = DetectRequest {
        camera: (!id.is_empty()).then(|| CameraId::from(id.as_str())),
        setting: form::to_config(&form),
    };
    let reply: DetectReply = ask(json_post(format!("{address}/api/detect"), &request)?).await?;
    Ok(form::detected(
        &form,
        form::Found {
            brand: reply.brand,
            api: reply.api_port.zip(reply.api_https),
            http_api: reply.http_api,
            device: reply.device.as_ref(),
            features: reply.features.as_ref(),
            asked_with_password: reply.asked_with_password,
            onvif_events: reply.onvif_events,
            onvif_port: reply.onvif_port,
            onvif_problem: reply.onvif_problem.as_deref(),
            streams: &reply.streams,
            onvif_refused: reply.onvif_refused,
        },
    ))
}

pub fn form(id: &str) -> Result<CameraForm, String> {
    let mut form = form::from_config(&camera(id)?);
    if server().is_some() && has_password(id) {
        form.password_note = PASSWORD_KEPT.to_string();
    }
    Ok(form)
}

pub fn blank_form() -> CameraForm {
    form::blank()
}

/// The desktop's checks, so the web build refuses the same forms.
pub fn form_problems(form: &CameraForm) -> Vec<FormProblem> {
    form::problems(form)
}

/// Saves the form as camera `id`'s settings, on the server or in the preview.
pub async fn save_form(id: String, form: CameraForm) -> Result<(), String> {
    known(&id)?;
    apply(camera_update(&id, &form)).await.map(drop)
}

/// The web build is never told a saved password, so an empty one keeps the server's.
fn camera_update(id: &str, form: &CameraForm) -> Change {
    Change::UpdateCameras {
        cameras: vec![CameraUpdate {
            id: CameraId::from(id),
            camera: form::to_config(form),
            keep_password: form.password.is_empty() && has_password(id),
        }],
    }
}

/// The desktop's checks of changes for several cameras.
pub fn change_problems(changes: &CameraChanges) -> Vec<FormProblem> {
    form_check::change_problems(changes)
}

/// Gives every camera in `ids` the changes, all or none, counting those that changed.
pub async fn save_changes(ids: Vec<String>, changes: CameraChanges) -> Result<u32, String> {
    let Some((change, changed)) =
        with_library(|saved| library::save_changes(saved, &ids, &changes))?
    else {
        return Ok(0);
    };
    apply(change).await?;
    Ok(changed.len() as u32)
}

/// Saves a new camera; resolves with its id. It is in no group and no view yet.
pub async fn add_camera(form: CameraForm) -> Result<String, String> {
    if let Some(problem) = form::problems(&form).first() {
        return Err(problem.message.clone());
    }
    apply(Change::AddCamera {
        camera: form::to_config(&form),
    })
    .await
    .map(|applied| applied.added_id())
    .map_err(|error| format!("Could not save the camera: {error}"))
}

/// Deletes camera `id` from the list, every group and every view.
pub async fn delete_camera(id: String) -> Result<(), String> {
    known(&id)?;
    apply(Change::DeleteCamera {
        id: CameraId::from(id.as_str()),
    })
    .await
    .map(drop)
    .map_err(|error| format!("Could not delete the camera: {error}"))
}

/// Moves camera `id` to `index` among the cameras.
pub async fn move_camera(id: String, index: u32) -> Result<(), String> {
    apply(library::move_camera(&id, index))
        .await
        .map(drop)
        .map_err(|error| format!("Could not move the camera: {error}"))
}

pub fn library_problems() -> Vec<String> {
    with_library(Library::problems)
}

pub fn preferences() -> UiPreferences {
    UiPreferences {
        rail_collapsed: false,
        rail_width: 248.0,
        sidebar_width: 232.0,
        appearance: ipcamera_core::config::Ui::default().appearance.into(),
    }
}

pub async fn save_preferences(_preferences: UiPreferences) {}

/// The page reloads on the first view, as it forgets everything else.
pub fn last_page() -> String {
    String::new()
}

pub async fn save_page(_page: String) {}

/// On, as the desktop's default: the web build has no events to alert about yet.
pub fn alerts() -> bool {
    true
}

pub async fn save_alerts(_on: bool) {}

/// None: the page forgets it as it reloads.
pub fn events_table_width() -> Option<f64> {
    None
}

pub async fn save_events_table_width(_width: f64) {}

/// Nothing saved: the browser keeps its own window.
pub fn saved_window() -> WindowPlace {
    WindowPlace::default()
}

pub async fn save_window(_place: WindowPlace) {}

/// A browser can't open or choose the server's folders.
pub fn storage_abilities() -> StorageAbilities {
    StorageAbilities {
        reveal_label: "Open in a new tab".to_string(),
        can_open_folder: false,
        can_choose_folder: false,
    }
}

/// The server's folder of `kind`, as it last said; the defaults until it has.
pub fn storage_folder(kind: &str) -> Result<StorageFolder, String> {
    let entry = WEB.with(|web| {
        let web = web.borrow();
        let storage = web.storage.as_ref().filter(|_| web.server.is_some())?;
        Some(storage_entry(storage, kind)?.clone())
    });
    match (kind, entry) {
        (_, Some(entry)) => Ok(StorageFolder {
            path: entry.path,
            is_default: entry.is_default,
        }),
        ("snapshots" | "videos", None) => Ok(StorageFolder {
            path: format!("data/{kind}"),
            is_default: true,
        }),
        (other, None) => Err(format!("no folder for `{other}`")),
    }
}

fn storage_entry<'a>(storage: &'a api::Storage, kind: &str) -> Option<&'a api::StorageEntry> {
    match kind {
        "snapshots" => Some(&storage.snapshots),
        "videos" => Some(&storage.videos),
        _ => None,
    }
}

pub async fn choose_storage(_kind: String) -> Result<StorageFolder, String> {
    Err(FOLDERS_ON_THE_SERVER.to_string())
}

pub fn reset_storage(_kind: &str) -> Result<StorageFolder, String> {
    Err(FOLDERS_ON_THE_SERVER.to_string())
}

pub async fn open_storage(_kind: &str) -> Result<(), String> {
    Err(FOLDERS_ON_THE_SERVER.to_string())
}

/// How full the drive under the server's folder of `kind` is, as the server says now.
pub async fn storage_space(kind: String) -> Result<DriveSpace, String> {
    storage_folder(&kind)?;
    let address =
        on_server("The free space on the drives needs ipcamera-server or the desktop app.")?;
    let storage = storage(&address).await?;
    let entry = storage_entry(&storage, &kind).ok_or_else(|| format!("no folder for `{kind}`"))?;
    if let Some(problem) = &entry.problem {
        return Err(problem.clone());
    }
    Ok(DriveSpace {
        drive: entry.drive.clone(),
        free: entry.free as f64,
        total: entry.total as f64,
    })
}

#[cfg(test)]
mod tests;
