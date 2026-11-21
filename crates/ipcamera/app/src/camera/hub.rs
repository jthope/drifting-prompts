//! Every camera the app holds, and which of them stream: [`Hub`], the one the app runs on
//! ([`HUB`]). Its threads follow what `ipcamera-server` saves in the shared files, keep the
//! cameras' watches going and log their events, and keep the media within their limits.

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

#[derive(Clone, Debug, PartialEq, Eq)]
struct WatchedAs {
    window_ms: u64,
    videos: bool,
    event_video: EventVideo,
}

fn watched_as(automation: &Automation) -> Option<WatchedAs> {
    automation.log_events.then(|| WatchedAs {
        window_ms: automation.event_window_ms(),
        videos: automation.event_capture.video(),
        event_video: automation.event_video.clone(),
    })
}

/// How `camera`'s watch groups what it sees into events; `None` while its events aren't logged.
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

fn stream_key(stream: &TileStream) -> StreamKey {
    (*stream != TileStream::Own).then(|| stream.clone())
}

struct OpenVideo {
    event: String,
    tags: Vec<String>,
}

pub(super) const SOURCE_VARIABLE: &str = "IPCAMERA_SOURCE";

/// The headless test runner gets throwaway settings: beside the development host it would find the
/// saved password and connect to the real camera.
pub(super) static HUB: LazyLock<Hub> = LazyLock::new(|| {
    if dp_ui::is_test_runner() {
        Hub::new(Arc::new(throwaway_settings()), Source::Offline)
    } else {
        let config = Arc::new(match dp_ui::development_folder() {
            Some(folder) => ConfigStore::in_folder(&folder),
            None => ConfigStore::beside_the_executable(),
        });
        follow_other_programs();
        watch_cameras_always();
        Hub::new(config, Source::from_env())
    }
});

const FOLLOW_EVERY: Duration = Duration::from_secs(2);

fn follow_other_programs() {
    let _ = thread::Builder::new()
        .name("ipcamera-settings".into())
        .spawn(|| {
            loop {
                // Sleep first: this starts from inside `HUB`'s initializer.
                thread::sleep(FOLLOW_EVERY);
                HUB.follow();
            }
        });
}

const WATCH_EVERY: Duration = Duration::from_secs(1);

fn watch_cameras_always() {
    let _ = thread::Builder::new()
        .name("ipcamera-events".into())
        .spawn(|| {
            loop {
                // Sleep first: this starts from inside `HUB`'s initializer.
                thread::sleep(WATCH_EVERY);
                HUB.watch_cameras();
                HUB.keep_within_limits();
            }
        });
}

pub(super) fn throwaway_settings() -> ConfigStore {
    let folder = std::env::temp_dir().join("ipcamera-nsh-tests");
    let _ = std::fs::remove_dir_all(&folder);
    let config = ConfigStore::load(folder.join("settings.json"));
    seed_test_camera(&config);
    config
}

/// No password, and an address in 192.0.2.0/24, which no device answers on. Its name sorts before
/// "Deleted camera", which the TSX tests rely on.
pub(super) fn test_camera() -> CameraConfig {
    CameraConfig {
        name: "Camera 1".into(),
        host: "192.0.2.10".into(),
        user: "admin".into(),
        ..CameraConfig::default()
    }
}

pub(super) fn seed_test_camera(config: &ConfigStore) {
    let library = Library::beside(config);
    let Ok(id) = library.add_camera(test_camera(), None) else {
        return;
    };
    if let Some(view) = library.views().first() {
        let _ = library.show_in_tile(&view.id, 0, Some(&id.into()));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    Stream,
    Pattern,
    /// Nothing can reach a camera, even if a test presses Connect.
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct StreamOf {
    pub(super) camera: String,
    pub(super) stream: StreamKey,
}

impl StreamOf {
    pub(super) fn own(camera: &str) -> Self {
        Self {
            camera: camera.to_string(),
            stream: None,
        }
    }
}

#[derive(Default)]
pub(super) struct Screen {
    pub(super) shown: HashSet<StreamOf>,
    pub(super) kept: HashSet<StreamOf>,
    pub(super) audible: Option<StreamOf>,
}

impl Screen {
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Wanted {
    pub(super) shown: HashSet<StreamKey>,
    pub(super) kept: HashSet<StreamKey>,
    pub(super) audible: Option<StreamKey>,
}

pub(super) struct Hub {
    pub(super) config: Arc<ConfigStore>,
    pub(super) library: Library,
    pub(super) source: Source,
    pub(super) cameras: Mutex<Vec<Arc<Camera>>>,
    pub(super) screen: Mutex<Screen>,
    pub(super) events: EventLog,
    watching_as: Mutex<Option<WatchedAs>>,
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
        // An event left open by the last run ends where it started: its real end is unknown.
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

    /// Keeps every camera's watch going and acts on what they saw since the last call; reads and
    /// writes files.
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

    /// Renames each event video that closed to carry its tags; one that can't be renamed keeps it.
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

    fn stop_recording_by_itself(&self, camera: &Camera) {
        if camera.is_recording_by_itself() {
            camera.stop_recording();
            camera.reconcile(|| self.screen.locked().of(&camera.id));
        }
    }

    /// Logs what `camera`'s watch saw and saves what Automation asks; waits on the camera and disk.
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
                // Recording first, so the snapshot can come from the stream it connects.
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
            EventStep::Added if records && !camera.is_recording() => {
                self.record_sighting(camera, &id, &tags, &video);
            }
            EventStep::Quiet | EventStep::Closed => self.stop_recording_by_itself(camera),
            EventStep::SeenAgain | EventStep::Added => {}
        }
    }

    /// The event's snapshot: the camera's own `still`, else the picture on screen, else one from
    /// its stream; never for a camera the user disconnected or one with no password.
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

    /// Records a sighting of event `id` as `video` says. A camera the user disconnected, one with
    /// no password and one already recording are left as they are.
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
        // A stream connected for a recording that couldn't start disconnects again.
        camera.reconcile(|| self.screen.locked().of(&camera.id));
    }

    /// Deletes for good what `events` saved, never a file a camera is still writing.
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

    fn busy_files(&self) -> Vec<PathBuf> {
        self.all()
            .iter()
            .filter_map(|camera| camera.recording_file())
            .map(|file| saved_file::absolute(&file))
            .collect()
    }

    pub(super) fn media_size(&self) -> u64 {
        let (videos, snapshots) = self.media_sizes();
        videos + snapshots
    }

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

    /// Whether the media limit leaves room for a new snapshot or recording, deleting the oldest
    /// where allowed; if not, why, as a sentence.
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

    /// Once the media reach their limit while anything records, deletes the oldest where allowed or
    /// else stops every recording.
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

    /// Every camera, copied out so the list isn't held while they are asked anything.
    pub(super) fn all(&self) -> Vec<Arc<Camera>> {
        self.cameras.locked().clone()
    }

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

    /// Takes what another program saved in `data\settings` since the app last read or wrote it.
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

    pub(super) fn save(&self, id: &str, form: &CameraForm) -> Result<(), String> {
        let camera = self.camera(id)?;
        let camera_id = CameraId::from(id);
        camera.save(form::to_config(form), |setting| {
            // The desktop's form holds the password itself, so it is saved as it is.
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
            // What the file holds now, so a save of this camera alone in between still wins.
            let _ = camera.adopt(|| {
                self.library
                    .camera(id)
                    .ok_or_else(|| format!("no camera `{id}`"))
            });
        }
        Ok(changed.len() as u32)
    }

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

    /// Every frame is taken before any is written, so the snapshots show the same moment.
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

    pub(super) fn wanted_streams(&self, id: &str) -> Vec<StreamKey> {
        let Wanted { shown, kept, .. } = self.screen.locked().of(id);
        let streams: Vec<StreamKey> = shown.union(&kept).cloned().collect();
        if streams.is_empty() {
            vec![None]
        } else {
            streams
        }
    }

    /// Brings every camera in line with the screen, read after each camera's `transition` is taken,
    /// so the newest wishes win.
    pub(super) fn reconcile(&self) {
        for camera in self.all() {
            camera.reconcile(|| self.screen.locked().of(&camera.id));
        }
    }
}
