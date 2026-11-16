//! Application settings, persisted as one JSON file beside the executable.
//!
//! [`Config`] holds the app's preferences, grouped by area so a new setting has an obvious home:
//! `ui`, `window`, `storage`, `automation` and `server`. [`ConfigStore`] owns the file and the
//! in-memory copy, and [`ConfigStore::update`] is the one way to change it: it applies the change
//! to a copy, writes the file, and only then adopts the copy. That is the same file store the
//! JSON files in `data\settings` use (`dp_store`), with `settings.json` a document of its own and
//! no layout number.
//!
//! The file is `settings.json` beside the app's executable, so it is the first thing found next
//! to it. Generated files, such as snapshots, live in a `data` folder beside it unless [`Storage`]
//! names other folders, and so do the cameras, groups and views, in `data\settings` (see
//! [`crate::Library`]).
//!
//! [`ConfigStore`] is an ordinary value rather than a process-wide global: the app builds exactly one
//! and holds it, and the tests build one over a temporary folder. Nothing here reads or writes
//! outside the path it was given, which is also why the tests are safe to run in parallel.

use std::path::{Path, PathBuf};

pub use dp_store::WindowState;
use dp_store::{Appearance, Document, FileStore, Json};
use serde::{Deserialize, Serialize};

use crate::StoreError;

/// The folder beside the executable that holds the app's data: snapshots, videos, cameras and
/// groups.
const DATA_DIR: &str = "data";
/// The settings file. It sits beside the executable rather than inside `data`, so it is the
/// obvious file to find and hand-edit.
const FILE: &str = "settings.json";
/// Snapshots go in this folder inside [`ConfigStore::data_dir`].
const SNAPSHOTS_DIR: &str = "snapshots";
/// Recordings go in this folder inside [`ConfigStore::data_dir`].
const VIDEOS_DIR: &str = "videos";
/// The cameras and their groups go in this folder inside [`ConfigStore::data_dir`].
const SETTINGS_DIR: &str = "settings";

/// The whole settings tree.
///
/// Deliberately not `deny_unknown_fields`: a field this version doesn't have must load as if it
/// were not there rather than fail the whole file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `ui`: the app's preferences.
    pub ui: Ui,
    /// `window`: where the window was last seen.
    pub window: WindowState,
    /// `storage`: where snapshots and recordings go.
    pub storage: Storage,
    /// `automation`: what the app does by itself, and how much media it may keep.
    pub automation: Automation,
    /// `server`: who may use `ipcamera-server`. The desktop app keeps what it finds here.
    pub server: ServerAccess,
}

/// Who may use `ipcamera-server`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerAccess {
    /// The server's password as `ipcamera-server --set-password` saves it: a salted Argon2 hash,
    /// from which the password can be checked but not read back. Empty for a server with no
    /// password, which then asks for none. It is not something to write by hand.
    pub password: String,
}

/// Window preferences, restored on the next launch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ui {
    /// Whether the sound is off.
    pub muted: bool,
    /// The volume, 0 to 100.
    pub volume: f32,
    /// Whether the PTZ panel is hidden.
    pub rail_collapsed: bool,
    /// The PTZ panel's width, in points.
    pub rail_width: f32,
    /// The sidebar's width, in points, as it was last dragged to.
    pub sidebar_width: f32,
    /// How the window looks: its mode, themes, texture and elevation, among these fields in the
    /// file as in every product's.
    #[serde(flatten)]
    pub appearance: Appearance,
    /// The page the nsh app last showed, which it opens on at the next launch: `dashboard`,
    /// `cameras`, `groups`, `snapshots`, `videos`, `events`, or `view:<id>` or `camera:<id>` for a
    /// view or a camera by its id. Empty, or a view or camera since deleted, opens on the first
    /// view.
    pub page: String,
    /// Whether the app shows a message when a camera starts seeing something (an event starts).
    pub alerts: bool,
    /// The Events page's table beside its preview, in points, as wide as it was last dragged to;
    /// none until it is dragged, for its columns' own width.
    pub events_table_width: Option<f32>,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            muted: true,
            volume: 80.,
            rail_collapsed: true,
            rail_width: 248.,
            sidebar_width: 232.,
            appearance: Appearance::default(),
            page: String::new(),
            alerts: true,
            events_table_width: None,
        }
    }
}

/// Where the app saves what it makes. An empty folder is the default one in `data` beside the
/// executable, so a fresh install needs no setting and a cleared one goes back to it. A relative
/// folder is taken from beside the executable.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Storage {
    /// Where snapshots are written; empty for `data\snapshots`.
    pub snapshots: String,
    /// Where recordings are written; empty for `data\videos`.
    pub videos: String,
}

/// A megabyte, as the app counts sizes and the system shows them: 1,048,576 bytes.
pub const MEGABYTE: u64 = 1024 * 1024;

/// What each event saves into the snapshots and videos folders, tagged with what the camera saw:
/// Settings' Automation section.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCapture {
    /// Nothing: the event is only a note in the log.
    Nothing,
    /// A snapshot as the event starts.
    #[default]
    Picture,
    /// A video of each sighting, from a little before the camera sees something until it has seen
    /// nothing for a while, as [`EventVideo`](crate::EventVideo) says.
    Video,
    /// Both.
    PictureAndVideo,
}

impl EventCapture {
    /// Whether an event takes a snapshot.
    pub fn picture(self) -> bool {
        matches!(self, Self::Picture | Self::PictureAndVideo)
    }

    /// Whether an event records.
    pub fn video(self) -> bool {
        matches!(self, Self::Video | Self::PictureAndVideo)
    }
}

/// What the app does by itself, and how much media it may keep: Settings' Automation section.
/// Events are logged, each with a picture, grouped within five minutes, and the rest is off and
/// unlimited until the user says otherwise.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Automation {
    /// Whether the app watches every camera it can, on screen or not, and logs what they see as
    /// events. Off, a camera is watched only while it streams, for its badge and its spotlight,
    /// and nothing that follows events happens: no log, no alert, nothing saved.
    pub log_events: bool,
    /// What each event saves.
    pub event_capture: EventCapture,
    /// How long after a camera last saw something, in minutes, what it sees still joins the same
    /// event: a car that stops, waits and leaves is one event, not one each time the camera loses
    /// sight of it. 0 groups nothing: an event then ends ten seconds after the camera last saw it.
    pub event_window_minutes: u32,
    /// The most the snapshots and videos may hold together, in megabytes ([`MEGABYTE`]); 0 for
    /// no limit. At it the app takes no more, unless `overwrite_oldest` lets it make room.
    pub media_limit_mb: u64,
    /// The largest a video file may grow, in megabytes, before the recording goes on in a new
    /// file, from the next keyframe; 0 for no limit.
    pub video_limit_mb: u64,
    /// Whether, at the media limit, the oldest snapshots and videos are deleted to make room for
    /// new ones, rather than nothing new being taken.
    pub overwrite_oldest: bool,
    /// How the videos events save are made, for every camera that doesn't have its own
    /// (`CameraConfig::event_video`).
    pub event_video: crate::EventVideo,
}

impl Default for Automation {
    fn default() -> Self {
        Self {
            log_events: true,
            event_capture: EventCapture::Picture,
            event_window_minutes: 5,
            media_limit_mb: 0,
            video_limit_mb: 0,
            overwrite_oldest: false,
            event_video: crate::EventVideo::default(),
        }
    }
}

impl Automation {
    /// The event window in milliseconds, never shorter than the quiet that ends a sighting
    /// ([`QUIET_FOR_MS`](crate::QUIET_FOR_MS)).
    pub fn event_window_ms(&self) -> u64 {
        (u64::from(self.event_window_minutes) * 60_000).max(crate::QUIET_FOR_MS)
    }

    /// The media limit in bytes, when there is one.
    pub fn media_limit(&self) -> Option<u64> {
        (self.media_limit_mb > 0).then(|| self.media_limit_mb.saturating_mul(MEGABYTE))
    }

    /// The largest a video file may be, in bytes, when there is a limit.
    pub fn video_limit(&self) -> Option<u64> {
        (self.video_limit_mb > 0).then(|| self.video_limit_mb.saturating_mul(MEGABYTE))
    }
}

/// `settings.json` is a document like the files in `data\settings`, with no layout number. When
/// it can't be used the app runs on the defaults, and its problems say so.
impl Document for Config {
    type Format = Json;

    fn moved_problem(path: &Path, error: &str, moved: &Path) -> String {
        format!(
            "{} could not be read ({error}). It was moved to {} and the app started with the \
             default config.",
            path.display(),
            moved.display()
        )
    }

    fn kept_problem(path: &Path, reason: &str) -> String {
        format!(
            "The app started with the default settings and will not save over {}, because \
             {reason}.",
            path.display()
        )
    }
}

/// The settings file and the copy of it the app is running with.
///
/// One of these exists per process, built by the app; tests build their own over a temporary
/// folder. The in-memory copy is only replaced after a write succeeds, so a failed save cannot
/// leave the running app using settings that are not on disk. The window watcher's thread and the
/// UI thread both save, and the file store keeps either from losing the other's change.
pub struct ConfigStore {
    file: FileStore<Config>,
}

impl ConfigStore {
    /// Loads the file at `path`, creating it from the defaults when it is missing, so it can be
    /// inspected and edited from the first launch.
    ///
    /// A file that is there but cannot be used never stops the app from starting, and is never
    /// written over, as the JSON stores in `data\settings` do it:
    ///
    /// - A file that is not valid JSON is moved aside to `settings.json.broken-<seconds>`, and a
    ///   new one is written from the defaults.
    /// - A file that cannot be read, or cannot be moved aside, stays as it is. The app runs on
    ///   the defaults and every save is refused, saying why.
    ///
    /// Either way [`ConfigStore::problem`] says what happened, for the app to tell the user.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let file = FileStore::open(path.into());
        // A first launch, or a file just moved aside: write the defaults, so there is a file to look
        // at and edit. If this fails, the first save writes it instead. A file that is there but
        // could not be read is left alone, as every write to it is refused.
        if !file.path().exists() {
            let _ = file.create_file();
        }
        Self { file }
    }

    /// What went wrong opening the file, if anything did.
    pub fn problem(&self) -> Option<&str> {
        self.file.problem()
    }

    /// The settings the app ships with: `settings.json` beside the executable.
    pub fn beside_the_executable() -> Self {
        Self::load(default_path())
    }

    /// Where the settings file lives.
    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// The `data` folder beside the settings file.
    pub fn data_dir(&self) -> PathBuf {
        self.path()
            .parent()
            .map(|dir| dir.join(DATA_DIR))
            .unwrap_or_else(|| PathBuf::from(DATA_DIR))
    }

    /// Where snapshots are written: the folder `storage` names, else `data\snapshots` beside the
    /// executable.
    pub fn snapshots_dir(&self) -> PathBuf {
        self.chosen(&self.current().storage.snapshots)
            .unwrap_or_else(|| self.default_snapshots_dir())
    }

    /// Where recordings are written: the folder `storage` names, else `data\videos` beside the
    /// executable.
    pub fn videos_dir(&self) -> PathBuf {
        self.chosen(&self.current().storage.videos)
            .unwrap_or_else(|| self.default_videos_dir())
    }

    /// Where snapshots go when `storage` names no folder.
    pub fn default_snapshots_dir(&self) -> PathBuf {
        self.data_dir().join(SNAPSHOTS_DIR)
    }

    /// Where recordings go when `storage` names no folder.
    pub fn default_videos_dir(&self) -> PathBuf {
        self.data_dir().join(VIDEOS_DIR)
    }

    /// The folder `storage` names, taken from beside the settings file when relative; `None`
    /// when it names none.
    fn chosen(&self, folder: &str) -> Option<PathBuf> {
        let folder = folder.trim();
        if folder.is_empty() {
            return None;
        }
        let folder = PathBuf::from(folder);
        Some(match self.path().parent() {
            Some(beside) if folder.is_relative() => beside.join(folder),
            _ => folder,
        })
    }

    /// Where the cameras and their groups are saved: `data\settings` beside the executable.
    pub fn settings_dir(&self) -> PathBuf {
        self.data_dir().join(SETTINGS_DIR)
    }

    /// The settings as they are in memory right now.
    ///
    /// Taken even if a thread panicked holding the lock: the config is only ever replaced whole,
    /// so it is still whole. Falling back to the defaults instead would have the next save write
    /// them over the user's settings.
    pub fn current(&self) -> Config {
        self.file.current()
    }

    /// Saves a change to the settings: preferences, window geometry and storage folders alike.
    /// Applies `change` to a copy, writes the file, then adopts the copy. A change that changes
    /// nothing writes nothing.
    pub fn update(&self, change: impl FnOnce(&mut Config)) -> Result<(), StoreError> {
        self.file.update(|config| {
            change(config);
            Ok(())
        })
    }
}

/// `settings.json` beside the executable.
pub fn default_path() -> PathBuf {
    exe_dir().join(FILE)
}

/// The folder the executable sits in. Settings and data belong here; the app never writes to
/// AppData or the user's profile.
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// Settings loaded from a folder of their own, removed when the test ends. Nothing here touches
    /// the real `settings.json` beside the test executable.
    struct TempConfig {
        dir: tempfile::TempDir,
        config: ConfigStore,
    }

    impl TempConfig {
        fn new() -> Self {
            let dir = tempfile::TempDir::new().expect("a folder for the test");
            let config = ConfigStore::load(dir.path().join(FILE));
            Self { dir, config }
        }

        fn file(&self) -> PathBuf {
            self.dir.path().join(FILE)
        }
    }

    #[test]
    fn the_settings_file_sits_beside_the_executable() {
        let path = default_path();
        println!("settings file: {}", path.display());
        assert_eq!(
            path.parent(),
            Some(exe_dir().as_path()),
            "settings.json must sit next to the executable, not inside the data folder"
        );
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("settings.json")
        );
    }

    #[test]
    fn snapshots_sit_in_the_data_folder() {
        let temp = TempConfig::new();
        let folder = temp.config.snapshots_dir();
        println!("snapshots: {}", folder.display());
        assert_eq!(
            folder.parent().map(PathBuf::from),
            Some(temp.config.data_dir()),
            "snapshots belong under the data folder"
        );
        assert_eq!(
            folder.file_name().and_then(|name| name.to_str()),
            Some("snapshots")
        );
        assert_eq!(
            temp.config
                .data_dir()
                .file_name()
                .and_then(|name| name.to_str()),
            Some("data"),
            "the data folder is called `data`"
        );
    }

    #[test]
    fn a_chosen_folder_takes_the_place_of_the_default() {
        let temp = TempConfig::new();
        let elsewhere = std::env::temp_dir().join("ipcamera-elsewhere");
        temp.config
            .update(|config| {
                config.storage.snapshots = elsewhere.display().to_string();
                config.storage.videos = "clips".into();
            })
            .unwrap();

        assert_eq!(temp.config.snapshots_dir(), elsewhere);
        assert_eq!(
            temp.config.videos_dir(),
            temp.dir.path().join("clips"),
            "a relative folder is taken from beside the settings file"
        );
        assert_eq!(
            ConfigStore::load(temp.file()).snapshots_dir(),
            elsewhere,
            "the choice is saved"
        );

        // Cleared, each goes back to its default; blank counts as cleared.
        temp.config
            .update(|config| {
                config.storage.snapshots.clear();
                config.storage.videos = "  ".into();
            })
            .unwrap();
        assert_eq!(
            temp.config.snapshots_dir(),
            temp.config.default_snapshots_dir()
        );
        assert_eq!(temp.config.videos_dir(), temp.config.default_videos_dir());
    }

    #[test]
    fn videos_sit_beside_the_snapshots() {
        let temp = TempConfig::new();
        let folder = temp.config.videos_dir();
        assert_eq!(
            folder.parent().map(PathBuf::from),
            Some(temp.config.data_dir())
        );
        assert_eq!(
            folder.file_name().and_then(|name| name.to_str()),
            Some("videos")
        );
    }

    /// The `settings.json.broken-<seconds>` files beside the settings file.
    fn moved_aside(temp: &TempConfig) -> Vec<PathBuf> {
        fs::read_dir(temp.dir.path())
            .expect("the folder must be there")
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("settings.json.broken-")
            })
            .map(|entry| entry.path())
            .collect()
    }

    #[test]
    fn a_file_that_is_not_json_is_moved_aside_never_overwritten() {
        let temp = TempConfig::new();
        let broken = "{ this is not json";
        fs::write(temp.file(), broken).expect("the test must be able to write the file");

        let config = ConfigStore::load(temp.file());
        assert_eq!(
            config.current().ui.volume,
            80.0,
            "a corrupt file must not stop the app from starting"
        );
        let problem = config.problem().expect("the user must be told");
        assert!(problem.contains("moved to"), "{problem}");
        assert!(problem.contains("line 1"), "it says where: {problem}");
        assert!(
            !problem.contains('\n'),
            "it fits one notification: {problem}"
        );

        // A save no longer has anything of the user's to write over.
        config
            .update(|config| config.ui.volume = 12.0)
            .expect("the app saves as usual");

        let kept = moved_aside(&temp);
        assert_eq!(kept.len(), 1, "the broken file is kept beside it");
        assert_eq!(fs::read_to_string(&kept[0]).unwrap(), broken);
        assert_eq!(ConfigStore::load(temp.file()).current().ui.volume, 12.0);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_never_written() {
        let temp = TempConfig::new();
        // A folder can't be read as text, which stands in for a locked or forbidden file.
        fs::remove_file(temp.file()).unwrap();
        fs::create_dir_all(temp.file()).unwrap();

        let config = ConfigStore::load(temp.file());

        assert_eq!(
            config.current().ui.appearance.dark_theme,
            "default-dark",
            "it still starts"
        );
        let problem = config.problem().expect("the user must be told");
        assert!(problem.contains("could not be read"), "{problem}");
        let error = config.update(|config| config.ui.volume = 12.0).unwrap_err();
        assert!(matches!(error, StoreError::ReadOnly { .. }), "{error}");
        assert_eq!(config.current().ui.volume, 80.0, "nothing was taken up");
        assert!(temp.file().is_dir(), "what was there is left alone");
    }

    #[test]
    fn a_good_file_has_no_problem() {
        let temp = TempConfig::new();
        assert!(temp.config.problem().is_none(), "a first launch is fine");
        assert!(ConfigStore::load(temp.file()).problem().is_none());
        assert!(moved_aside(&temp).is_empty());
    }

    #[test]
    fn a_first_launch_follows_windows_with_no_texture_and_no_elevation() {
        let temp = TempConfig::new();
        let appearance = temp.config.current().ui.appearance;
        assert!(appearance.follow_system);
        assert_eq!(appearance.texture, "none");
        assert_eq!(appearance.elevation, "none");
    }

    #[test]
    fn saves_from_several_threads_lose_nothing() {
        const THREADS: usize = 4;
        const SAVES: usize = 25;
        let temp = TempConfig::new();
        let config = &temp.config;
        config.update(|config| config.ui.volume = 0.0).unwrap();

        // Each save adds one, so a save that started from a stale copy shows as a missing one,
        // and two sharing the temporary file show as a failed save.
        std::thread::scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for _ in 0..SAVES {
                        config
                            .update(|config| config.ui.volume += 1.0)
                            .expect("every save should succeed");
                    }
                });
            }
        });

        let total = (THREADS * SAVES) as f32;
        assert_eq!(config.current().ui.volume, total);
        assert_eq!(ConfigStore::load(temp.file()).current().ui.volume, total);
    }

    #[test]
    fn a_missing_field_keeps_its_default() {
        let config: Config =
            serde_json::from_str(r#"{"ui":{"volume":42.0,"dark_theme":"monokai"}}"#).unwrap();
        assert_eq!(config.ui.volume, 42.0);
        assert_eq!(config.ui.rail_width, 248.0);
        assert_eq!(config.ui.sidebar_width, 232.0);
        assert_eq!(
            config.ui.appearance,
            Appearance {
                dark_theme: "monokai".into(),
                ..Appearance::default()
            },
            "the appearance's fields sit among the window's others"
        );
        assert!(config.ui.page.is_empty(), "it opens on the first view");
        assert_eq!(
            config.ui.events_table_width, None,
            "the events' table starts at its columns' width"
        );
        assert!(config.storage.snapshots.is_empty() && config.storage.videos.is_empty());
        assert!(!config.window.maximized);
    }

    #[test]
    fn a_full_config_round_trips() {
        let mut config = Config::default();
        config.storage.videos = "clips".into();
        config.window.maximized = true;
        config.ui.appearance.dark = false;
        config.ui.page = "view:3f2a".into();

        let text = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&text).unwrap();

        assert_eq!(back.storage.videos, "clips");
        assert!(back.window.maximized);
        assert!(!back.ui.appearance.dark);
        assert_eq!(back.ui.page, "view:3f2a");
    }

    #[test]
    fn automation_is_off_and_unlimited_until_set() {
        let config: Config = serde_json::from_str(r#"{"ui":{"volume":42.0}}"#).unwrap();
        assert_eq!(config.automation, Automation::default());
        assert!(
            config.automation.log_events,
            "events are logged until turned off"
        );
        assert_eq!(
            config.automation.event_capture,
            EventCapture::Picture,
            "each event saves a picture until the user picks"
        );
        assert_eq!(config.automation.event_window_ms(), 5 * 60_000);
        assert_eq!(config.automation.media_limit(), None);
        assert_eq!(config.automation.video_limit(), None);

        let set: Config = serde_json::from_str(
            r#"{"automation":{"event_capture":"picture_and_video",
                "event_window_minutes":0,"media_limit_mb":5120,"video_limit_mb":250}}"#,
        )
        .unwrap();
        assert!(set.automation.event_capture.picture() && set.automation.event_capture.video());
        assert_eq!(
            set.automation.event_window_ms(),
            crate::QUIET_FOR_MS,
            "no window is the camera's own quiet"
        );
        assert_eq!(set.automation.media_limit(), Some(5 * 1024 * MEGABYTE));
        assert_eq!(set.automation.video_limit(), Some(250 * MEGABYTE));
        assert!(!set.automation.overwrite_oldest);
    }
}
