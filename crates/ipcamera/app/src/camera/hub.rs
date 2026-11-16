//! Every camera the app holds, and which of them stream: [`Hub`], the one the app runs on
//! ([`HUB`]).
//!
//! The hub holds one [`Camera`] per saved camera, in the library's order, and adds and removes
//! them as the user does. What TSX last said is on screen is kept here too, and decides which
//! cameras connect. `nsh test`'s headless runner gets a hub of its own over a throwaway folder,
//! which can reach no camera.
//!
//! `ipcamera-server` can share the app's files. Every couple of seconds a thread of the hub's own
//! takes what the server saved since ([`Hub::follow`]), so the app shows a camera added in the
//! browser and restarts one changed there, without a restart.
//!
//! **Events.** Every second another thread keeps each camera's watch going and logs what the
//! watches saw ([`Hub::watch_cameras`]) in `data\events`, grouped within Settings → Automation's
//! window. A camera is watched whether or not it is on screen, so the log and the alerts cover
//! them all.
//!
//! **Automation.** What an event saves, Settings → Automation says ([`Hub::log_event`]): a
//! snapshot as it starts (the camera's own picture where it hands one out, else the picture on
//! screen, else one from its stream, connected for it), a video of each sighting
//! ([`Hub::record_sighting`]), or both. Both go into the snapshots and videos folders, tagged with
//! what the camera saw, and are linked to the event.
//!
//! How a video is made, the camera's [`EventVideo`] says, its own or Settings → Automation's: a
//! sighting lasts its post-roll, so what the camera sees again sooner continues the video; plain
//! motion may hold it open; only the kinds picked save one, though another kind seen while it
//! records keeps it going and joins its tags, which each file takes in its name as it closes
//! ([`Hub::retag_closed`]); it records the stream picked; and a file that runs as long as the
//! longest video, or grows as large as Settings → Automation's video file size, goes on in a new
//! one. With a pre-roll, each camera the app records events of keeps that stream connected,
//! decoding nothing, and holding back its last seconds (`Camera::set_armed`), so a video starts
//! before the camera saw something.
//!
//! The limits are kept here too: a snapshot or a recording is refused once the snapshots and
//! videos together hold the most they may ([`Hub::room_for_new`]), or the oldest are deleted to
//! make room where the user allows it; and while anything records, the same thread stops every
//! recording once the media reach their limit with nothing left to delete
//! ([`Hub::keep_within_limits`]).

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
    thread,
    time::Duration,
};

use ipcamera_core::{
    Added, CameraConfig, CameraEvent, CameraId, CameraUpdate, Change, EventFile, EventLog,
    EventRules, EventStep, EventVideo, Library, SeenEvent, TileStream,
    config::{Automation, ConfigStore},
    sync::LockExt,
    watches,
};
use ipcamera_media::{DecodedFrame, RecordRules};
use ipcamera_storage::{
    quota::{self, size_label},
    recording, saved_file, snapshot,
    snapshot::{write_from_jpeg, write_snapshot, write_tagged},
};

use super::device::{Camera, StreamKey, file_name};
use crate::{
    bridge::{CameraChanges, CameraForm},
    form, library,
};

/// How Settings → Automation has the watches log events, for every camera: what starts every
/// watch again as it changes. `None` while events aren't logged.
#[derive(Clone, Debug, PartialEq, Eq)]
struct WatchedAs {
    window_ms: u64,
    /// Whether events record videos, whose post-roll is then a sighting's quiet.
    videos: bool,
    /// Settings → Automation's event video settings.
    event_video: EventVideo,
}

fn watched_as(automation: &Automation) -> Option<WatchedAs> {
    automation.log_events.then(|| WatchedAs {
        window_ms: automation.event_window_ms(),
        videos: automation.event_capture.video(),
        event_video: automation.event_video.clone(),
    })
}

/// How `camera`'s watch groups what it sees into events, as Settings → Automation and the
/// camera's own event video settings say: `None` while events aren't logged, for every camera or
/// for this one. Where events record videos, a sighting lasts the video's post-roll, so each
/// sighting is one video.
pub(super) fn watch_rules(automation: &Automation, camera: &CameraConfig) -> Option<EventRules> {
    if !automation.log_events || !camera.log_events {
        return None;
    }
    let window_ms = automation.event_window_ms();
    if !automation.event_capture.video() {
        return Some(EventRules::grouping(window_ms));
    }
    let video = camera.event_video_or(&automation.event_video);
    Some(EventRules {
        window_ms,
        quiet_ms: video.quiet_ms(),
        motion_continues: video.motion_continues,
    })
}

/// The stream key of the stream `stream` names: `None` for the camera's own.
fn stream_key(stream: &TileStream) -> StreamKey {
    (*stream != TileStream::Own).then(|| stream.clone())
}

/// An event's video still being written, as the hub last saw it: the event, and everything the
/// camera saw while it recorded, which the file takes in its name once it closes.
struct OpenVideo {
    event: String,
    tags: Vec<String>,
}

/// Set `IPCAMERA_SOURCE=pattern` to run on the test picture, with nothing reaching a camera.
pub(super) const SOURCE_VARIABLE: &str = "IPCAMERA_SOURCE";

/// `nsh test`'s headless runner sits beside the development host, so without its own settings it
/// would read the same `data\settings`, find the saved password, and connect to the real camera from
/// every test.
pub(super) static HUB: LazyLock<Hub> = LazyLock::new(|| {
    if dp_ui::is_test_runner() {
        Hub::new(Arc::new(throwaway_settings()), Source::Offline)
    } else {
        let config = Arc::new(ConfigStore::beside_the_executable());
        follow_other_programs();
        watch_cameras_always();
        Hub::new(config, Source::from_env())
    }
});

/// How often the app looks for what another program saved in its files. It costs a look at three
/// files' details, off the main thread, and nothing is read while they are the same.
const FOLLOW_EVERY: Duration = Duration::from_secs(2);

/// Starts the thread that keeps [`HUB`] in step with its files when `ipcamera-server` shares them
/// (`Hub::follow`), for as long as the app runs. The headless test runner gets none: nothing else
/// writes its throwaway folder.
fn follow_other_programs() {
    // If the thread can't start, the app runs as it did before: it never overwrites the other
    // program's change either way, it only learns of one when it next starts.
    let _ = thread::Builder::new()
        .name("ipcamera-settings".into())
        .spawn(|| {
            loop {
                // The first wait also lets `HUB` finish being made: this starts from inside that.
                thread::sleep(FOLLOW_EVERY);
                HUB.follow();
            }
        });
}

/// How often the app looks after the cameras' watches and logs what they saw: as often as a watch
/// asks its camera.
const WATCH_EVERY: Duration = Duration::from_secs(1);

/// Starts the thread that keeps the cameras' watches going and logs their events
/// ([`Hub::watch_cameras`]), for as long as the app runs. The headless test runner gets none: it
/// reaches no camera.
fn watch_cameras_always() {
    // If the thread can't start, the cameras are still shown; nothing is watched or logged.
    let _ = thread::Builder::new()
        .name("ipcamera-events".into())
        .spawn(|| {
            loop {
                // The first wait also lets `HUB` finish being made: this starts from inside that.
                thread::sleep(WATCH_EVERY);
                HUB.watch_cameras();
                HUB.keep_within_limits();
            }
        });
}

/// Default settings in a folder of their own, so tests start the same way on every machine and
/// cannot change what the app saved. The cameras, groups and views go in its `data\settings`,
/// which starts with [`test_camera`].
pub(super) fn throwaway_settings() -> ConfigStore {
    let folder = std::env::temp_dir().join("ipcamera-nsh-tests");
    let _ = std::fs::remove_dir_all(&folder);
    let config = ConfigStore::load(folder.join("settings.json"));
    seed_test_camera(&config);
    config
}

/// The camera tests start with, since a first launch has none: no password, and an address in
/// 192.0.2.0/24, a range kept for documentation that no device answers on. Its name sorts before
/// "Deleted camera", so the TSX tests of the pages that list cameras by name find it first.
pub(super) fn test_camera() -> CameraConfig {
    CameraConfig {
        name: "Camera 1".into(),
        host: "192.0.2.10".into(),
        user: "admin".into(),
        ..CameraConfig::default()
    }
}

/// Saves [`test_camera`] in the library beside `settings` and shows it in the first view's first
/// tile, so a test finds a camera on the stage as a user with one camera would.
pub(super) fn seed_test_camera(config: &ConfigStore) {
    let library = Library::beside(config);
    let Ok(id) = library.add_camera(test_camera(), None) else {
        return;
    };
    if let Some(view) = library.views().first() {
        let _ = library.show_in_tile(&view.id, 0, Some(&id.into()));
    }
}

/// Where a camera's frames come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    Stream,
    Pattern,
    /// Nowhere: nothing here can reach a camera, even if a test presses Connect.
    Offline,
}

impl Source {
    pub(super) fn from_env() -> Self {
        match std::env::var(SOURCE_VARIABLE).as_deref() {
            Ok("pattern") => Self::Pattern,
            _ => Self::Stream,
        }
    }
}

/// One of a camera's streams, by the camera's id: what a tile shows.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct StreamOf {
    pub(super) camera: String,
    pub(super) stream: StreamKey,
}

impl StreamOf {
    /// Camera `camera`'s own stream.
    pub(super) fn own(camera: &str) -> Self {
        Self {
            camera: camera.to_string(),
            stream: None,
        }
    }
}

/// What TSX last said is on screen.
#[derive(Default)]
pub(super) struct Screen {
    /// The streams the tiles show.
    pub(super) shown: HashSet<StreamOf>,
    /// The streams that stay connected although no tile shows them.
    pub(super) kept: HashSet<StreamOf>,
    /// The selected tile's stream, the only one heard.
    pub(super) audible: Option<StreamOf>,
}

impl Screen {
    /// What it asks of camera `id`.
    fn of(&self, id: &str) -> Wanted {
        let streams = |all: &HashSet<StreamOf>| {
            all.iter()
                .filter(|shown| shown.camera == id)
                .map(|shown| shown.stream.clone())
                .collect()
        };
        Wanted {
            shown: streams(&self.shown),
            kept: streams(&self.kept),
            audible: self
                .audible
                .as_ref()
                .filter(|audible| audible.camera == id)
                .map(|audible| audible.stream.clone()),
        }
    }
}

/// What the screen asks of one camera, stream by stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Wanted {
    /// The streams a tile shows.
    pub(super) shown: HashSet<StreamKey>,
    /// The streams that stay connected although no tile shows them.
    pub(super) kept: HashSet<StreamKey>,
    /// The selected tile's stream, when it is this camera's: the only one heard.
    pub(super) audible: Option<StreamKey>,
}

/// The app's cameras, and where they are saved.
pub(super) struct Hub {
    /// `settings.json`: window and audio preferences, and the snapshot and video folders.
    pub(super) config: Arc<ConfigStore>,
    /// The cameras, groups and views in `data\settings`.
    pub(super) library: Library,
    pub(super) source: Source,
    /// One per saved camera, in the library's order.
    pub(super) cameras: Mutex<Vec<Arc<Camera>>>,
    pub(super) screen: Mutex<Screen>,
    /// What the cameras saw, in `data\events`.
    pub(super) events: EventLog,
    /// How the watches were last started, as Settings → Automation said then; `None` while events
    /// aren't logged. They start again as it changes.
    watching_as: Mutex<Option<WatchedAs>>,
    /// The events' videos being written, by file, until each closes and takes its tags.
    open_videos: Mutex<HashMap<PathBuf, OpenVideo>>,
}

impl Hub {
    pub(super) fn new(config: Arc<ConfigStore>, source: Source) -> Self {
        let library = Library::beside(&config);
        let cameras = library
            .cameras()
            .into_iter()
            .map(|setting| Arc::new(Camera::new(setting, config.clone(), source)))
            .collect();
        let events = EventLog::open(&config.data_dir().join(EventLog::FOLDER));
        // An event still going on when the app last closed ends where it started: when the
        // camera stopped seeing it can't be known.
        let _ = events.end_unseen(|_| false, None);
        let watching_as = Mutex::new(watched_as(&config.current().automation));
        Self {
            watching_as,
            config,
            library,
            source,
            cameras: Mutex::new(cameras),
            screen: Mutex::default(),
            events,
            open_videos: Mutex::default(),
        }
    }

    /// Keeps every camera's watch going, and acts on what the watches saw since the last call,
    /// while Settings → Automation logs events, for each camera that logs its own. A camera's
    /// watch starts once it may log in (`Camera::tend_watch`): a camera the screen is about to
    /// connect waits for its stream. An event ends now when its camera is no longer watched (it
    /// was deleted, or its login was refused), when a new watch takes over from the one that saw
    /// it (its settings changed), or when events are turned off, for every camera or for its own;
    /// so does the recording it started. Turning events on or off, or
    /// changing how they are grouped or their videos made, starts every watch again, as it now
    /// asks; a camera whose own settings change starts its own again. Each camera whose events
    /// record videos with a pre-roll keeps that stream connected to hold it back. Each file a
    /// sighting's video opens is linked to its event here, as the recorder names it only once it
    /// opens, and each takes the tags of everything its camera saw once it closes. Reads and writes
    /// files, so it runs off the main thread.
    pub(super) fn watch_cameras(&self) {
        let automation = self.config.current().automation;
        let now_as = watched_as(&automation);
        if *self.watching_as.locked() != now_as {
            *self.watching_as.locked() = now_as;
            for camera in self.all() {
                camera.release_watch();
            }
        }
        let now = saved_file::now_ms();
        let mut watching = HashSet::new();
        for camera in self.all() {
            let setting = camera.setting();
            let wanted = self.screen.locked().of(&camera.id);
            let coming = camera.ready()
                && !camera.held()
                && !(wanted.shown.is_empty() && wanted.kept.is_empty());
            let rules = watch_rules(&automation, &setting);
            let logs = rules.is_some();
            if camera.tend_watch(coming, rules) {
                // A new watch knows nothing of what the last one saw going on.
                let _ = self
                    .events
                    .end_unseen(|id| id.as_str() != camera.id, Some(now));
                self.stop_recording_by_itself(&camera);
            }
            if camera.set_armed(self.armed(&automation, &setting, &camera)) {
                camera.reconcile(|| self.screen.locked().of(&camera.id));
            }
            if logs {
                for seen in camera.take_events() {
                    self.log_event(&camera, seen);
                }
                if let Some((event, file)) = camera.event_recording() {
                    let _ = self.events.link(&event, EventFile::Video(file_name(&file)));
                    let tags = camera.event_tags();
                    self.open_videos
                        .locked()
                        .insert(file, OpenVideo { event, tags });
                }
                if camera.watching() {
                    watching.insert(camera.id.clone());
                }
            } else {
                // No event will end what an event started.
                self.stop_recording_by_itself(&camera);
            }
        }
        let _ = self
            .events
            .end_unseen(|camera| watching.contains(camera.as_str()), Some(now));
        self.retag_closed();
    }

    /// The stream `camera` keeps connected to hold back the seconds before an event for its
    /// video, and how many: while events are logged and record videos with a pre-roll, for a
    /// camera the app can watch, whose own events are logged, that may connect by itself and that
    /// the user hasn't disconnected. The test picture has nothing to hold back.
    fn armed(
        &self,
        automation: &Automation,
        setting: &CameraConfig,
        camera: &Camera,
    ) -> Option<(StreamKey, Duration)> {
        if self.source != Source::Stream
            || !automation.log_events
            || !setting.log_events
            || !automation.event_capture.video()
            || !watches(setting)
            || camera.held()
            || !camera.ready()
        {
            return None;
        }
        let video = setting.event_video_or(&automation.event_video);
        Some((stream_key(&video.stream), video.before()?))
    }

    /// Renames each event video that closed since the last look to carry the tags of everything
    /// its camera saw while it recorded, and keeps it linked to its event under its new name. One
    /// that can't be renamed (open in a player, or its new name taken) keeps the name it had.
    /// Renames files, so it runs off the main thread.
    pub(super) fn retag_closed(&self) {
        let busy: HashSet<PathBuf> = self
            .all()
            .iter()
            .filter_map(|camera| camera.recording_file())
            .collect();
        self.open_videos.locked().retain(|file, open| {
            if busy.contains(file) {
                return true;
            }
            let tags: Vec<&str> = open.tags.iter().map(String::as_str).collect();
            if let Ok(renamed) = recording::retag(file, &tags)
                && renamed != *file
            {
                let _ =
                    self.events
                        .rename_video(&open.event, &file_name(file), &file_name(&renamed));
            }
            false
        });
    }

    /// Stops `camera`'s recording when an event started it, and disconnects the stream it kept.
    fn stop_recording_by_itself(&self, camera: &Camera) {
        if camera.is_recording_by_itself() {
            camera.stop_recording();
            camera.reconcile(|| self.screen.locked().of(&camera.id));
        }
    }

    /// Logs what `camera`'s watch saw, and saves what Settings → Automation asks: as the event
    /// starts, a video of its first sighting and its snapshot; as each later sighting starts,
    /// another video; as each ends, the video stops. A video is only of a kind the camera's event
    /// video settings pick, and starts when one is first seen during a sighting; what else the
    /// camera sees while it records joins its tags. The snapshot is tagged with what the camera saw
    /// as the event started, and everything saved is linked to the event. An event deleted while
    /// it was quiet starts over when the camera sees something again. Saving waits on the camera
    /// and the disk, so the caller is off the main thread.
    pub(super) fn log_event(&self, camera: &Camera, seen: SeenEvent) {
        let SeenEvent {
            step,
            event,
            tags,
            still,
        } = seen;
        let id = event.id.clone();
        let step = if step == EventStep::SeenAgain && !self.events.has(&id) {
            EventStep::Started
        } else {
            step
        };
        let _ = self.events.record(event);
        camera.note_event_tags(&tags);
        let automation = self.config.current().automation;
        let video = camera.setting().event_video_or(&automation.event_video);
        let records = automation.event_capture.video() && video.saves(&tags);
        match step {
            EventStep::Started => {
                // Recording first: its stream connects for it, and the snapshot can then come
                // from it rather than from a connection of its own.
                if records {
                    self.record_sighting(camera, &id, &tags, &video);
                }
                if automation.event_capture.picture()
                    && let Some(name) = self.event_snapshot(camera, still, &tags)
                {
                    let _ = self.events.link(&id, EventFile::Snapshot(name));
                }
            }
            EventStep::SeenAgain if records => self.record_sighting(camera, &id, &tags, &video),
            // A kind to record, first seen during a sighting of another.
            EventStep::Added if records && !camera.is_recording() => {
                self.record_sighting(camera, &id, &tags, &video);
            }
            EventStep::Quiet | EventStep::Closed => self.stop_recording_by_itself(camera),
            EventStep::SeenAgain | EventStep::Added => {}
        }
    }

    /// Saves the snapshot of `camera`'s event as it starts, tagged with `tags`, returning its
    /// name: the camera's own picture, `still`, where it hands one out; else the picture on
    /// screen; else one from its own stream, connected for it as a recording's is, unless the
    /// user disconnected the camera or it has no password. None once the media are at Settings →
    /// Automation's limit (`room_for_new`), nor when no picture comes.
    fn event_snapshot(
        &self,
        camera: &Camera,
        still: Option<Vec<u8>>,
        tags: &[&str],
    ) -> Option<String> {
        self.room_for_new().ok()?;
        let folder = self.config.snapshots_dir();
        let stamp = saved_file::now_seconds();
        let from_camera =
            still.and_then(|jpeg| write_from_jpeg(&folder, &jpeg, stamp, &camera.id, tags).ok());
        let path = match from_camera {
            Some(path) => path,
            None => {
                let picture = match camera.shown_picture() {
                    Some(picture) => picture,
                    None if camera.held() || !camera.ready() => return None,
                    None => {
                        let picture = camera.picture_for_event();
                        camera.reconcile(|| self.screen.locked().of(&camera.id));
                        picture.ok()?
                    }
                };
                write_tagged(&folder, &picture, stamp, &camera.id, tags).ok()?
            }
        };
        Some(file_name(&path))
    }

    /// Starts `camera` recording a sighting of event `id` as it starts seeing something, as
    /// `video` says, its files tagged with `tags`, until the sighting ends: from the seconds held
    /// back before it where there are any, on the stream `video` picks, each file going on in a
    /// new one after the longest video or at Settings → Automation's video file size. A camera the
    /// user disconnected, one with no password and one already recording are left as they are. One
    /// that can't record says why, once, as a recording that stops by itself does. Starting a
    /// stream waits on it, so the caller is off the main thread.
    pub(super) fn record_sighting(
        &self,
        camera: &Camera,
        id: &str,
        tags: &[&str],
        video: &EventVideo,
    ) {
        if camera.held() || !camera.ready() || camera.is_recording() {
            return;
        }
        let rules = RecordRules {
            from_preroll: true,
            split_after: video.longest(),
            split_bytes: self.config.current().automation.video_limit(),
        };
        if let Err(why) = self
            .room_for_new()
            .and_then(|()| camera.record_by_itself(id, tags, stream_key(&video.stream), rules))
        {
            camera.set_record_problem(format!("Didn't record what it saw. {why}"));
        }
        // Its own stream connected for the recording decodes only what a tile shows; one that
        // couldn't start recording disconnects again.
        camera.reconcile(|| self.screen.locked().of(&camera.id));
    }

    /// Deletes for good what `events` saved, as they are deleted: their snapshots, with their
    /// thumbnails, and their recordings, but never one a camera is still writing. A file already
    /// gone, deleted from its own page, is passed over. Writes files, so the caller is off the main
    /// thread.
    pub(super) fn delete_saved(&self, events: &[CameraEvent]) {
        let snapshots = self.config.snapshots_dir();
        let videos = self.config.videos_dir();
        let busy: HashSet<String> = self
            .all()
            .iter()
            .filter_map(|camera| camera.recording_file())
            .map(|path| file_name(&path))
            .collect();
        for event in events {
            if let Some(name) = &event.snapshot {
                let _ = snapshot::delete(&snapshots, name);
            }
            for name in event.videos.iter().filter(|name| !busy.contains(*name)) {
                let _ = recording::delete(&videos, name);
            }
        }
    }

    /// The files cameras are writing now, as the media folders' listing names them.
    fn busy_files(&self) -> Vec<PathBuf> {
        self.all()
            .iter()
            .filter_map(|camera| camera.recording_file())
            .map(|file| saved_file::absolute(&file))
            .collect()
    }

    /// What the snapshots and videos hold together, in bytes.
    pub(super) fn media_size(&self) -> u64 {
        let (videos, snapshots) = self.media_sizes();
        videos + snapshots
    }

    /// What the videos and the snapshots each hold, in bytes, from the folder listings and the
    /// files cameras are writing.
    pub(super) fn media_sizes(&self) -> (u64, u64) {
        let busy = self.busy_files();
        let files = quota::media_files(
            &self.config.snapshots_dir(),
            &self.config.videos_dir(),
            &busy,
        );
        let (videos, snapshots): (Vec<_>, Vec<_>) = files.into_iter().partition(|file| file.video);
        (quota::total(&videos), quota::total(&snapshots))
    }

    /// What the snapshots and videos hold together, after the oldest are deleted to get under
    /// `most` where `automation` lets the app.
    fn media_held(&self, automation: &Automation, most: u64, busy: &[PathBuf]) -> u64 {
        let files = quota::media_files(
            &self.config.snapshots_dir(),
            &self.config.videos_dir(),
            busy,
        );
        if automation.overwrite_oldest {
            quota::make_room(&files, most, busy)
        } else {
            quota::total(&files)
        }
    }

    /// Whether Settings → Automation's media limit leaves room for a new snapshot or recording,
    /// deleting the oldest first where it lets the app; why not, as a sentence, when it doesn't.
    /// Reads the folders, so the caller is off the main thread.
    pub(super) fn room_for_new(&self) -> Result<(), String> {
        let automation = self.config.current().automation;
        let Some(most) = automation.media_limit() else {
            return Ok(());
        };
        let held = self.media_held(&automation, most, &self.busy_files());
        if held < most {
            return Ok(());
        }
        let limit = size_label(most);
        Err(if automation.overwrite_oldest {
            format!(
                "Snapshots and videos hold {limit}, the most Settings → Automation allows, and \
                 what is left is still recording."
            )
        } else {
            format!(
                "Snapshots and videos hold {limit}, the most Settings → Automation allows. Delete \
                 some, raise the limit, or let the app delete the oldest to make room."
            )
        })
    }

    /// Keeps what records within Settings → Automation's media limit, while anything records: once
    /// the snapshots and videos together reach it, deletes the oldest to make room where the user
    /// allows it, else stops every recording, saying why, once. (A file that grows to the largest a
    /// video may be goes on in a new one by itself, in the recorder.) Reads the folders, so it runs
    /// off the main thread; with nothing recording it reads nothing.
    pub(super) fn keep_within_limits(&self) {
        let recording: Vec<(Arc<Camera>, PathBuf)> = self
            .all()
            .into_iter()
            .filter_map(|camera| {
                let file = camera.recording_file()?;
                Some((camera, saved_file::absolute(&file)))
            })
            .collect();
        if recording.is_empty() {
            return;
        }
        let automation = self.config.current().automation;
        let mut stopped = false;

        if let Some(most) = automation.media_limit() {
            let busy: Vec<PathBuf> = recording.iter().map(|(_, file)| file.clone()).collect();
            if self.media_held(&automation, most, &busy) >= most {
                for (camera, _) in &recording {
                    camera.stop_recording_because(format!(
                        "Recording stopped: snapshots and videos reached {}, the most Settings → \
                         Automation allows.",
                        size_label(most)
                    ));
                }
                stopped = true;
            }
        }

        if stopped {
            // A stream connected only for its recording disconnects.
            self.reconcile();
        }
    }

    pub(super) fn camera(&self, id: &str) -> Result<Arc<Camera>, String> {
        self.cameras
            .locked()
            .iter()
            .find(|camera| camera.id == id)
            .cloned()
            .ok_or_else(|| format!("no camera `{id}`"))
    }

    /// Every camera, copied out so the list is not held while they are asked anything.
    pub(super) fn all(&self) -> Vec<Arc<Camera>> {
        self.cameras.locked().clone()
    }

    /// Saves a new camera and starts holding it, returning its id. It is not connected: that
    /// happens when it comes on screen.
    pub(super) fn add(&self, setting: CameraConfig) -> Result<String, String> {
        let id = self
            .library
            .apply(&Change::AddCamera { camera: setting })
            .map_err(|error| format!("Could not save the camera: {error}"))?
            .added;
        let Some(Added::Camera(id)) = id else {
            return Err("The camera was not saved.".to_string());
        };
        let saved = self
            .library
            .camera(&id)
            .ok_or_else(|| "The camera was not saved.".to_string())?;
        self.cameras.locked().push(Arc::new(Camera::new(
            saved,
            self.config.clone(),
            self.source,
        )));
        Ok(id.to_string())
    }

    /// Deletes camera `id` everywhere, then stops it. Waits for its stream, so the caller is off
    /// the main thread.
    pub(super) fn delete(&self, id: &str) -> Result<(), String> {
        let camera = self.camera(id)?;
        // The files first: a camera that cannot be deleted keeps running as it was.
        self.library
            .apply(&Change::DeleteCamera {
                id: CameraId::from(id),
            })
            .map_err(|error| format!("Could not delete the camera: {error}"))?;
        self.cameras.locked().retain(|held| held.id != id);

        camera.disconnect();
        camera.release_ptz();
        camera.release_watch();
        Ok(())
    }

    /// Takes what another program saved in `data\settings` since the app last read or wrote it,
    /// as `ipcamera-server` does when it shares these files: a camera it added is held from now on,
    /// one it deleted stops and goes, and one it changed restarts a running stream with its new
    /// settings, as a save made here does. Reads files and waits on streams, so it runs off the
    /// main thread.
    pub(super) fn follow(&self) {
        if !self.library.refresh() {
            return;
        }
        let saved = self.library.cameras();
        let gone: Vec<Arc<Camera>> = {
            let mut cameras = self.cameras.locked();
            let (kept, gone) = cameras
                .drain(..)
                .partition(|held| saved.iter().any(|camera| camera.id.as_str() == held.id));
            *cameras = kept;
            gone
        };
        for camera in gone {
            camera.disconnect();
            camera.release_ptz();
            camera.release_watch();
        }
        for setting in &saved {
            match self.camera(setting.id.as_str()) {
                Ok(camera) if camera.setting() != *setting => {
                    let _ = camera.adopt(|| Ok(setting.clone()));
                }
                Ok(_) => {}
                Err(_) => self.cameras.locked().push(Arc::new(Camera::new(
                    setting.clone(),
                    self.config.clone(),
                    self.source,
                ))),
            }
        }
        self.cameras.locked().sort_by_key(|held| {
            saved
                .iter()
                .position(|camera| camera.id.as_str() == held.id)
                .unwrap_or(usize::MAX)
        });
    }

    /// Moves camera `id` to `index` among the cameras, in the files and here, so every list the
    /// app shows follows. Its stream carries on as it was.
    pub(super) fn move_camera(&self, id: &str, index: u32) -> Result<(), String> {
        self.library
            .apply(&library::move_camera(id, index))
            .map_err(|error| format!("Could not move the camera: {error}"))?;
        let order: Vec<CameraId> = self
            .library
            .cameras()
            .into_iter()
            .map(|camera| camera.id)
            .collect();
        self.cameras.locked().sort_by_key(|held| {
            order
                .iter()
                .position(|saved| saved.as_str() == held.id)
                .unwrap_or(usize::MAX)
        });
        Ok(())
    }

    /// Saves `form` as camera `id`'s settings, and restarts a running stream with them.
    pub(super) fn save(&self, id: &str, form: &CameraForm) -> Result<(), String> {
        let camera = self.camera(id)?;
        let camera_id = CameraId::from(id);
        camera.save(form::to_config(form), |setting| {
            // The desktop shows the password in the form, so what the form holds is what to save.
            let update = CameraUpdate {
                id: camera_id.clone(),
                camera: setting,
                keep_password: false,
            };
            self.library
                .apply(&Change::UpdateCameras {
                    cameras: vec![update],
                })
                // The error already names the file.
                .map_err(|error| format!("Could not save {error}"))?;
            self.library
                .camera(&camera_id)
                .ok_or_else(|| format!("no camera `{id}`"))
        })
    }

    /// Gives every camera in `ids` the changes, saving them all in one write, then restarts the
    /// running streams of those that changed; returns how many changed. A camera the changes
    /// leave as it was is not written or restarted. A problem with the changes, an unknown camera
    /// or a failed write changes nothing. Restarting waits on streams, so the caller is off the
    /// main thread.
    pub(super) fn save_changes(
        &self,
        ids: &[String],
        changes: &CameraChanges,
    ) -> Result<u32, String> {
        let Some((change, changed)) = library::save_changes(&self.library, ids, changes)? else {
            return Ok(0);
        };
        self.library
            .apply(&change)
            // The error already names the file.
            .map_err(|error| format!("Could not save {error}"))?;
        for id in &changed {
            // A camera deleted meanwhile has nothing left to use the changes.
            let Ok(camera) = self.camera(id.as_str()) else {
                continue;
            };
            // What the file holds now, so a save of the camera alone that landed in between still
            // wins.
            let _ = camera.adopt(|| {
                self.library
                    .camera(id)
                    .ok_or_else(|| format!("no camera `{id}`"))
            });
        }
        Ok(changed.len() as u32)
    }

    /// Starts recording every camera in `ids` that is streaming, returning how many. The others are
    /// left out; only when none can record does this say why.
    ///
    /// A camera whose tiles show only another of its streams connects its own for the recording,
    /// which no tile shows: the screen is gone through again afterwards, so that stream decodes
    /// nothing but the picture each file starts on (the decode gate's), and one connected for a
    /// recording that couldn't start disconnects again. Starting streams waits on them, so the
    /// caller is off the main thread. Nothing starts once the media are at Settings →
    /// Automation's limit (`room_for_new`).
    pub(super) fn record(&self, ids: &[String]) -> Result<u32, String> {
        self.room_for_new()?;
        let mut started = 0;
        let mut first_problem = None;
        for id in ids {
            match self.camera(id).and_then(|camera| camera.start_recording()) {
                Ok(()) => started += 1,
                Err(problem) => {
                    first_problem.get_or_insert(problem);
                }
            }
        }
        self.reconcile();
        if started > 0 {
            Ok(started)
        } else {
            Err(first_problem
                .unwrap_or_else(|| "There is no camera on screen to record.".to_string()))
        }
    }

    /// Saves the frame on screen of every camera in `ids` that is streaming, as PNGs in the
    /// snapshots folder, returning how many, as recording does: the others are left out, and only
    /// when none can does this say why. Every frame is taken before any is written, so they show
    /// the same moment, and they are written several at a time. Writes files, so the caller is off
    /// the main thread. None is taken once the media are at Settings → Automation's limit
    /// (`room_for_new`).
    pub(super) fn snapshot(&self, ids: &[String]) -> Result<u32, String> {
        self.room_for_new()?;
        let mut first_problem = None;
        let pictures: Vec<(&str, Arc<DecodedFrame>)> = ids
            .iter()
            .filter_map(
                |id| match self.camera(id).and_then(|camera| camera.picture()) {
                    Ok(picture) => Some((id.as_str(), picture)),
                    Err(problem) => {
                        first_problem.get_or_insert(problem);
                        None
                    }
                },
            )
            .collect();

        // Snapshots belong with the app's other generated files, beside the executable, never in
        // the user's profile, which may be redirected, roamed or unavailable.
        let folder = self.config.snapshots_dir();
        let stamp = saved_file::now_seconds();
        // One encoder per core at most: each holds a copy of its picture while it works.
        let at_once = thread::available_parallelism().map_or(4, |cores| cores.get());
        let mut saved = 0;
        for batch in pictures.chunks(at_once) {
            let written: Vec<Result<PathBuf, String>> = thread::scope(|scope| {
                let writers: Vec<_> = batch
                    .iter()
                    .map(|(id, picture)| {
                        let folder = &folder;
                        scope.spawn(move || {
                            write_snapshot(folder, picture, stamp, id)
                                .map_err(|error| error.to_string())
                        })
                    })
                    .collect();
                writers
                    .into_iter()
                    .map(|writer| {
                        writer
                            .join()
                            .unwrap_or_else(|_| Err("the encoder stopped".to_string()))
                    })
                    .collect()
            });
            for result in written {
                match result {
                    Ok(_) => saved += 1,
                    Err(error) => {
                        first_problem.get_or_insert(format!("Snapshot failed: {error}"));
                    }
                }
            }
        }

        if saved > 0 {
            Ok(saved)
        } else {
            Err(first_problem.unwrap_or_else(|| {
                "There is no camera on screen to take a snapshot of.".to_string()
            }))
        }
    }

    pub(super) fn set_on_screen(
        &self,
        shown: Vec<StreamOf>,
        kept: Vec<StreamOf>,
        audible: Option<StreamOf>,
    ) {
        *self.screen.locked() = Screen {
            shown: shown.into_iter().collect(),
            kept: kept.into_iter().collect(),
            audible,
        };
    }

    /// The streams of camera `id` that a press of its Connect starts: those the screen shows or
    /// keeps, or its own when it shows and keeps none.
    pub(super) fn wanted_streams(&self, id: &str) -> Vec<StreamKey> {
        let Wanted { shown, kept, .. } = self.screen.locked().of(id);
        let streams: Vec<StreamKey> = shown.union(&kept).cloned().collect();
        if streams.is_empty() {
            vec![None]
        } else {
            streams
        }
    }

    /// Brings every camera in line with what is on screen. Starting and stopping wait on other
    /// threads, so this runs off the main thread.
    ///
    /// The screen is read again for each camera, after its transition lock is taken, so when two of
    /// these overlap the newer wishes always win.
    pub(super) fn reconcile(&self) {
        for camera in self.all() {
            camera.reconcile(|| self.screen.locked().of(&camera.id));
        }
    }
}
