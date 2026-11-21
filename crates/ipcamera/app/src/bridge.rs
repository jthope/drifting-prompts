//! What TSX sees of the app: the types and functions it imports from `ipcamera/native`.

use std::time::Duration;

use dp_ui::{SavedAppearance, WindowPlace, listen, listen_later};
use ipcamera_core::Change;
use nsh_core::prelude::*;

#[cfg(windows)]
use crate::camera as backend;
#[cfg(not(windows))]
use crate::web_backend as backend;
use crate::{form, library};

/// How often a listener checks for a change.
const REFRESH_EVERY: Duration = Duration::from_millis(250);

/// How often the media sizes are added up for the status bar.
const USAGE_CHECK_EVERY: Duration = Duration::from_secs(3);

/// How often the snapshots and videos folders are listed.
const FOLDER_CHECK_EVERY: Duration = Duration::from_secs(1);

/// How a status should read.
#[derive(Clone, Copy, Debug, PartialEq, PropEnum)]
pub enum StatusTone {
    /// Nothing to report.
    Normal,
    /// Working as it should.
    Ok,
    /// Needs attention.
    Warn,
}

/// What the snapshots and videos take, and the room left for them, for the status bar.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct MediaUsage {
    /// What the recordings take, in bytes.
    pub videos: f64,
    /// What the snapshots take, in bytes.
    pub snapshots: f64,
    /// What is left on the drive under the videos folder, in bytes; 0 when `drive` is `""`.
    pub free: f64,
    /// That drive, as Explorer names it (`D:`); `""` when its room can't be told.
    pub drive: String,
    /// The most the snapshots and videos may hold together, in bytes; 0 for no limit.
    pub limit: f64,
    /// Whether the oldest are deleted to make room at that limit.
    pub make_room: bool,
}

/// One of a camera's streams that runs, with its numbers.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct RunningStream {
    /// Which stream, as `StreamChoice.key` names it: `""` for the camera's own.
    pub stream: String,
    /// Size, frame rate and transport: `1920×1080 · 25.0 fps · TCP`.
    pub stats: String,
    /// Size and frame rate alone, for a narrow tile: `1920×1080 · 25 fps`.
    pub brief: String,
    /// Whether it has sent no picture yet: it is connecting, or waiting for its first keyframe.
    pub waiting: bool,
    /// Whether it lost the camera or gave up on it, which its tiles say over the picture.
    pub trouble: StreamTrouble,
    /// What went wrong, as a sentence, while `trouble` isn't `fine`; `""` while it is.
    pub problem: String,
    /// When its last picture arrived, in milliseconds since 1970; 0 before its first.
    pub last_picture_at: f64,
}

/// What a stream's tiles say over its picture while no new one comes.
#[derive(Clone, Copy, Debug, PartialEq, PropEnum)]
pub enum StreamTrouble {
    /// Nothing: it is connecting, or pictures arrive.
    Fine,
    /// It lost the camera and connects again by itself.
    Reconnecting,
    /// It stopped and won't try again by itself: Connect tries again.
    Stopped,
    /// The camera refused its login; nothing retries until the settings change.
    Refused,
}

/// A camera in a list.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct CameraSummary {
    /// What every other call names it by.
    pub id: String,
    /// The name the user gave it.
    pub name: String,
    /// Who it logs in as and where: `admin@192.0.2.10:554 · ch1 main`.
    pub stream_label: String,
}

/// What a camera is doing right now.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct CameraState {
    /// Which camera this is.
    pub id: String,
    /// The name the user gave it.
    pub name: String,
    /// Who it logs in as and where: `admin@192.0.2.10:554 · ch1 main`.
    pub stream_label: String,
    /// Whether a stream runs or tries to, one that gave up included: Connect or Disconnect.
    pub connected: bool,
    /// Whether it may connect by itself: without a password it waits to be asked.
    pub ready: bool,
    /// What the stream is doing, as a sentence.
    pub status: String,
    /// How `status` should read.
    pub tone: StatusTone,
    /// Size, frame rate and transport of the stream that stands for the camera; `""` with none.
    pub stats: String,
    /// Where the camera is on the network: its host, as its tiles show it (`192.168.1.64`).
    pub address: String,
    /// Each of its streams that runs, with its numbers; empty where the app doesn't draw it.
    pub streams: Vec<RunningStream>,
    /// What PTZ last reported.
    pub ptz_status: String,
    /// Whether the audio is muted.
    pub muted: bool,
    /// The volume, 0 to 100.
    pub volume: f64,
    /// Whether it is recording, or waiting for its stream to record.
    pub recording: bool,
    /// Why its last recording stopped by itself, until the next one starts; `""` when none did.
    pub record_problem: String,
    /// Whether something moves in its picture, for a camera the app watches (a Reolink).
    pub motion: bool,
    /// What its AI sees now, by `DetectionInfo` key; empty for nothing or an unwatched camera.
    pub detected: Vec<String>,
    /// Whether it has a spotlight the app can switch (`Cameras.setSpotlight`).
    pub spotlight: bool,
    /// Whether that spotlight is on now.
    pub spotlight_on: bool,
    /// What it says it is and has, or why watching it failed; `""` for an unwatched camera.
    pub watch_status: String,
}

/// The camera settings form. Numbers stay text so a half-typed value is not lost.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct CameraForm {
    /// The camera's name.
    pub name: String,
    /// Who made it, by `BrandInfo` key: `dahua`, `reolink` or `other`.
    pub brand: String,
    /// Its address or host name.
    pub host: String,
    /// The login's user name.
    pub user: String,
    /// The login's password.
    pub password: String,
    /// The RTSP port.
    pub rtsp_port: String,
    /// The camera channel, from 1.
    pub channel: String,
    /// 0 for the main stream, 1 for the sub stream.
    pub sub_stream: u32,
    /// The RTSP path: one of `streams`' or what was typed; blank for the brand's path.
    pub rtsp_path: String,
    /// Force RTSP over TCP.
    pub use_tcp: bool,
    /// Reconnect by itself after losing the stream.
    pub auto_reconnect: bool,
    /// The ONVIF port.
    pub onvif_port: String,
    /// The DVRIP port.
    pub dvrip_port: String,
    /// The ONVIF device path.
    pub onvif_path: String,
    /// The port of the camera's own HTTP API: a Reolink's, or a Dahua's with `httpApi` on.
    pub api_port: String,
    /// Whether that API is HTTPS.
    pub api_https: bool,
    /// For a Dahua: whether PTZ and light go over its HTTP API rather than DVRIP.
    pub http_api: bool,
    /// Whether the app watches its ONVIF events for motion, for a camera that isn't a Reolink.
    pub onvif_events: bool,
    /// A note under the password field; `""` when there is nothing to say.
    pub password_note: String,
    /// The streams the camera listed when Detect last asked with a login; empty until then.
    pub streams: Vec<StreamInfo>,
    /// Whether the camera's ONVIF refused this login when Detect asked, which then leaves it alone.
    pub onvif_refused: bool,
    /// Whether the app logs what the camera sees as events; on for a new camera.
    pub log_events: bool,
    /// Whether the camera's event videos follow `eventVideo` rather than Settings → Automation.
    pub own_event_video: bool,
    /// How the camera's event videos are made with `ownEventVideo`; Automation's otherwise.
    pub event_video: EventVideoSettings,
}

/// How the videos an event saves are made; out-of-range numbers are brought within range on save.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct EventVideoSettings {
    /// Seconds a video keeps recording after the camera last saw something, 10 to 120.
    pub after_seconds: u32,
    /// Seconds from before the camera saw something each video starts with, 5 to 30, or 0 for none.
    pub before_seconds: u32,
    /// Minutes a video runs before it goes on in a new file, up to 60; 0 for no limit.
    pub longest_minutes: u32,
    /// Whether plain motion keeps a video going between the AI's detections, for up to two minutes.
    pub motion_continues: bool,
    /// Which stream the videos record: `own`, `main`, `sub`, or a path the camera listed.
    pub stream: String,
    /// What the camera has to see for a video, by `DetectionInfo` key; motion-only ones ignore it.
    pub kinds: Vec<String>,
}

/// One of the streams a camera says it has, in the camera's own words.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct StreamInfo {
    /// What the camera calls it.
    pub name: String,
    /// Its path for `CameraForm.rtspPath`: an RTSP path, or `flv:` and a Reolink's FLV stream.
    pub path: String,
    /// What the camera says of its picture (`3840×2160, H265`); `""` when it says nothing.
    pub detail: String,
    /// What a list shows for it, as `StreamChoice.title` is: its size, codec and name.
    pub title: String,
    /// The lens it is of (`Tracking lens`); `""` for the main lens.
    pub lens: String,
}

/// A brand the camera form offers.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct BrandInfo {
    /// What the form saves: `dahua`, `reolink`, `other`.
    pub key: String,
    /// What the form calls it.
    pub title: String,
    /// The RTSP path a blank override stands for, for the field's help: channel 1's main stream.
    pub rtsp_path: String,
}

/// Something a camera can tell apart in its picture.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct DetectionInfo {
    /// What `CameraState.detected` names it by: `person`, `vehicle`, `animal`, `face`, `package`.
    pub key: String,
    /// What the app calls it.
    pub title: String,
}

/// What `Cameras.detect` found at a camera's address.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct DetectedCamera {
    /// The form it was asked with, filled in with what was found.
    pub form: CameraForm,
    /// What was found, as a sentence: `Reolink RLC-811A, firmware v3.1.0.2368.`
    pub summary: String,
    /// What the camera has, in words (`Motion`, `Pan and tilt`); empty when unknown or nothing.
    pub features: Vec<String>,
}

/// A camera `Cameras.discover` found on the network, with what the camera form takes from it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct FoundCamera {
    /// Its address on the network, for the form's address.
    pub host: String,
    /// The name the camera gives itself; `""` when it gives none.
    pub name: String,
    /// Its model, as it says it; `""` when it says none.
    pub model: String,
    /// The port its ONVIF service is on, as text; `""` for a device found by its video alone.
    pub onvif_port: String,
    /// The path of its ONVIF service; `""` for a device found by its video alone.
    pub onvif_path: String,
    /// The port it answers RTSP on, as text; `""` where it wasn't tried or didn't answer.
    pub rtsp_port: String,
    /// Whatever else the camera says of itself, as `Kind: what it says` (`MAC: aa:bb:cc:00:11:22`).
    pub details: Vec<String>,
}

/// A field of the settings form that cannot be saved as it is.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct FormProblem {
    /// Which field, named as in `CameraForm`.
    pub field: String,
    /// What is wrong with it, as a sentence.
    pub message: String,
}

/// Settings to give several cameras at once; a field left `null` keeps each camera's own.
#[derive(Clone, Debug, Default, PartialEq, JsObject)]
pub struct CameraChanges {
    /// The address or host name.
    pub host: Option<String>,
    /// The login's user name.
    pub user: Option<String>,
    /// The login's password.
    pub password: Option<String>,
    /// The RTSP port.
    pub rtsp_port: Option<String>,
    /// 0 for the main stream, 1 for the sub stream.
    pub sub_stream: Option<u32>,
    /// The RTSP path, or blank for the brand's.
    pub rtsp_path: Option<String>,
    /// Force RTSP over TCP.
    pub use_tcp: Option<bool>,
    /// Reconnect by itself after losing the stream.
    pub auto_reconnect: Option<bool>,
    /// The ONVIF port.
    pub onvif_port: Option<String>,
    /// The DVRIP port.
    pub dvrip_port: Option<String>,
    /// The ONVIF device path.
    pub onvif_path: Option<String>,
}

/// A group of cameras, which lists them by id.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct GroupInfo {
    /// What every other call names it by.
    pub id: String,
    /// The name the user gave it.
    pub name: String,
    /// The ids of its cameras, in order. Only cameras that exist.
    pub cameras: Vec<String>,
}

/// A saved view: where its tiles sit, and the camera in each.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct ViewInfo {
    /// What every other call names it by.
    pub id: String,
    /// The name the user gave it.
    pub name: String,
    /// The key of the `LayoutInfo` its tiles are; `""` for an arrangement of their own.
    pub layout: String,
    /// Where each tile sits on the view's grid (`Views.grid`), in the order of `tiles`.
    pub cells: Vec<CellInfo>,
    /// The key of the `FitInfo` a tile shows its picture with until it picks its own.
    pub fit: String,
    /// The camera id in each tile, in the order of `cells`; `""` for an empty tile.
    pub tiles: Vec<String>,
    /// Each tile's stream: a `Cameras.streamChoices` key, `own` for its own, or `""` for Auto.
    pub streams: Vec<String>,
    /// The key of the `FitInfo` each tile shows its picture with: its own, else the view's `fit`.
    pub fits: Vec<String>,
    /// The camera ids of the tiles past `cells`, kept for when the view has room; `""` for empty.
    pub hidden_tiles: Vec<String>,
    /// The stream each of `hiddenTiles` shows, as `streams` has it, in the same order.
    pub hidden_streams: Vec<String>,
    /// The fit each of `hiddenTiles` shows with, as `fits` has it, in the same order.
    pub hidden_fits: Vec<String>,
}

/// One of a camera's streams, as a tile plays it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct ShownStream {
    /// The camera's id; `""` for none.
    pub camera: String,
    /// The stream, as `StreamChoice.key` names it, `""` or `own` for the camera's own; never Auto.
    pub stream: String,
}

/// One picture of a camera as the stage draws it, which `Cameras.autoStream` picks a stream for.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct DrawnPicture {
    /// How wide it is drawn, in device pixels, its zoom included.
    pub width: f64,
    /// How tall it is drawn, in device pixels, its zoom included.
    pub height: f64,
    /// How it fills its place: a `Views.fits` key.
    pub fit: String,
}

/// One of a camera's streams, as the menus that pick one list it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct StreamChoice {
    /// `""` for the camera's own, which a tile saves as `own`; else `main`, `sub`, or a path.
    pub key: String,
    /// The camera's own name for it; `Main stream`, `Sub stream` or the path where it named none.
    pub name: String,
    /// What a menu shows for it: its size, codec and name, those the camera said.
    pub title: String,
    /// Its picture's size, `1920×1080`; `""` when the camera didn't say.
    pub size: String,
    /// The lens it is of (`Tracking lens`); `""` for the main lens.
    pub lens: String,
    /// Whether it is the camera's own stream: the one it records, and Auto's without sizes.
    pub own: bool,
}

/// One tile's place on a view's grid (`Views.grid`), in whole grid cells from the top left.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct CellInfo {
    /// The first column, from 0.
    pub column: u32,
    /// The first row, from 0.
    pub row: u32,
    /// How many columns the tile covers.
    pub column_span: u32,
    /// How many rows the tile covers.
    pub row_span: u32,
}

/// A way to divide a view into tiles, which a view starts as and can be set to again.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct LayoutInfo {
    /// What `Views.add` and `Views.setLayout` take, and `ViewInfo.layout` says.
    pub key: String,
    /// What the picker calls it.
    pub title: String,
    /// Where each tile goes on a view's grid; the large tile, when there is one, first.
    pub cells: Vec<CellInfo>,
}

/// The grid a view's tiles sit on.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct GridInfo {
    /// How many columns, and how many rows, it has.
    pub size: u32,
    /// The fewest columns or rows a tile covers.
    pub smallest: u32,
}

/// One tile of a view as its layout is edited, for `Views.arrange`.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct ArrangedTileInfo {
    /// The first column, from 0.
    pub column: u32,
    /// The first row, from 0.
    pub row: u32,
    /// How many columns it covers.
    pub column_span: u32,
    /// How many rows it covers.
    pub row_span: u32,
    /// The tile it was, from 0, so it keeps that tile's camera; `null` for a new, empty one.
    pub was: Option<u32>,
}

/// A way for a picture to fill its tile.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct FitInfo {
    /// What a view and a tile save.
    pub key: String,
    /// What the menu calls it.
    pub title: String,
    /// Whether it forces the picture to a shape of its own (`1:1`), which the menu lists apart.
    pub reshapes: bool,
}

/// A saved snapshot, as the gallery shows it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct SnapshotInfo {
    /// The file's name, which every other call names it by.
    pub name: String,
    /// Where the file is, for `<Image src>`: a path on the desktop, the server's URL on the web.
    pub path: String,
    /// It made small, which the gallery shows rather than the full picture; `""` until it has one.
    pub thumbnail: String,
    /// The id of the camera that took it; `""` when its name doesn't say.
    pub camera: String,
    /// What the camera saw as an event took it (`vehicle`, `motion`); empty for one taken by hand.
    pub tags: Vec<String>,
    /// The event that took it, by id, while the log has it; `""` otherwise.
    pub event: String,
    /// When it was taken, in milliseconds since 1970, as `Date` takes it.
    pub taken_at: f64,
    /// Its size on disk, in bytes.
    pub bytes: f64,
    /// Its width in pixels; 0 when its header can't be read.
    pub width: u32,
    /// Its height in pixels; 0 when its header can't be read.
    pub height: u32,
}

/// A recording, as the videos page shows it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct VideoInfo {
    /// The file's name, which every other call names it by.
    pub name: String,
    /// Where the file is, for `<Video src>`: a path on the desktop, the server's URL on the web.
    pub path: String,
    /// Its first picture, for `<Image src>` as `path` is; `""` while it has none.
    pub thumbnail: String,
    /// The id of the camera that made it, or `""` when its name doesn't say.
    pub camera: String,
    /// What the camera saw as an event started it; empty for one someone started.
    pub tags: Vec<String>,
    /// The event it recorded, by id, while the log has it; `""` otherwise, and always on the web.
    pub event: String,
    /// When it started, in milliseconds since 1970, as `Date` takes it.
    pub started_at: f64,
    /// How long it runs, in seconds, to the file's last write.
    pub seconds: f64,
    /// Its size on disk, in bytes.
    pub bytes: f64,
    /// Whether a camera is still writing it.
    pub recording: bool,
}

/// Something a camera saw, as the events page and the alerts show it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct EventInfo {
    /// What every other call names it by.
    pub id: String,
    /// The id of the camera that saw it, which may have been deleted since.
    pub camera: String,
    /// When it started, in milliseconds since 1970, as `Date` takes it.
    pub started_at: f64,
    /// When the camera last saw it, as `startedAt` is; 0 while it sees it now.
    pub ended_at: f64,
    /// Whether the camera sees it now.
    pub ongoing: bool,
    /// Whether the camera saw motion during it.
    pub motion: bool,
    /// What the camera's AI told apart during it, by `DetectionInfo` key; empty for motion only.
    pub detected: Vec<String>,
    /// How many times the camera saw something during it, each after ten seconds of nothing.
    pub sightings: u32,
    /// Its snapshot, as the snapshots page names it; `""` when it took none or it was deleted.
    pub snapshot: String,
    /// That snapshot, for the viewer's `<Image src>`: an absolute path; `""` without one.
    pub picture: String,
    /// That snapshot made small, for a row's or a tile's `<Image src>`; `""` until it has one.
    pub thumbnail: String,
    /// Its recordings, as the videos page names them, oldest first, without those deleted since.
    pub videos: Vec<String>,
}

/// What each event saves into the snapshots and videos folders.
#[derive(Clone, Copy, Debug, PartialEq, PropEnum)]
pub enum EventCaptureInfo {
    /// Nothing: the event is only a note in the log.
    Nothing,
    /// A snapshot as it starts.
    Picture,
    /// A recording of each sighting.
    Video,
    /// Both.
    PictureAndVideo,
}

/// What the app does by itself and how much media it may keep; sizes are in MiB.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct AutomationSettings {
    /// Whether the app watches every camera and logs what they see; off, events save nothing.
    pub log_events: bool,
    /// What each event saves into the snapshots and videos folders, tagged with what was seen.
    pub event_capture: EventCaptureInfo,
    /// Minutes in which what a camera sees again still joins the same event; 0 for none.
    pub event_window_minutes: u32,
    /// The most the snapshots and videos may hold together, in MiB; 0 for no limit.
    pub media_limit_mb: f64,
    /// The largest a video file grows before a new one starts, in MiB; 0 for no limit.
    pub video_limit_mb: f64,
    /// Whether the oldest media are deleted for good at the limit; one still recording never is.
    pub overwrite_oldest: bool,
    /// How the videos events save are made, for every camera that doesn't have its own.
    pub event_video: EventVideoSettings,
}

/// Window preferences, restored on the next launch.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct UiPreferences {
    /// Whether the PTZ panel is hidden.
    pub rail_collapsed: bool,
    /// The PTZ panel's width, in points.
    pub rail_width: f64,
    /// The sidebar's width, in points, as it was last dragged to.
    pub sidebar_width: f64,
    /// How the window looks: its mode, themes, texture and elevation.
    pub appearance: SavedAppearance,
}

/// One of the folders the app saves into, for Settings' General section.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct StorageFolder {
    /// Where it is: an absolute path.
    pub path: String,
    /// Whether it is the default folder beside the app, not one the user chose.
    pub is_default: bool,
}

/// What can be done with the app's folders where it runs; on the web they are the server's.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct StorageAbilities {
    /// What a file's menu calls showing it: `Show in Explorer`, or `Open in a new tab` on the web.
    pub reveal_label: String,
    /// Whether a folder can be opened in the system's file manager.
    pub can_open_folder: bool,
    /// Whether another folder can be chosen, or the default chosen again.
    pub can_choose_folder: bool,
}

/// How full the drive under one of the app's folders is, for the dashboard.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct DriveSpace {
    /// The drive, as Explorer names it (`D:`), or the folder's root without drive letters (`/`).
    pub drive: String,
    /// What is left for the app to fill, in bytes.
    pub free: f64,
    /// How much the drive holds, in bytes.
    pub total: f64,
}

/// The app's cameras. Every call names a camera by its `id`.
#[module]
pub struct Cameras;

#[module]
impl Cameras {
    /// Every camera, in the order they are shown.
    pub fn list() -> Vec<CameraSummary> {
        backend::list()
    }

    /// What every camera is doing right now, in the order they are shown.
    pub fn states() -> Vec<CameraState> {
        backend::states()
    }

    /// Calls `listener` with every camera's state now and after every change.
    pub fn on_states(#[args(states)] listener: Callback<Vec<CameraState>>, cx: CallCx<'_>) -> Stop {
        listen(cx, listener, REFRESH_EVERY, backend::states)
    }

    /// Whether the camera may connect by itself: it has a password, or this run has no camera.
    pub fn ready(id: String) -> Result<bool, String> {
        backend::ready(&id)
    }

    /// Starts the camera's streams on screen, those that gave up included, and undoes a Disconnect.
    pub async fn connect(id: String) -> Result<(), String> {
        backend::connect(id).await
    }

    /// Stops the camera's stream, which stays stopped until Connect, on screen or not.
    pub async fn disconnect(id: String) -> Result<(), String> {
        backend::disconnect(id).await
    }

    /// Says which streams are shown, kept and selected, and resolves once the streams match.
    pub async fn set_on_screen(
        shown: Vec<ShownStream>,
        kept: Vec<ShownStream>,
        selected: ShownStream,
    ) -> Result<(), String> {
        backend::set_on_screen(shown, kept, selected).await
    }

    /// The form for a new camera: the usual ports, with no name, address or password yet.
    pub fn blank_form() -> CameraForm {
        backend::blank_form()
    }

    /// Every brand the form offers, in order.
    pub fn brands() -> Vec<BrandInfo> {
        form::brands()
    }

    /// Everything the app can tell apart in a picture, in the order it lists them.
    pub fn detections() -> Vec<DetectionInfo> {
        form::detections()
    }

    /// `form` with brand `brand`: ports still at the old brand's defaults take the new one's.
    pub fn with_brand(form: CameraForm, brand: String) -> CameraForm {
        form::with_brand(&form, &brand)
    }

    /// Asks the camera at the form's address what it is and fills in the form; `id` is `""` if new.
    pub async fn detect(id: String, form: CameraForm) -> Result<DetectedCamera, String> {
        backend::detect(id, form).await
    }

    /// Searches the network for cameras without logging in to any; takes about three seconds.
    pub async fn discover() -> Result<Vec<FoundCamera>, String> {
        backend::discover().await
    }

    /// Switches the camera's spotlight; rejects a camera without one or one not connected.
    pub async fn set_spotlight(id: String, on: bool) -> Result<(), String> {
        backend::set_spotlight(id, on).await
    }

    /// Saves a new camera from the form; resolves with its id. It is in no group and no view yet.
    pub async fn add(form: CameraForm) -> Result<String, String> {
        backend::add_camera(form).await
    }

    /// The page that shows the camera's live picture on `stream`; `""` where the app draws it.
    pub fn live_url(id: String, stream: String) -> String {
        backend::live_url(&id, &stream)
    }

    /// Every stream the camera has, once each, as the menus list them; empty for an unknown camera.
    pub fn stream_choices(id: String) -> Vec<StreamChoice> {
        backend::with_library(|saved| library::stream_choices(saved, &id))
    }

    /// The key of the camera's `streamChoices` row that is the same stream as `key`, else `key`.
    pub fn choice_key(id: String, key: String) -> String {
        backend::with_library(|saved| library::choice_key(saved, &id, &key))
    }

    /// Whether Auto has a choice for the camera: streams of more than one size on its own lens.
    pub fn auto_picks(id: String) -> bool {
        backend::with_library(|saved| library::auto_picks(saved, &id))
    }

    /// The `streamChoices` key Auto plays for the camera's `pictures`, one for all of them.
    pub fn auto_stream(id: String, pictures: Vec<DrawnPicture>) -> String {
        backend::with_library(|saved| library::auto_stream(saved, &id, &pictures))
    }

    /// Deletes the camera from the list, every group and every view, and stops its stream.
    pub async fn delete(id: String) -> Result<(), String> {
        backend::delete_camera(id).await
    }

    /// Moves the camera to `index` (from 0) among the cameras; past the end moves it to the end.
    pub async fn move_to(id: String, index: u32) -> Result<(), String> {
        backend::move_camera(id, index).await
    }

    /// The ids of the groups the camera is in.
    pub fn groups_of(id: String) -> Vec<String> {
        backend::with_library(|saved| library::groups_of(saved, &id))
    }

    /// Saves the picture on screen of each connected camera in `ids`; resolves with how many.
    pub async fn snapshot(ids: Vec<String>) -> Result<u32, String> {
        backend::snapshot(ids).await
    }

    /// Starts recording each connected camera in `ids`; resolves with how many.
    pub async fn record(ids: Vec<String>) -> Result<u32, String> {
        backend::record(ids).await
    }

    /// Stops every recording; resolves with how many there were.
    pub async fn stop_recording() -> Result<u32, String> {
        backend::stop_recording().await
    }

    /// Sets the camera's audio. `volume` is 0 to 100.
    pub fn set_audio(id: String, muted: bool, volume: f64) -> Result<(), String> {
        backend::set_audio(&id, muted, volume)
    }

    /// Starts or changes a PTZ move: each axis -1 to 1 (right, up, in), `speed` 0 to 1.
    pub async fn ptz_move(
        id: String,
        pan: f64,
        tilt: f64,
        zoom: f64,
        speed: f64,
    ) -> Result<(), String> {
        backend::ptz_move(id, pan, tilt, zoom, speed).await
    }

    /// Stops a PTZ move.
    pub async fn ptz_stop(id: String) -> Result<(), String> {
        backend::ptz_stop(id).await
    }

    /// The camera's settings, as the form shows them.
    pub fn form(id: String) -> Result<CameraForm, String> {
        backend::form(&id)
    }

    /// What stops the form being saved: nothing when it is ready.
    pub fn form_problems(form: CameraForm) -> Vec<FormProblem> {
        backend::form_problems(&form)
    }

    /// Saves the form; a running stream reconnects with it.
    pub async fn save_form(id: String, form: CameraForm) -> Result<(), String> {
        backend::save_form(id, form).await
    }

    /// What stops `changes` being given to several cameras; only the fields they name are checked.
    pub fn change_problems(changes: CameraChanges) -> Vec<FormProblem> {
        backend::change_problems(&changes)
    }

    /// Gives every camera in `ids` the changes in one save; resolves with how many changed.
    pub async fn save_changes(ids: Vec<String>, changes: CameraChanges) -> Result<u32, String> {
        backend::save_changes(ids, changes).await
    }
}

/// The snapshots saved in `data\snapshots` beside the executable.
#[module]
pub struct Snapshots;

#[module]
impl Snapshots {
    /// Every snapshot, newest first.
    pub async fn list() -> Result<Vec<SnapshotInfo>, String> {
        backend::snapshots().await
    }

    /// Calls `listener` whenever a snapshot is added, deleted or rewritten.
    pub fn on_change(#[args(revision)] listener: Callback<f64>, cx: CallCx<'_>) -> Stop {
        listen_later(cx, listener, FOLDER_CHECK_EVERY, || async {
            backend::snapshots_revision()
                .await
                .map(|count| count as f64)
        })
    }

    /// Deletes the snapshot for good: it does not go to the Recycle Bin.
    pub async fn delete(name: String) -> Result<(), String> {
        backend::delete_snapshot(name).await
    }

    /// Shows the snapshot in its folder, or in a new tab on the web.
    pub async fn reveal(name: String) -> Result<(), String> {
        backend::reveal_snapshot(&name).await
    }

    /// Opens the snapshots folder in the system's file manager, where `Storage.abilities` allows.
    pub async fn open_folder() -> Result<(), String> {
        backend::open_snapshots_folder().await
    }
}

/// The recordings saved in `data\videos` beside the executable.
#[module]
pub struct Videos;

#[module]
impl Videos {
    /// Every recording, newest first.
    pub async fn list() -> Result<Vec<VideoInfo>, String> {
        backend::videos().await
    }

    /// Calls `listener` whenever a recording is added, deleted or written to.
    pub fn on_change(#[args(revision)] listener: Callback<f64>, cx: CallCx<'_>) -> Stop {
        listen_later(cx, listener, FOLDER_CHECK_EVERY, || async {
            backend::videos_revision().await.map(|count| count as f64)
        })
    }

    /// Deletes the recording for good; one still recording is refused.
    pub async fn delete(name: String) -> Result<(), String> {
        backend::delete_video(name).await
    }

    /// Shows the recording in its folder, or in a new tab on the web.
    pub async fn reveal(name: String) -> Result<(), String> {
        backend::reveal_video(&name).await
    }

    /// Opens the videos folder in the system's file manager, where `Storage.abilities` allows.
    pub async fn open_folder() -> Result<(), String> {
        backend::open_videos_folder().await
    }
}

/// What the cameras saw: the event log in `data\events`, which the desktop app keeps.
#[module]
pub struct Events;

#[module]
impl Events {
    /// Whether this app keeps an event log: the desktop app does; the web build doesn't yet.
    pub fn kept() -> bool {
        backend::events_kept()
    }

    /// Every event, newest first.
    pub async fn list() -> Result<Vec<EventInfo>, String> {
        backend::events().await
    }

    /// Calls `listener` whenever an event starts, changes, saves a file, ends or is deleted.
    pub fn on_change(#[args(revision)] listener: Callback<f64>, cx: CallCx<'_>) -> Stop {
        listen(cx, listener, REFRESH_EVERY, || {
            backend::events_revision() as f64
        })
    }

    /// Deletes the events in `ids` that have ended, and what each saved; resolves with how many.
    pub async fn delete(ids: Vec<String>) -> Result<u32, String> {
        backend::delete_events(ids).await
    }

    /// Deletes every event that has ended, and what each saved; resolves with how many.
    pub async fn clear() -> Result<u32, String> {
        backend::clear_events().await
    }
}

/// What the app does by itself: saving what the cameras see, and keeping media within limits.
#[module]
pub struct Automation;

#[module]
impl Automation {
    /// Whether this app does anything by itself: the desktop app does; the web build doesn't yet.
    pub fn available() -> bool {
        backend::automation_available()
    }

    /// The saved settings.
    pub fn settings() -> AutomationSettings {
        backend::automation()
    }

    /// Saves the settings, which apply at once.
    pub async fn save(settings: AutomationSettings) -> Result<(), String> {
        backend::save_automation(settings).await
    }

    /// What the snapshots and videos hold together now, in bytes.
    pub async fn media_size() -> Result<f64, String> {
        backend::media_size().await
    }
}

/// Where snapshots and recordings are saved; `kind` is `"snapshots"` or `"videos"`.
#[module]
pub struct Storage;

#[module]
impl Storage {
    /// Calls `listener` with what the snapshots and videos take, now and whenever it changes.
    pub fn on_usage(#[args(usage)] listener: Callback<MediaUsage>, cx: CallCx<'_>) -> Stop {
        listen_later(cx, listener, USAGE_CHECK_EVERY, || async {
            backend::media_usage().await.ok()
        })
    }

    /// What can be done with the folders where the app runs: on the web they are the server's.
    pub fn abilities() -> StorageAbilities {
        backend::storage_abilities()
    }

    /// Where `kind` are saved now.
    pub fn folder(kind: String) -> Result<StorageFolder, String> {
        backend::storage_folder(&kind)
    }

    /// Picks a folder for `kind` with the system's dialog; videos stay put while a camera records.
    pub async fn choose(kind: String) -> Result<StorageFolder, String> {
        backend::choose_storage(kind).await
    }

    /// Saves `kind` in their default folder beside the app again.
    pub fn reset(kind: String) -> Result<StorageFolder, String> {
        backend::reset_storage(&kind)
    }

    /// Opens where `kind` are saved in the system's file manager.
    pub async fn open(kind: String) -> Result<(), String> {
        backend::open_storage(&kind).await
    }

    /// How full the drive under `kind`'s folder is.
    pub async fn space(kind: String) -> Result<DriveSpace, String> {
        backend::storage_space(kind).await
    }
}

/// The window preferences.
#[module]
pub struct Preferences;

#[module]
impl Preferences {
    /// The saved preferences.
    pub fn load() -> UiPreferences {
        backend::preferences()
    }

    /// Saves the preferences; a failure is silent, so it never interrupts the user.
    pub async fn save(preferences: UiPreferences) {
        backend::save_preferences(preferences).await;
    }

    /// The page the window showed last (`dashboard`, `view:<id>`); empty when none was saved.
    pub fn last_page() -> String {
        backend::last_page()
    }

    /// Whether the app shows a message when a camera starts seeing something.
    pub fn alerts() -> bool {
        backend::alerts()
    }

    /// Saves whether the app shows those messages; a failure is silent.
    pub async fn save_alerts(on: bool) {
        backend::save_alerts(on).await;
    }

    /// Saves the page the window shows; of several saves under way the newest wins.
    pub async fn save_page(page: String) {
        backend::save_page(page).await;
    }

    /// How wide the Events page's table was last dragged, in points; `null` until it is.
    pub fn events_table_width() -> Option<f64> {
        backend::events_table_width()
    }

    /// Saves how wide the Events page's table was dragged; a failure is silent.
    pub async fn save_events_table_width(width: f64) {
        backend::save_events_table_width(width).await;
    }
}

/// Where the window was left, kept in `settings.json` so it reopens there.
#[module]
pub struct AppWindow;

#[module]
impl AppWindow {
    /// Where the window was left; a width of 0 when nothing was saved yet.
    pub fn place() -> WindowPlace {
        backend::saved_window()
    }

    /// Saves where the window settled; a failure is silent.
    pub async fn save_place(place: WindowPlace) {
        backend::save_window(place).await;
    }
}

/// The camera groups. A group lists cameras by id, so one camera can be in several.
#[module]
pub struct Groups;

#[module]
impl Groups {
    /// Every group, in the order they are shown; each lists only cameras that exist.
    pub fn list() -> Vec<GroupInfo> {
        backend::with_library(library::groups)
    }

    /// Adds a group with `cameras` in it, in order; resolves with its id. Rejects a blank name.
    pub async fn add(name: String, cameras: Vec<String>) -> Result<String, String> {
        make(Ok(library::add_group(&name, &cameras))).await
    }

    /// Renames the group. Rejects a blank name.
    pub async fn rename(id: String, name: String) -> Result<(), String> {
        make(Ok(library::rename_group(&id, &name))).await.map(drop)
    }

    /// Gives the group `name` and `cameras`, in that order, in one save.
    pub async fn save(id: String, name: String, cameras: Vec<String>) -> Result<(), String> {
        make(Ok(library::save_group(&id, &name, &cameras)))
            .await
            .map(drop)
    }

    /// Removes the group. Its cameras stay, in the list and in every other group.
    pub async fn remove(id: String) -> Result<(), String> {
        make(Ok(library::remove_groups(std::slice::from_ref(&id))))
            .await
            .map(drop)
    }

    /// Removes every group in `ids` in one save; their cameras stay.
    pub async fn remove_many(ids: Vec<String>) -> Result<(), String> {
        make(Ok(library::remove_groups(&ids))).await.map(drop)
    }

    /// Moves the group to `index` (from 0) among the groups; past the end moves it to the end.
    pub async fn move_to(id: String, index: u32) -> Result<(), String> {
        make(Ok(library::move_group(&id, index))).await.map(drop)
    }

    /// Puts the camera at the end of the group; nothing happens when it is already there.
    pub async fn add_camera(group: String, camera: String) -> Result<(), String> {
        make(Ok(library::add_to_group(&group, &camera)))
            .await
            .map(drop)
    }

    /// Takes the camera out of this group only.
    pub async fn remove_camera(group: String, camera: String) -> Result<(), String> {
        make(Ok(library::remove_from_group(&group, &camera)))
            .await
            .map(drop)
    }
}

/// The saved views: layouts of tiles, each showing a camera by id.
#[module]
pub struct Views;

#[module]
impl Views {
    /// Every view, in the order they are shown.
    pub fn list() -> Vec<ViewInfo> {
        backend::with_library(library::views)
    }

    /// Every layout a view can start as or be set to, in the order the picker shows them.
    pub fn layouts() -> Vec<LayoutInfo> {
        library::layouts()
    }

    /// The grid a view's tiles sit on: how many columns and rows, and the smallest a tile is.
    pub fn grid() -> GridInfo {
        library::grid()
    }

    /// Every way a picture can fill its tile, in the order the menu shows them.
    pub fn fits() -> Vec<FitInfo> {
        library::fits()
    }

    /// Adds a view with empty tiles; resolves with its id. Rejects a blank name.
    pub async fn add(name: String, layout: String) -> Result<String, String> {
        make(library::add_view(&name, &layout)).await
    }

    /// Renames the view. Rejects a blank name.
    pub async fn rename(id: String, name: String) -> Result<(), String> {
        make(Ok(library::rename_view(&id, &name))).await.map(drop)
    }

    /// Removes the view. Its cameras are not touched.
    pub async fn remove(id: String) -> Result<(), String> {
        make(Ok(library::remove_view(&id))).await.map(drop)
    }

    /// Divides the view as the layout; tiles it has no room for keep their cameras for later.
    pub async fn set_layout(id: String, layout: String) -> Result<(), String> {
        make(library::set_layout(&id, &layout)).await.map(drop)
    }

    /// Moves, resizes, adds and removes the view's tiles at once: `tiles` is every tile it keeps.
    pub async fn arrange(id: String, tiles: Vec<ArrangedTileInfo>) -> Result<(), String> {
        make(Ok(library::arrange(&id, tiles))).await.map(drop)
    }

    /// Sets how the picture fills tile `index` (from 0): the key of one of `Views.fits`.
    pub async fn set_tile_fit(id: String, index: u32, fit: String) -> Result<(), String> {
        make(library::set_tile_fit(&id, index, &fit))
            .await
            .map(drop)
    }

    /// Shows the camera in tile `index` on `stream` (a `ViewInfo.streams` key); `""` empties it.
    pub async fn set_tile(
        id: String,
        index: u32,
        camera: String,
        stream: String,
    ) -> Result<(), String> {
        make(Ok(library::set_tile(&id, index, &camera, &stream)))
            .await
            .map(drop)
    }
}

/// The saved cameras, groups and views as a whole.
#[module]
pub struct Library;

#[module]
impl Library {
    /// Calls `listener` after every saved change to the cameras, groups or views.
    pub fn on_change(#[args(revision)] listener: Callback<f64>, cx: CallCx<'_>) -> Stop {
        listen(cx, listener, REFRESH_EVERY, || backend::revision() as f64)
    }

    /// What went wrong opening the files, such as one moved aside because it could not be read.
    pub fn problems() -> Vec<String> {
        backend::library_problems()
    }
}

/// The server the web build works on, when it has a password; the desktop never asks to sign in.
#[module]
pub struct Server;

#[module]
impl Server {
    /// Calls `listener` with whether the server wants its password, now and whenever that changes.
    pub fn on_sign_in_needed(#[args(needed)] listener: Callback<bool>, cx: CallCx<'_>) -> Stop {
        listen(cx, listener, REFRESH_EVERY, backend::sign_in_needed)
    }

    /// Whether someone has signed in from this page, so there is a session to end.
    pub fn signed_in() -> bool {
        backend::signed_in()
    }

    /// Signs in with the server's password; a wrong one is refused, and the next try must wait.
    pub async fn sign_in(password: String) -> Result<(), String> {
        backend::sign_in(password).await
    }

    /// Ends this page's session: the server asks for its password again.
    pub async fn sign_out() -> Result<(), String> {
        backend::sign_out().await
    }
}

/// Makes a change where the cameras are kept, resolving with the id of what it added or `""`.
async fn make(change: Result<Change, String>) -> Result<String, String> {
    backend::apply(change?)
        .await
        .map(|applied| applied.added_id())
}
