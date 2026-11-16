//! What TSX sees of the app: the types and functions it imports from `ipcamera/native`.
//!
//! This file is the same on every platform. Behind it, `camera` does the work on Windows and
//! `web_backend` stands in everywhere else, so `App.tsx` never needs to know which one it got.

use std::time::Duration;

use dp_ui::{SavedAppearance, WindowPlace, listen, listen_later};
use ipcamera_core::Change;
use nsh_core::prelude::*;

#[cfg(windows)]
use crate::camera as backend;
#[cfg(not(windows))]
use crate::web_backend as backend;
use crate::{form, library};

/// How often a listener checks for a change. Quick enough that a status reads as immediate, and
/// the stats only change about once a second anyway.
const REFRESH_EVERY: Duration = Duration::from_millis(250);

/// How often what the snapshots and videos take is added up for the status bar: their folder
/// listings, off the main thread. A recording grows by a few megabytes a second, which a number
/// in gigabytes shows no faster.
const USAGE_CHECK_EVERY: Duration = Duration::from_secs(3);

/// How often the snapshots and videos folders are checked. Files only change there when the user
/// does something or a camera records, so a second is soon enough, and the check reads only the
/// folder listing, off the main thread: a folder on a slow drive or a network share can keep a
/// listing waiting.
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
    /// The most the snapshots and videos may hold together (Settings → Automation), in bytes; 0
    /// for no limit.
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
    /// What went wrong, as a sentence, while `trouble` isn't `fine`: `Reconnecting (attempt 3)…`,
    /// or why it stopped. `""` while it is fine.
    pub problem: String,
    /// When its last picture arrived, in milliseconds since 1970: the one its tiles still show
    /// while it is in trouble. 0 before its first.
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
    /// The camera refused its login, which nothing tries again by itself until the settings
    /// change.
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
    /// Whether a stream is running, or trying to. It decides Connect or Disconnect. A stream that
    /// gave up still counts until Disconnect, and its tiles say so (`RunningStream.trouble`).
    pub connected: bool,
    /// Whether it may connect by itself: without a password it waits to be asked.
    pub ready: bool,
    /// What the stream is doing, as a sentence.
    pub status: String,
    /// How `status` should read.
    pub tone: StatusTone,
    /// Size, frame rate and transport of the stream that stands for the camera; empty with no
    /// stream.
    pub stats: String,
    /// Where the camera is on the network: its host, as its tiles show it (`192.168.1.64`).
    pub address: String,
    /// Each of its streams that runs, with its numbers, for the tiles that show it; empty where
    /// the app doesn't draw the picture itself (the web).
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
    /// What its AI sees now, by `DetectionInfo` key (`person`, `vehicle`); empty when nothing,
    /// or for a camera the app doesn't watch.
    pub detected: Vec<String>,
    /// Whether it has a spotlight the app can switch (`Cameras.setSpotlight`).
    pub spotlight: bool,
    /// Whether that spotlight is on now.
    pub spotlight_on: bool,
    /// What it says it is and has, or why watching it failed, as a sentence; `""` for a camera the
    /// app doesn't watch.
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
    /// The RTSP path: one of `streams`' where the camera lists any, else what was typed, or
    /// blank for the path the brand, channel and `subStream` make.
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
    /// The port of the camera's own HTTP API: a Reolink's, which its PTZ, motion and spotlight go
    /// over, or a Dahua's, which its PTZ and light go over with `httpApi` on.
    pub api_port: String,
    /// Whether that API is HTTPS.
    pub api_https: bool,
    /// For a Dahua: whether its PTZ and light go over its HTTP API rather than PTZ over DVRIP, as
    /// a camera without DVRIP (Amcrest's light bulb cameras) needs.
    pub http_api: bool,
    /// Whether the app watches the camera's ONVIF events for motion, for a camera that isn't a
    /// Reolink.
    pub onvif_events: bool,
    /// A note under the password field: on the web, which is never told a saved password, that
    /// leaving it empty keeps the saved one. `""` when there is nothing to say.
    pub password_note: String,
    /// The streams the camera listed when Detect last asked it with a login, which the form
    /// offers to choose from; empty until then, and for a camera that lists none. Choosing one
    /// puts its `path` in `rtspPath`.
    pub streams: Vec<StreamInfo>,
    /// Whether the camera's ONVIF refused this login when Detect asked, where the login works
    /// another way: Detect then leaves ONVIF alone. The form clears it when the address, the
    /// login or the ONVIF port or path is typed again.
    pub onvif_refused: bool,
    /// Whether the app logs what the camera sees as events, while Settings → Automation logs
    /// them; on for a new camera. Off, nothing is logged, alerted or saved for it, and it is asked
    /// what it sees only while it streams, for its badge.
    pub log_events: bool,
    /// Whether the camera's event videos are made its own way, `eventVideo`, rather than as
    /// Settings → Automation says.
    pub own_event_video: bool,
    /// How the camera's event videos are made, with `ownEventVideo`; Settings → Automation's
    /// without, to start its own from.
    pub event_video: EventVideoSettings,
}

/// How the videos an event saves are made: Settings → Automation's for every camera, or a
/// camera's own in its place. Out-of-range numbers are brought within the ranges as they are
/// saved.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct EventVideoSettings {
    /// How long a video keeps recording after the camera last saw something, in seconds, 10 to
    /// 120: what it sees again sooner continues the same video. 30 until changed.
    pub after_seconds: u32,
    /// How many seconds from before the camera saw something each video starts with, 5 to 30, or
    /// 0 for none; more than 0 keeps the camera connected while events are recorded. 10 until
    /// changed.
    pub before_seconds: u32,
    /// The longest a video runs before it goes on in a new file, in minutes, up to 60; 0 for no
    /// limit. 10 until changed.
    pub longest_minutes: u32,
    /// Whether plain motion keeps a video going between the AI's detections, for a camera whose
    /// AI tells things apart, for up to two minutes after the last. On until changed.
    pub motion_continues: bool,
    /// Which stream the videos record: `own` (the one the camera's settings name), `main`, `sub`,
    /// or a path the camera listed.
    pub stream: String,
    /// What the camera has to see for an event to save a video, by `DetectionInfo` key: all of
    /// them until changed. A camera that knows only motion saves one for motion whatever this
    /// holds.
    pub kinds: Vec<String>,
}

/// One of the streams a camera says it has, in the camera's own words.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct StreamInfo {
    /// What the camera calls it.
    pub name: String,
    /// Its path, which `CameraForm.rtspPath` takes: an RTSP path, or `flv:` and a Reolink's name
    /// for one of its FLV streams.
    pub path: String,
    /// What the camera says of its picture (`3840×2160, H265`); `""` when it says nothing.
    pub detail: String,
    /// What a list shows for it, as `StreamChoice.title` is: its size, codec and name.
    pub title: String,
    /// The lens it is of, for a camera with more than one (`Tracking lens`); `""` for the main
    /// lens.
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
    /// The form it was asked with, with the brand and the API's port filled in as found, and
    /// the streams the camera listed.
    pub form: CameraForm,
    /// What was found, as a sentence: `Reolink RLC-811A, firmware v3.1.0.2368.`
    pub summary: String,
    /// What the camera has, in words (`Motion`, `Person`, `Spotlight`, `Pan and tilt`); empty
    /// when it wasn't asked, without a password, or has nothing beyond its stream.
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
    /// The port its ONVIF service is on, as text, as the form keeps its numbers; `""` for a
    /// device found by its video alone.
    pub onvif_port: String,
    /// The path of its ONVIF service; `""` for a device found by its video alone.
    pub onvif_path: String,
    /// The port it answers RTSP on, as text; `""` where the search didn't try it or it didn't
    /// answer.
    pub rtsp_port: String,
    /// Whatever else the camera says of itself, each as `Kind: what it says`
    /// (`MAC: aa:bb:cc:00:11:22`); empty when it says nothing more.
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

/// Settings to give several cameras at once, as the Cameras page edits them. A field left out
/// (`null`) keeps each camera's own. The name and the channel are not here: they are each
/// camera's own. Numbers stay text, as in `CameraForm`, and a blank one saves the default.
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
    /// The RTSP path, or blank for the Dahua default.
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
    /// The key of the `LayoutInfo` its tiles are, in that layout's order; `""` for tiles moved or
    /// resized to an arrangement of their own.
    pub layout: String,
    /// Where each tile sits on the view's grid (`Views.grid`), in the order of `tiles`.
    pub cells: Vec<CellInfo>,
    /// The key of the `FitInfo` a tile shows its picture with until it picks one of its own: what
    /// a new tile takes.
    pub fit: String,
    /// The camera id in each tile, in the order of `cells`; `""` for an empty tile.
    pub tiles: Vec<String>,
    /// The stream each tile picks, in the same order as `tiles`: the key of one of
    /// `Cameras.streamChoices`, or `own` for its camera's own; `""` for Auto, which plays the
    /// stream the size the tile is drawn at calls for (`Cameras.autoStream`), and for an empty
    /// tile.
    pub streams: Vec<String>,
    /// The key of the `FitInfo` each tile shows its picture with, which the video component is
    /// given too, in the same order as `tiles`: its own, else the view's `fit`.
    pub fits: Vec<String>,
    /// The camera ids of the tiles past `cells`, which a view keeps for when it has room for them
    /// again, in order: tile `tiles.length + i` holds `hiddenTiles[i]`; `""` for an empty one.
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
    /// The stream, as `StreamChoice.key` names it: `""` (or `own`) for the camera's own. Never
    /// Auto: what Auto plays is one of these.
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
    /// What a picture is given to play it (`Cameras.setOnScreen`, `liveUrl`): `""` for the
    /// camera's own, else `main`, `sub`, or the camera's path for it. A tile picks it by saving
    /// this, except the camera's own, which it saves as `own`, as `""` is Auto there.
    pub key: String,
    /// The camera's own name for it (`Profile000_MainStream`, `ext`); `Main stream` or `Sub
    /// stream` for one at its brand's path that it didn't list; the path for one it named nothing.
    pub name: String,
    /// What a menu shows for it: its picture's size, its codec and its name, those the camera
    /// said (`1920×1080 · H.264 · Profile003_AutotrackStream`).
    pub title: String,
    /// Its picture's size, `1920×1080`; `""` when the camera didn't say.
    pub size: String,
    /// The lens it is of, for a camera with more than one (`Tracking lens`); `""` for the main
    /// lens.
    pub lens: String,
    /// Whether it is the camera's own stream, its default: the one it records, and the one a tile
    /// on Auto plays where Auto has no sizes to go by.
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

/// One tile of a view as its layout is edited, for `Views.arrange`: where it goes on the grid,
/// and which tile it was.
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
    /// The tile it was, from 0, as `ViewInfo.tiles` counts them, so it keeps that tile's camera;
    /// `null` for a new, empty one.
    pub was: Option<u32>,
}

/// A way for a picture to fill its tile.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct FitInfo {
    /// What a view and a tile save.
    pub key: String,
    /// What the menu calls it.
    pub title: String,
    /// Whether it forces the picture to a shape of its own (`1:1` and the rest), which the menu
    /// lists apart, after the others.
    pub reshapes: bool,
}

/// A saved snapshot, as the gallery shows it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct SnapshotInfo {
    /// The file's name, which every other call names it by.
    pub name: String,
    /// Where the file is, as `<Image src>` takes it: an absolute path on the desktop, the
    /// server's URL for it on the web. The viewer shows it.
    pub path: String,
    /// It made small, for `<Image src>` as `path` is: what the gallery shows, since a full picture
    /// decodes to 8 MB at 1080p. `""` until it has one: a snapshot without one gets it soon after
    /// it is first listed.
    pub thumbnail: String,
    /// The id of the camera that took it, or `""` when its name doesn't say, as for a PNG put in
    /// the folder by hand.
    pub camera: String,
    /// What the camera saw as an event took it, from its name (`vehicle`, `person`, or `motion`
    /// for a camera that knows only motion); empty for one taken by hand.
    pub tags: Vec<String>,
    /// The event that took it, by id, while the log has it; `""` otherwise, and always on the
    /// web, which keeps no log.
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
    /// Where the file is, as `<Video src>` takes it: an absolute path on the desktop, the
    /// server's URL for it on the web.
    pub path: String,
    /// Its first picture, for `<Image src>` as `path` is; `""` while it has none.
    pub thumbnail: String,
    /// The id of the camera that made it, or `""` when its name doesn't say.
    pub camera: String,
    /// What the camera saw as an event's sighting started it, from its name, as a snapshot's
    /// `tags`; empty for one someone started.
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
    /// What the camera's AI told apart during it, by `DetectionInfo` key (`person`, `vehicle`);
    /// empty for a camera that knows only motion.
    pub detected: Vec<String>,
    /// How many times the camera saw something during it, each after ten seconds of nothing; 1
    /// for one pass.
    pub sightings: u32,
    /// Its snapshot, as the snapshots page names it; `""` when it took none, or it was deleted
    /// since.
    pub snapshot: String,
    /// That snapshot, for the viewer's `<Image src>`: an absolute path; `""` without one.
    pub picture: String,
    /// That snapshot made small, for a row's or a tile's `<Image src>`; `""` until it has one.
    pub thumbnail: String,
    /// Its recordings, as the videos page names them, oldest first, without those deleted since.
    pub videos: Vec<String>,
}

/// What each event saves into the snapshots and videos folders, as Settings' Automation section
/// offers it.
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

/// What the app does by itself, and how much media it may keep: Settings' Automation section,
/// saved in `settings.json`'s `automation`. Sizes are in megabytes of 1,048,576 bytes, as the
/// system shows them.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct AutomationSettings {
    /// Whether the app watches every camera it can, on screen or not, and logs what they see on
    /// the Events page. On until turned off; off, there are no alerts and nothing is saved for
    /// events either.
    pub log_events: bool,
    /// What each event saves into the snapshots and videos folders, tagged with what the camera
    /// saw. A picture until the user picks.
    pub event_capture: EventCaptureInfo,
    /// How long after a camera last saw something, in minutes, what it sees still joins the same
    /// event; 0 for none, when an event ends ten seconds after the camera last saw it. Five until
    /// the user picks.
    pub event_window_minutes: u32,
    /// The most the snapshots and videos may hold together, in megabytes; 0 for no limit. At it
    /// the app takes no more, and stops what it records.
    pub media_limit_mb: f64,
    /// The largest a video file may grow, in megabytes, before the recording goes on in a new
    /// file; 0 for no limit.
    pub video_limit_mb: f64,
    /// Whether, at the media limit, the oldest snapshots and videos are deleted for good to make
    /// room for new ones, rather than nothing new being taken. One still recording never is.
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

/// What can be done with the app's folders where it runs. The desktop has them on this PC; on the
/// web they are the server's, which a browser can't open or choose.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct StorageAbilities {
    /// What a snapshot's or a recording's menu calls showing the file: `Show in Explorer` on the
    /// desktop, `Open in a new tab` on the web.
    pub reveal_label: String,
    /// Whether a folder can be opened in the system's file manager.
    pub can_open_folder: bool,
    /// Whether another folder can be chosen, or the default chosen again.
    pub can_choose_folder: bool,
}

/// How full the drive under one of the app's folders is, for the dashboard.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct DriveSpace {
    /// The drive, as Explorer names it (`D:`), or the folder's root where there are no drive
    /// letters (`/`).
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

    /// Calls `listener` with every camera's state now and after every change, cameras added and
    /// deleted included, until the returned function is called or what called this goes.
    pub fn on_states(#[args(states)] listener: Callback<Vec<CameraState>>, cx: CallCx<'_>) -> Stop {
        listen(cx, listener, REFRESH_EVERY, backend::states)
    }

    /// Whether the camera may connect by itself: it has a password, or this run has no camera.
    pub fn ready(id: String) -> Result<bool, String> {
        backend::ready(&id)
    }

    /// Starts the camera's streams the screen shows; nothing happens to one already running, and
    /// one that gave up (`StreamTrouble.stopped`, `refused`) starts again, as nothing else tries
    /// it. It also undoes a Disconnect, so the camera connects by itself again when it comes on
    /// screen. On the web it has the server try the camera again, after its stream gave up or
    /// though it has no saved password.
    pub async fn connect(id: String) -> Result<(), String> {
        backend::connect(id).await
    }

    /// Stops the camera's stream, which stays stopped until Connect, on screen or not.
    pub async fn disconnect(id: String) -> Result<(), String> {
        backend::disconnect(id).await
    }

    /// Says which streams the screen shows, which it keeps, and which one is selected: each a
    /// camera's own, or another a tile picks, which is a connection of its own. Streams shown or
    /// kept connect by themselves when their camera has a password (unless the user disconnected
    /// it), the rest disconnect, and only `selected` is heard. A kept stream is one the window
    /// comes back to: the last view's, and the cameras opened on their own since. One no tile
    /// shows stays connected and its status current, but nothing is decoded or drawn until it is
    /// shown again. Resolves once they have.
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

    /// `form` with brand `brand`: the ports still at the old brand's defaults take the new one's,
    /// and what the user typed stays.
    pub fn with_brand(form: CameraForm, brand: String) -> CameraForm {
        form::with_brand(&form, &brand)
    }

    /// Asks the camera at the form's address which brand it is, without a login, then, for a
    /// Reolink with a password, logs in once for its model and what it has. `id` is the camera the
    /// form edits, `""` for a new one: on the web, whose form never holds a saved password, the
    /// server logs in with the saved one. Resolves with the form filled in from what was found;
    /// rejects, saying why, when nothing answered or the login was refused. Takes a few seconds.
    /// A run that reaches no camera (the test picture, a test, the web preview) resolves with
    /// the form as it is and a summary saying so, since the form adds only a camera that answered.
    pub async fn detect(id: String, form: CameraForm) -> Result<DetectedCamera, String> {
        backend::detect(id, form).await
    }

    /// Searches the network for cameras, logging in to none: those ONVIF's discovery finds, then
    /// every other device that answers in RTSP. Resolves with those that answered, by address: the desktop's own network, or on the web the server's. Rejects,
    /// saying why, where nothing can be asked. Takes about three seconds.
    pub async fn discover() -> Result<Vec<FoundCamera>, String> {
        backend::discover().await
    }

    /// Switches the camera's spotlight on or off. Rejects, saying why, for a camera without one or
    /// one that isn't connected.
    pub async fn set_spotlight(id: String, on: bool) -> Result<(), String> {
        backend::set_spotlight(id, on).await
    }

    /// Saves a new camera from the form; resolves with its id. It is in no group and no view yet.
    pub async fn add(form: CameraForm) -> Result<String, String> {
        backend::add_camera(form).await
    }

    /// The web page that shows the camera's live picture, on `stream` (a key of
    /// `streamChoices`, or `""` for its own), where the app doesn't draw it itself: the server's
    /// player on the web. `""` on the desktop, which draws it.
    pub fn live_url(id: String, stream: String) -> String {
        backend::live_url(&id, &stream)
    }

    /// Every stream the camera has, as the menus that pick one list them: its own (its default),
    /// the main and the sub stream for a camera of a known brand, and those the camera listed when
    /// Detect asked it, each once however many ways it is reached. The main lens's first, then each
    /// other lens's, each largest picture first. Empty for an unknown camera.
    pub fn stream_choices(id: String) -> Vec<StreamChoice> {
        backend::with_library(|saved| library::stream_choices(saved, &id))
    }

    /// The key of the camera's `streamChoices` row that is the same stream as `key` (a tile's
    /// pick): `""` for its own, whatever `key` calls it, `own` among them. `key` itself for a
    /// stream none of them is.
    pub fn choice_key(id: String, key: String) -> String {
        backend::with_library(|saved| library::choice_key(saved, &id, &key))
    }

    /// Whether Auto has a choice to make for the camera: streams of more than one size on the
    /// lens of its own, as it listed them. Where it hasn't, a tile on Auto plays the camera's own,
    /// whatever its size.
    pub fn auto_picks(id: String) -> bool {
        backend::with_library(|saved| library::auto_picks(saved, &id))
    }

    /// The stream Auto plays for the camera's `pictures` on Auto, each as the stage draws it: the
    /// key of a `streamChoices` row (`""` for its own), one for all of them, so the camera has one
    /// connection whichever tiles show it. The smallest stream on the lens of the camera's own
    /// that none of them shows more than a quarter larger than it is, else the largest; the
    /// camera's own where Auto has no choice (`autoPicks`) or there are no pictures.
    pub fn auto_stream(id: String, pictures: Vec<DrawnPicture>) -> String {
        backend::with_library(|saved| library::auto_stream(saved, &id, &pictures))
    }

    /// Deletes the camera from the list, every group and every view, and stops its stream.
    pub async fn delete(id: String) -> Result<(), String> {
        backend::delete_camera(id).await
    }

    /// Moves the camera to `index` (from 0) among the cameras, which is the order `list`, `states`
    /// and the sidebar's All cameras show them in; an index past the end moves it to the end.
    /// Groups keep their own order, and its stream carries on. Rejects an unknown camera, or a file
    /// that cannot be written, moving nothing.
    pub async fn move_to(id: String, index: u32) -> Result<(), String> {
        backend::move_camera(id, index).await
    }

    /// The ids of the groups the camera is in.
    pub fn groups_of(id: String) -> Vec<String> {
        backend::with_library(|saved| library::groups_of(saved, &id))
    }

    /// Saves the picture on screen of each of these cameras that is connected, into
    /// `data\snapshots`; resolves with how many. Rejects, saying why, when none can.
    pub async fn snapshot(ids: Vec<String>) -> Result<u32, String> {
        backend::snapshot(ids).await
    }

    /// Starts recording each of these cameras that is connected, into `data\videos`; resolves
    /// with how many. Rejects, saying why, when none can. A recording camera stays connected off
    /// screen until recording stops.
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

    /// Starts or changes a PTZ move: each axis is -1 to 1, `speed` 0 to 1. Positive pan is right,
    /// tilt is up, zoom is in. On the desktop it resolves once the move is queued. On the web it
    /// sends the move to the server again and again, which stops a move that isn't, and resolves
    /// once the move is let go (`ptzStop`, or another move).
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

    /// Saves the form; a running stream reconnects with it. Rejects when the file cannot be
    /// written, which changes nothing.
    pub async fn save_form(id: String, form: CameraForm) -> Result<(), String> {
        backend::save_form(id, form).await
    }

    /// What stops `changes` being given to several cameras: nothing when they can be. Only the
    /// fields the changes name are checked, as `formProblems` names them.
    pub fn change_problems(changes: CameraChanges) -> Vec<FormProblem> {
        backend::change_problems(&changes)
    }

    /// Gives every camera in `ids` the changes, in one save; resolves with how many changed.
    /// Running streams of those that changed reconnect with them; a camera the changes leave as
    /// it was is neither saved nor restarted. Rejects, changing nothing, when a change has a
    /// problem, a camera is unknown or the file cannot be written.
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

    /// Calls `listener` whenever a snapshot is added, deleted or rewritten, by this app or
    /// anything else, until the returned function is called or what called this goes.
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

    /// Shows the snapshot: its folder in the system's file manager with it selected, or on the
    /// web the picture in a new tab. `Storage.abilities` says which. Async because opening a tab
    /// is nsh's to do from a task, not from a plain call.
    pub async fn reveal(name: String) -> Result<(), String> {
        backend::reveal_snapshot(&name)
    }

    /// Opens the snapshots folder in the system's file manager, where `Storage.abilities` says it
    /// can.
    pub fn open_folder() -> Result<(), String> {
        backend::open_snapshots_folder()
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

    /// Calls `listener` whenever a recording is added, deleted or written to, by this app or
    /// anything else, until the returned function is called or what called this goes.
    pub fn on_change(#[args(revision)] listener: Callback<f64>, cx: CallCx<'_>) -> Stop {
        listen_later(cx, listener, FOLDER_CHECK_EVERY, || async {
            backend::videos_revision().await.map(|count| count as f64)
        })
    }

    /// Deletes the recording for good: it does not go to the Recycle Bin. One still recording
    /// is refused.
    pub async fn delete(name: String) -> Result<(), String> {
        backend::delete_video(name).await
    }

    /// Shows the recording: its folder in the system's file manager with it selected, or on the
    /// web the video in a new tab. `Storage.abilities` says which. Async as `Snapshots.reveal` is.
    pub async fn reveal(name: String) -> Result<(), String> {
        backend::reveal_video(&name)
    }

    /// Opens the videos folder in the system's file manager, where `Storage.abilities` says it
    /// can.
    pub fn open_folder() -> Result<(), String> {
        backend::open_videos_folder()
    }
}

/// What the cameras saw: the event log in `data\events`, which the desktop app keeps while it runs,
/// watching every camera it can, on screen or not. An event is what a camera's AI tells apart
/// (a person, a vehicle), or motion for a camera that knows only motion, from when it came into
/// view until the camera has seen nothing for Settings → Automation's window (five minutes until
/// changed): what it sees within it joins the event. What an event saves is in the snapshots and
/// videos folders, and the event names it.
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

    /// Calls `listener` whenever an event starts, adds to what it saw, saves a file, ends or is
    /// deleted, until the returned function is called or what called this goes.
    pub fn on_change(#[args(revision)] listener: Callback<f64>, cx: CallCx<'_>) -> Stop {
        listen(cx, listener, REFRESH_EVERY, || {
            backend::events_revision() as f64
        })
    }

    /// Deletes each of the events `ids` that the camera doesn't see now, in one save of the log,
    /// and for good what each saved, but a recording still being written; resolves with how
    /// many. One the camera sees now stays: it would come back at its next change.
    pub async fn delete(ids: Vec<String>) -> Result<u32, String> {
        backend::delete_events(ids).await
    }

    /// Deletes every event the cameras don't see now, and for good what they saved; resolves
    /// with how many.
    pub async fn clear() -> Result<u32, String> {
        backend::clear_events().await
    }
}

/// What the app does by itself: saving what the cameras see, and keeping the snapshots and
/// videos within limits. The desktop app does it; the web build doesn't yet.
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

    /// Saves the settings, off the main thread; they apply at once. Rejects, changing nothing,
    /// when the file can't be written.
    pub async fn save(settings: AutomationSettings) -> Result<(), String> {
        backend::save_automation(settings).await
    }

    /// What the snapshots and videos hold together now, in bytes, read off the main thread.
    pub async fn media_size() -> Result<f64, String> {
        backend::media_size().await
    }
}

/// Where snapshots and recordings are saved. Each call names the folder as `kind`: `"snapshots"`
/// or `"videos"`.
#[module]
pub struct Storage;

#[module]
impl Storage {
    /// Calls `listener` with what the snapshots and videos take, now and whenever it changes,
    /// adding them up every few seconds off the main thread, until the returned function is
    /// called or what called this goes.
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

    /// Asks for a folder with the system's dialog and saves `kind` there from now on: the next
    /// snapshot or recording goes there, and those already saved stay where they are. Resolves
    /// with where `kind` are saved afterwards, unchanged when the user canceled. Refused for the
    /// videos while a camera records.
    pub async fn choose(kind: String) -> Result<StorageFolder, String> {
        backend::choose_storage(kind).await
    }

    /// Saves `kind` in their default folder beside the app again.
    pub fn reset(kind: String) -> Result<StorageFolder, String> {
        backend::reset_storage(&kind)
    }

    /// Opens where `kind` are saved in the system's file manager.
    pub fn open(kind: String) -> Result<(), String> {
        backend::open_storage(&kind)
    }

    /// How full the drive under `kind`'s folder is: asked of the system, and of the nearest
    /// folder above it that exists while it hasn't been made yet.
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

    /// Saves the preferences, off the main thread. A failure is silent, because a preference must
    /// never interrupt what the user is doing.
    pub async fn save(preferences: UiPreferences) {
        backend::save_preferences(preferences).await;
    }

    /// The page the window showed last, which it opens on: `dashboard`, `cameras`, `groups`,
    /// `snapshots`, `videos`, `events`, `view:<id>` or `camera:<id>`; empty when none was saved.
    pub fn last_page() -> String {
        backend::last_page()
    }

    /// Whether the app shows a message when a camera starts seeing something.
    pub fn alerts() -> bool {
        backend::alerts()
    }

    /// Saves whether the app shows those messages, off the main thread. A failure is silent, as
    /// with `save`.
    pub async fn save_alerts(on: bool) {
        backend::save_alerts(on).await;
    }

    /// Saves the page the window shows, as `last_page` reads it, off the main thread. Of several
    /// saves under way the newest wins, whichever finishes last. A failure is silent, as with
    /// `save`.
    pub async fn save_page(page: String) {
        backend::save_page(page).await;
    }

    /// How wide the Events page's table is beside its preview, in points, as it was last dragged
    /// to; `null` until it is dragged, for its columns' own width.
    pub fn events_table_width() -> Option<f64> {
        backend::events_table_width()
    }

    /// Saves how wide the Events page's table was dragged, as `events_table_width` reads it, off
    /// the main thread. A failure is silent, as with `save`.
    pub async fn save_events_table_width(width: f64) {
        backend::save_events_table_width(width).await;
    }
}

/// Where the window was left, kept in `settings.json` so it reopens there: `keepWindowPlace` (in
/// `dp/ui`) hands it to nsh, and saves each new place the window settles in. The web build keeps
/// none: the browser has its own window.
#[module]
pub struct AppWindow;

#[module]
impl AppWindow {
    /// Where the window was left; a width of 0 when nothing was saved yet.
    pub fn place() -> WindowPlace {
        backend::saved_window()
    }

    /// Saves where the window settled, off the main thread. A failure is silent, as with every
    /// preference.
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

    /// Adds a group with `cameras` in it, in that order, in one save; resolves with its id. A
    /// camera listed twice goes in once. Rejects a blank name or an unknown camera, adding
    /// nothing.
    pub async fn add(name: String, cameras: Vec<String>) -> Result<String, String> {
        make(Ok(library::add_group(&name, &cameras))).await
    }

    /// Renames the group. Rejects a blank name.
    pub async fn rename(id: String, name: String) -> Result<(), String> {
        make(Ok(library::rename_group(&id, &name))).await.map(drop)
    }

    /// Gives the group `name` and `cameras`, in that order, in one save: what its sheet saves. A
    /// camera listed twice goes in once. Rejects a blank name, an unknown group or an unknown
    /// camera, changing nothing.
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

    /// Removes every group in `ids` in one save. Their cameras stay. Rejects an unknown group,
    /// removing none.
    pub async fn remove_many(ids: Vec<String>) -> Result<(), String> {
        make(Ok(library::remove_groups(&ids))).await.map(drop)
    }

    /// Moves the group to `index` (from 0) among the groups, which is the order the sidebar lists
    /// them in. An index past the end moves it to the end.
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

    /// Divides the view as the layout, each tile keeping its camera. Tiles the layout has no room
    /// for keep their cameras, for when the view has room for them again.
    pub async fn set_layout(id: String, layout: String) -> Result<(), String> {
        make(library::set_layout(&id, &layout)).await.map(drop)
    }

    /// Moves, resizes, adds and removes the view's tiles at once: `tiles` is every tile it has
    /// from now on, in order, each keeping the camera of the tile it was. Rejects tiles that
    /// overlap, leave the grid, or are smaller than `Views.grid` allows.
    pub async fn arrange(id: String, tiles: Vec<ArrangedTileInfo>) -> Result<(), String> {
        make(Ok(library::arrange(&id, tiles))).await.map(drop)
    }

    /// Sets how the picture fills tile `index` (from 0), which shows a camera: the key of one of
    /// `Views.fits`. The tile keeps it as its camera or stream changes.
    pub async fn set_tile_fit(id: String, index: u32, fit: String) -> Result<(), String> {
        make(library::set_tile_fit(&id, index, &fit))
            .await
            .map(drop)
    }

    /// Shows the camera in tile `index` (from 0) on `stream`, as `ViewInfo.streams` has it: a key
    /// of `Cameras.streamChoices`, `own` for the camera's own stream, or `""` for Auto; a camera
    /// of `""` empties the tile.
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
    /// Calls `listener` after every saved change to the cameras, groups or views, until the returned
    /// function is called or what called this goes.
    pub fn on_change(#[args(revision)] listener: Callback<f64>, cx: CallCx<'_>) -> Stop {
        listen(cx, listener, REFRESH_EVERY, || backend::revision() as f64)
    }

    /// What went wrong opening the files, such as one moved aside because it could not be read.
    pub fn problems() -> Vec<String> {
        backend::library_problems()
    }
}

/// The server the web build works on, when it has a password. The desktop app keeps its cameras
/// itself and never asks to sign in.
#[module]
pub struct Server;

#[module]
impl Server {
    /// Calls `listener` with whether the server wants its password before it answers, now and
    /// whenever that changes, until the returned function is called or what called this goes.
    pub fn on_sign_in_needed(#[args(needed)] listener: Callback<bool>, cx: CallCx<'_>) -> Stop {
        listen(cx, listener, REFRESH_EVERY, backend::sign_in_needed)
    }

    /// Whether someone has signed in from this page, so there is a session to end.
    pub fn signed_in() -> bool {
        backend::signed_in()
    }

    /// Signs in with the server's password. A wrong one is refused in the server's words, and
    /// the next try has to wait a few seconds.
    pub async fn sign_in(password: String) -> Result<(), String> {
        backend::sign_in(password).await
    }

    /// Ends this page's session: the server asks for its password again.
    pub async fn sign_out() -> Result<(), String> {
        backend::sign_out().await
    }
}

/// Makes a change to the cameras, groups or views where they are kept (the desktop's files, or
/// the server's on the web), and resolves with the id of what it added, `""` when it added
/// nothing. A change that couldn't be described, such as an unknown layout, is refused before it
/// is sent.
async fn make(change: Result<Change, String>) -> Result<String, String> {
    backend::apply(change?)
        .await
        .map(|applied| applied.added_id())
}
