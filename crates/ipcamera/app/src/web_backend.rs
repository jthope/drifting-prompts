//! The web build's backend: the server's cameras, groups and views, and their live video.
//!
//! A browser can't open an RTSP connection, so on the web `ipcamera-server` does that, and keeps
//! the cameras. This gives `bridge` the same functions as `camera` gives the desktop, over the
//! server's library:
//!
//! - **Reads come from a copy.** The bridge's reads are synchronous, so this holds a copy of the
//!   server's library: a `ipcamera_core` [`Library`] made from the snapshot `GET /api/library` sends
//!   ([`Library::from_snapshot`]), with every rule the desktop's has. The server never sends a
//!   password, only which cameras have one.
//! - **Changes go to the server.** Each is a `ipcamera_core` [`Change`], which the server applies to
//!   its own library (`POST /api/changes`), with the same rules and in the same words. This then
//!   takes a fresh copy, so the change shows at once, and a refusal reaches the usual toast.
//! - **Changes made elsewhere arrive.** While the app listens for changes, a task asks the server
//!   for its status (`GET /api/status`) every [`CHECK_EVERY`]: the library's revision, which takes
//!   a fresh copy when it moved, numbers that change with the snapshots and the recordings, and
//!   which cameras record and what their PTZ says. The bridge's synchronous reads answer from it.
//! - **The picture is the server's player.** A camera's tile shows the server's page for it, on
//!   the stream the tile picks ([`live_url`]), which plays it with the browser's own decoder.
//! - **The server does the rest.** A snapshot, recording, PTZ, a spotlight and Detect are the
//!   server's, in its folders and by the desktop's rules, and what a watched camera sees comes with
//!   its status, and the Snapshots and Videos pages show its files from their URLs.
//!   A PTZ move lasts only a second on the server unless sent again, so a held one is sent again
//!   every [`PTZ_RENEW_EVERY`] until it is let go: a page that goes away mid-move can't leave the
//!   camera turning. What a browser can't do, open or choose the server's folders, the bridge says
//!   ([`storage_abilities`]). Sound still needs the desktop app.
//!
//! The server is at this page's own address, as when it serves the packaged web build, unless the
//! build was made with `IPCAMERA_SERVER` set, as for `nsh run --target web`, which serves the page
//! from elsewhere. Until a server answers (or when none does) this is a preview over a pretend
//! camera, kept in this page's memory, so the layout can still be worked on with no server.
//!
//! **Signing in.** A server with a password answers nothing until someone signs in
//! ([`sign_in`]), which gives this page a session's key. It goes with every request, and in the
//! address of what the browser fetches itself: a tile's player, a picture, a file opened in a tab
//! ([`keyed`]). The key is kept in this page's memory only, so a reload asks for the password
//! again. A request the server answers with "sign in first" drops the key, and the page asks again
//! ([`sign_in_needed`]).
//!
//! Requests go through nsh's `fetch`, the browser's own on the web, so the web build needs no
//! JavaScript glue.

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

/// Where the server is when the page is served from elsewhere, as `nsh run --target web` serves
/// it: set at build time. Without it, the server is at this page's own address.
const SERVER: Option<&str> = option_env!("IPCAMERA_SERVER");

/// How often the server is asked for its status. A recording's state and a new snapshot show
/// within this, and the videos page's length of a recording moves on at this pace.
const CHECK_EVERY: Duration = Duration::from_secs(1);

/// How many checks in a row the server may leave unanswered before the page says it is out of
/// reach: one missed check, as a busy moment costs, changes nothing on screen.
const OUT_OF_REACH_AFTER: u32 = 3;

/// What every camera says while the server doesn't answer.
const OUT_OF_REACH: &str = "The server isn't answering. Check that ipcamera-server is running.";

/// How often a held PTZ move is sent again. The server stops a move that isn't within a second,
/// so this rides out a late request or two.
const PTZ_RENEW_EVERY: Duration = Duration::from_millis(400);

/// What a camera with no saved password says until someone presses Connect for it.
const NO_PASSWORD: &str =
    "Add the camera's password in Settings. If it has none, press Connect to try it without one.";

/// The status the server answers a request with no session with (`401 Unauthorized`).
const SIGN_IN_FIRST: u16 = 401;

/// The camera the preview starts with.
const NAME: &str = "Preview camera";
/// What a camera with no address yet says where the desktop shows its login and stream.
const NO_ADDRESS: &str = "No address yet";
const NOT_HERE: &str = "That needs ipcamera-server or the desktop app.";
/// Why a change to the server didn't happen, when the server didn't answer.
const NO_ANSWER: &str =
    "The server didn't answer. Check that ipcamera-server is running, then try again.";
/// Why an answer of the server's couldn't be used.
const UNREADABLE: &str = "The server's answer couldn't be read. Reload the page.";
/// The settings form's note under the password, for a camera whose password the server keeps.
const PASSWORD_KEPT: &str = "Leave it empty to keep the saved password.";
/// Why the server's folders can't be changed from a browser.
const FOLDERS_ON_THE_SERVER: &str =
    "The server's folders are set in its settings.json, or from the desktop app.";

/// What the web build holds: the server's library, or the preview's.
struct Web {
    /// The cameras, groups and views: a copy of the server's, or the preview's own.
    library: Library,
    /// Where the server is, once the copy is the server's; `None` while this is the preview.
    server: Option<String>,
    /// The revision of the server's library the copy was taken at.
    server_revision: Option<u64>,
    /// The cameras whose password the server keeps.
    with_password: HashSet<CameraId>,
    /// Where the server's player comes from, which the server says with its library.
    player: String,
    /// Added to the copy's own revision, which starts again at 0 with each fresh copy, so the
    /// page still hears about every change.
    base: u64,
    /// Whether a task is checking the server.
    checking: bool,
    /// What the server last said of itself: its revisions, and the cameras that record or have
    /// PTZ.
    status: Status,
    /// Where the server saves, as it last said.
    storage: Option<api::Storage>,
    /// Why the server refused a camera's last PTZ move, until one goes through.
    ptz_refused: HashMap<CameraId, String>,
    /// The PTZ move held now, sent again until it is let go.
    held: Option<Hold>,
    /// How many PTZ moves have been pressed, which numbers each [`Hold`].
    presses: u64,
    /// How many checks of the server in a row got no answer. [`OUT_OF_REACH_AFTER`] of them and
    /// the server counts as out of reach, until one is answered.
    unanswered: u32,
    /// The session's key, once someone has signed in to a server with a password.
    key: Option<String>,
    /// Whether the server wants someone to sign in before it answers.
    sign_in_needed: bool,
    /// What the server's snapshots and videos took when they were last added up, with the
    /// revisions of its folders then: added up again only when those change.
    usage: Option<((u64, u64), MediaUsage)>,
}

/// A PTZ move held down.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hold {
    /// Which press this is: a newer press or a release ends it.
    press: u64,
    request: PtzMove,
}

impl Web {
    /// The preview: one pretend camera with no address, in the first view.
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

    /// The server turned away a request sent with `key` for having no session: it wants someone
    /// to sign in. A key that has been replaced since is left alone.
    fn turned_away(&mut self, key: Option<&str>) {
        if self.key.as_deref() == key {
            self.key = None;
            self.sign_in_needed = true;
        }
    }

    /// Counts a check of the server: one it answered, or one it didn't.
    fn checked(&mut self, answered: bool) {
        self.unanswered = if answered {
            0
        } else {
            self.unanswered.saturating_add(1)
        };
    }

    /// Whether the server has stopped answering: what it last said of its cameras may no longer
    /// be so.
    fn out_of_reach(&self) -> bool {
        self.server.is_some() && self.unanswered >= OUT_OF_REACH_AFTER
    }

    fn revision(&self) -> u64 {
        self.base + self.library.revision()
    }

    /// Takes a fresh copy of the library from the server at `server`, whose player comes from
    /// `player`.
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
    /// What the page holds. One per thread, so tests running side by side each start from the
    /// preview; the page has one.
    static WEB: RefCell<Web> = RefCell::new(Web::preview());
}

/// Calls `use_it` with the cameras, groups and views, for `library`'s functions.
pub fn with_library<R>(use_it: impl FnOnce(&Library) -> R) -> R {
    WEB.with(|web| use_it(&web.borrow().library))
}

/// The server, once the copy is the server's.
fn server() -> Option<String> {
    WEB.with(|web| web.borrow().server.clone())
}

/// Bumped by every change to the cameras, groups or views, here or on the server. Reading it also
/// keeps the copy in step with the server, from the listener that reads it.
pub fn revision() -> u64 {
    keep_in_step();
    WEB.with(|web| web.borrow().revision())
}

/// Starts checking the server, unless a task already does. The task runs under the listener that
/// read the revision, and ends with it.
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
    // The task runs on until its owner ends; the handle is only for awaiting or cancelling it.
    drop(spawn(async {
        let _checking = Checking;
        loop {
            check().await;
            timer(CHECK_EVERY).await;
        }
    }));
}

/// Where requests go: the build's `IPCAMERA_SERVER`, or this page's own address.
fn server_address() -> String {
    SERVER.unwrap_or("").trim_end_matches('/').to_string()
}

/// Asks the server for its status, and takes a fresh copy of its library when that has changed
/// since the last one. A server that doesn't answer leaves the copy as it is, and after
/// [`OUT_OF_REACH_AFTER`] such checks in a row every camera says so ([`state_of`]), until it
/// answers again.
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

/// Takes a fresh copy of the server's library, and of where it saves.
async fn refresh(address: &str) -> Result<(), String> {
    let body = call(HttpRequest::get(format!("{address}/api/library"))).await?;
    let unreadable = |_| "The server's library couldn't be read. Reload the page.".to_string();
    // The copy, and beside it where the server's player comes from.
    let answer: serde_json::Value = serde_json::from_slice(&body).map_err(unreadable)?;
    let player = answer["player"].as_str().unwrap_or(address).to_string();
    let copy: ClientCopy = serde_json::from_value(answer).map_err(unreadable)?;
    WEB.with(|web| web.borrow_mut().take(address.to_string(), copy, player));
    let _ = storage(address).await;
    Ok(())
}

/// Where the server saves, and how full its drives are, as it says now; kept for the folders
/// Settings shows.
async fn storage(address: &str) -> Result<api::Storage, String> {
    let storage: api::Storage = ask(HttpRequest::get(format!("{address}/api/storage"))).await?;
    WEB.with(|web| web.borrow_mut().storage = Some(storage.clone()));
    Ok(storage)
}

/// Sends `request` and reads the answer as `T`.
async fn ask<T: DeserializeOwned>(request: HttpRequest) -> Result<T, String> {
    let body = call(request).await?;
    serde_json::from_slice(&body).map_err(|_| UNREADABLE.to_string())
}

/// The server, once the copy is the server's, or `missing` as an error: what needs the server.
fn on_server(missing: &str) -> Result<String, String> {
    server().ok_or_else(|| missing.to_string())
}

/// Sends `request`, with the session's key when there is one, resolving with the body of a
/// success, or with a sentence saying why not: the server's own sentence when it refused. A
/// server that asks to sign in first has the page ask for its password.
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

/// `url` with the session's key in it, when there is one: for what the browser fetches itself,
/// where no header can go with the request.
fn keyed(url: String) -> String {
    match WEB.with(|web| web.borrow().key.clone()) {
        Some(key) => {
            let joint = if url.contains('?') { '&' } else { '?' };
            format!("{url}{joint}key={key}")
        }
        None => url,
    }
}

/// Whether the server wants its password before it answers. Reading it keeps the status in step,
/// as [`revision`] does, which is how a server that starts asking is noticed.
pub fn sign_in_needed() -> bool {
    keep_in_step();
    WEB.with(|web| web.borrow().sign_in_needed)
}

/// Whether someone has signed in to the server from this page.
pub fn signed_in() -> bool {
    WEB.with(|web| web.borrow().key.is_some())
}

/// Signs in to the server with its `password`, and takes its library. The error is the server's
/// sentence: a wrong password, or one tried too soon after a wrong one.
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
    // The change is made either way; a copy that fails to come now comes with the next check.
    let _ = refresh(&address).await;
    Ok(applied)
}

/// A `POST` of `body` as JSON.
fn json_post(url: String, body: &impl serde::Serialize) -> Result<HttpRequest, String> {
    let body = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    Ok(HttpRequest::post(url, body).header("Content-Type", "application/json"))
}

/// Makes a change in the preview's memory, until the page reloads.
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

/// Whether the server keeps a password for camera `id`.
fn has_password(id: &str) -> bool {
    WEB.with(|web| web.borrow().with_password.contains(&CameraId::from(id)))
}

/// Who it logs in as and where, as the desktop says it: `admin@192.0.2.10:554  ·  ch1 main`; or
/// that there is no address yet.
fn stream_label(camera: &CameraConfig) -> String {
    if camera.host.is_empty() {
        NO_ADDRESS.to_string()
    } else {
        camera.stream_label()
    }
}

/// A camera as the web build shows it: playing through the server, recording there and moving
/// through it, as the server last said; or a preview that never connects. While the server doesn't
/// answer, what it last said is not shown as if it still held: the camera reads as not connected,
/// not recording and seeing nothing, and its status says the server isn't answering.
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
    // A stream that gave up, or a camera with no saved password that nobody has pressed Connect
    // for: neither plays, and both say what to do.
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
        // The server's player draws the picture, and keeps its numbers to itself.
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

/// The server's player for camera `id` on `stream` (`""` for its own), which a tile shows; `""` in
/// the preview, which has none.
///
/// It comes from where the server says, which is never this page's own origin: nsh takes the
/// origin away from a frame of the page's own, and the browser then refuses that frame's
/// connection to the server.
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

/// Has the server try camera `id` again: one whose stream gave up, a rejected login included, or
/// one with no saved password, which the server never tries by itself. The tile's player connects
/// again by itself.
pub async fn connect(id: String) -> Result<(), String> {
    known(&id)?;
    let address = on_server("Live video needs ipcamera-server or the desktop app.")?;
    let url = format!("{address}/api/cameras/{}/connect", path_segment(&id));
    call(HttpRequest::post(url, Vec::new())).await?;
    // The status shows the new try now rather than at the next check.
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

/// The tiles connect by themselves, each to the server's player, which keeps a camera's relay up
/// for a while after its last viewer leaves; the web keeps nothing longer than that.
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
    // The gallery hears of them now rather than at the next check.
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

/// One of the server's URLs, `url`, made whole with its `address`; none is `""`, as TSX takes it.
fn served(address: &str, url: Option<&str>) -> String {
    url.map(|url| keyed(format!("{address}{url}")))
        .unwrap_or_default()
}

/// A snapshot the server at `address` lists, as the gallery takes it.
fn snapshot_info(address: &str, entry: SnapshotEntry) -> SnapshotInfo {
    SnapshotInfo {
        path: keyed(format!("{address}{}", entry.url)),
        thumbnail: served(address, entry.thumbnail.as_deref()),
        camera: entry.camera.map(|id| id.to_string()).unwrap_or_default(),
        tags: entry.tags,
        // The web keeps no event log, so a file has no event to show.
        event: String::new(),
        taken_at: entry.taken_ms as f64,
        bytes: entry.bytes as f64,
        width: entry.size.map_or(0, |size| size.width),
        height: entry.size.map_or(0, |size| size.height),
        name: entry.name,
    }
}

/// Changes whenever the server's snapshots do. Reading it keeps the status in step, as
/// [`revision`] does. It reads what the server last said, so it never waits: async only because
/// the desktop's lists a folder.
pub async fn snapshots_revision() -> Option<u64> {
    keep_in_step();
    Some(WEB.with(|web| web.borrow().status.snapshots))
}

/// Why the web build has no events: the server doesn't log them yet.
const NO_EVENT_LOG: &str =
    "The server keeps no event log yet. The desktop app logs what the cameras see.";

/// Whether the app keeps an event log: the server doesn't log events yet, so the web build has
/// none.
pub fn events_kept() -> bool {
    false
}

/// The events: none on the web, which has no log yet.
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

/// Why the web build has no automation: the server doesn't do it yet.
const NO_AUTOMATION: &str =
    "The server does nothing by itself yet. The desktop app records what the cameras see.";

/// Whether the app does anything by itself: the server doesn't yet, so the web build doesn't.
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

/// What the server's snapshots and videos take, and the room left on its videos' drive: from its
/// lists, read again only when its folders' revisions change; nothing in the preview. The server
/// keeps no media limit.
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
pub fn reveal_snapshot(name: &str) -> Result<(), String> {
    open_file("snapshots", name)
}

pub fn open_snapshots_folder() -> Result<(), String> {
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

/// A recording the server at `address` lists, as the videos page takes it.
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

/// Changes whenever the server's recordings do, one being written included. Async as
/// [`snapshots_revision`] is.
pub async fn videos_revision() -> Option<u64> {
    keep_in_step();
    Some(WEB.with(|web| web.borrow().status.videos))
}

/// Deletes the server's recording called `name` for good; one still recording is refused.
pub async fn delete_video(name: String) -> Result<(), String> {
    delete_file("videos", &name).await
}

/// Opens the recording in a new tab, where the browser can play or save it.
pub fn reveal_video(name: &str) -> Result<(), String> {
    open_file("videos", name)
}

pub fn open_videos_folder() -> Result<(), String> {
    Err(FOLDERS_ON_THE_SERVER.to_string())
}

/// Deletes the file called `name` from the server's `kind` (`snapshots` or `videos`).
async fn delete_file(kind: &str, name: &str) -> Result<(), String> {
    let address = on_server(NOT_HERE)?;
    let url = format!("{address}/api/{kind}/{}", path_segment(name));
    call(HttpRequest::new("DELETE", url)).await?;
    check().await;
    Ok(())
}

/// Opens the server's file of `kind` called `name` in a new tab. From the player's address, which
/// is always whole, so it works whether or not the page came from the server.
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

/// Starts or changes camera `id`'s PTZ move on the server, and sends it again every
/// [`PTZ_RENEW_EVERY`] until [`ptz_stop`] or another move, when it resolves: the server stops a
/// move that isn't sent again. A refused move says why in the camera's PTZ status. In the
/// preview, nothing moves.
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

/// Lets go of camera `id`'s PTZ move, and tells the server to stop it. A stop that doesn't arrive
/// still stops the camera: the server lets the move run out.
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

/// Holds `request` as the move now, in place of any other; returns which press it is.
fn hold(request: PtzMove) -> u64 {
    WEB.with(|web| {
        let mut web = web.borrow_mut();
        web.presses += 1;
        let press = web.presses;
        web.held = Some(Hold { press, request });
        press
    })
}

/// The move of `press`, while it is still held.
fn held(press: u64) -> Option<PtzMove> {
    WEB.with(|web| {
        web.borrow()
            .held
            .filter(|held| held.press == press)
            .map(|held| held.request)
    })
}

/// Lets go of `press`, unless a newer press has taken its place.
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

/// Asks the server which camera is at the form's address. A form that edits camera `id` and leaves
/// its password empty has the server log in with the saved one.
pub async fn detect(id: String, form: CameraForm) -> Result<DetectedCamera, String> {
    // The preview has pretend cameras and no server to ask one.
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

/// The change that saves `form` as camera `id`'s settings. The web build is never told a saved
/// password, so its form shows none, and an empty one keeps what the server has.
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

/// Gives every camera in `ids` the changes, as the desktop does: all or none, counting those that
/// changed.
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

/// Deletes camera `id` from the list, every group and every view, as the desktop does.
pub async fn delete_camera(id: String) -> Result<(), String> {
    known(&id)?;
    apply(Change::DeleteCamera {
        id: CameraId::from(id.as_str()),
    })
    .await
    .map(drop)
    .map_err(|error| format!("Could not delete the camera: {error}"))
}

/// Moves camera `id` to `index` among the cameras, on the server or in the preview's memory, as
/// the desktop does.
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

/// The page forgets it as it reloads, as it does the page it showed: the columns' own width.
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

/// The server's folder of `kind`, as it last said; the defaults in the preview, or before the
/// server has said.
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

/// The server's folder of `kind` in `storage`.
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

pub fn open_storage(_kind: &str) -> Result<(), String> {
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
