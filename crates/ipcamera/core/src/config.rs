//! The app's settings, `settings.json` beside the executable. [`ConfigStore::update`] is the one
//! way to change them, and adopts a change only once it is written.

use std::path::{Path, PathBuf};

pub use dp_store::WindowState;
use dp_store::{Appearance, Document, FileStore, Json};
use serde::{Deserialize, Serialize};

use crate::StoreError;

const DATA_DIR: &str = "data";
const FILE: &str = "settings.json";
const SNAPSHOTS_DIR: &str = "snapshots";
const VIDEOS_DIR: &str = "videos";
const SETTINGS_DIR: &str = "settings";

/// The whole settings tree. Not `deny_unknown_fields`: an unknown field must not fail the file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The app's preferences.
    pub ui: Ui,
    /// Where the window was last seen.
    pub window: WindowState,
    /// Where snapshots and recordings go.
    pub storage: Storage,
    /// What the app does by itself, and how much media it may keep.
    pub automation: Automation,
    /// Who may use `ipcamera-server`.
    pub server: ServerAccess,
}

/// Who may use `ipcamera-server`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerAccess {
    /// The password's salted Argon2 hash, from `--set-password`; empty for no password.
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
    /// The sidebar's width, in points.
    pub sidebar_width: f32,
    /// How the window looks, flattened among these fields.
    #[serde(flatten)]
    pub appearance: Appearance,
    /// The page last shown (`dashboard`, `view:<id>`, `camera:<id>`); empty for the first view.
    pub page: String,
    /// Whether the app shows a message as an event starts.
    pub alerts: bool,
    /// The Events page's table width, in points; `None` for its columns' own width.
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

/// Where the app saves what it makes; a relative folder is taken from beside the executable.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Storage {
    /// Where snapshots are written; empty for `data\snapshots`.
    pub snapshots: String,
    /// Where recordings are written; empty for `data\videos`.
    pub videos: String,
}

/// A megabyte: 1,048,576 bytes.
pub const MEGABYTE: u64 = 1024 * 1024;

/// What each event saves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCapture {
    /// Nothing: the event is only a note in the log.
    Nothing,
    /// A snapshot as the event starts.
    #[default]
    Picture,
    /// A video of each sighting, as [`EventVideo`](crate::EventVideo) says.
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

/// What the app does by itself, and how much media it may keep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Automation {
    /// Whether the app watches every camera it can and logs what they see as events.
    pub log_events: bool,
    /// What each event saves.
    pub event_capture: EventCapture,
    /// How long after the last sighting, in minutes, a new one joins the same event; 0 groups none.
    pub event_window_minutes: u32,
    /// The most the snapshots and videos may hold together, in megabytes; 0 for no limit.
    pub media_limit_mb: u64,
    /// The largest a video file may grow, in megabytes, before it goes on in a new one; 0 for none.
    pub video_limit_mb: u64,
    /// Whether, at the media limit, the oldest files are deleted to make room.
    pub overwrite_oldest: bool,
    /// How event videos are made for a camera without its own settings.
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
    /// The event window in ms, never shorter than [`QUIET_FOR_MS`](crate::QUIET_FOR_MS).
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

/// The settings file and the copy the app runs with, replaced only after a write succeeds.
pub struct ConfigStore {
    file: FileStore<Config>,
}

impl ConfigStore {
    /// Loads the file at `path`, creating it from the defaults when missing. A file that can't be
    /// used is never written over; [`problem`](Self::problem) says what happened.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let file = FileStore::open(path.into());
        // An unreadable file is left alone: every write to it is refused.
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

    /// The settings kept in `folder` instead, with the `data` folder beside them.
    pub fn in_folder(folder: &Path) -> Self {
        Self::load(folder.join(FILE))
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

    /// Where snapshots are written: the folder `storage` names, else `data\snapshots`.
    pub fn snapshots_dir(&self) -> PathBuf {
        self.chosen(&self.current().storage.snapshots)
            .unwrap_or_else(|| self.default_snapshots_dir())
    }

    /// Where recordings are written: the folder `storage` names, else `data\videos`.
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

    /// Where the cameras, groups and views are saved: `data\settings`.
    pub fn settings_dir(&self) -> PathBuf {
        self.data_dir().join(SETTINGS_DIR)
    }

    /// The settings as they are in memory now.
    pub fn current(&self) -> Config {
        self.file.current()
    }

    /// Saves a change to the settings; one that changes nothing writes nothing.
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
        // A folder stands in for a file that can't be read.
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
