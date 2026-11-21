//! The relay sessions, one per camera stream, shared by its viewers and stopped once nothing has
//! needed one for [`LINGER`].
//!
//! A refused login is never retried, and a camera with no saved password never tried, until
//! someone presses Connect: failed logins can lock a camera's account.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use ipcamera_core::{
    CameraConfig, CameraId, CameraWatch, Library, TileStream, Watched, config::ConfigStore,
    sync::LockExt,
};
use ipcamera_media::{DecodedFrame, Feed, MediaSession, MediaStatus, RecordRules, RecordStatus};
use ipcamera_storage::{
    recording,
    saved_file::{now_seconds, unix_ms},
    snapshot, thumbnail,
};

use crate::closing::{Closing, OffThread};

const LINGER: Duration = Duration::from_secs(30);

const PICTURE_WITHIN: Duration = Duration::from_secs(10);

const KEYFRAME_WITHIN: Duration = Duration::from_secs(10);

pub(crate) const NO_PASSWORD: &str = "Add the camera's password in Settings first.";

/// A camera and which of its streams, `None` for its own.
pub(crate) type Key = (CameraId, Option<TileStream>);

pub(crate) struct Stream {
    setting: CameraConfig,
    session: OffThread<MediaSession>,
    idle_since: Mutex<Option<Instant>>,
    /// In one of the camera's sessions only; dropping it never waits.
    watch: Mutex<Option<CameraWatch>>,
}

impl Stream {
    fn session(&self) -> &MediaSession {
        &self.session
    }

    pub(crate) fn feed(&self) -> Feed {
        self.session().feed()
    }

    pub(crate) fn status(&self) -> MediaStatus {
        self.session().status()
    }

    fn watched(&self) -> bool {
        self.session().feed().viewers() > 0
    }

    fn writing(&self) -> Option<PathBuf> {
        match self.session().record_status() {
            RecordStatus::Writing(path) => Some(path),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Recording {
    pub(crate) on: bool,
    pub(crate) problem: String,
}

pub(crate) struct Streams {
    config: Arc<ConfigStore>,
    streams: Mutex<HashMap<Key, Arc<Stream>>>,
    recording: Mutex<HashSet<CameraId>>,
    problems: Mutex<HashMap<CameraId, String>>,
    /// Cameras with no saved password someone pressed Connect for, as they were then: the only
    /// blank logins ever tried.
    asked_for: Mutex<HashMap<CameraId, CameraConfig>>,
    closing: Arc<Closing>,
}

impl Streams {
    pub(crate) fn new(config: Arc<ConfigStore>) -> Self {
        Self {
            config,
            streams: Mutex::default(),
            recording: Mutex::default(),
            problems: Mutex::default(),
            asked_for: Mutex::default(),
            closing: Arc::default(),
        }
    }

    /// Every way to a camera's streams comes through here, so this is where a camera with no saved
    /// password is refused, unless Connect was pressed for it as it is now.
    pub(crate) fn open(
        &self,
        camera: &CameraConfig,
        picked: Option<&TileStream>,
    ) -> Result<Arc<Stream>, String> {
        if camera.password.is_empty() && self.asked_for.locked().get(&camera.id) != Some(camera) {
            return Err(NO_PASSWORD.to_string());
        }
        let setting = camera.with_stream(picked);
        let key = (camera.id.clone(), picked.cloned());
        let mut streams = self.streams.locked();
        if let Some(stream) = streams.get(&key)
            && stream.setting == setting
        {
            let gave_up = matches!(stream.status(), MediaStatus::Failed(_));
            if !gave_up || stream.session().refused() {
                // Its viewer is about to watch: no sweep may count it as idle meanwhile.
                *stream.idle_since.locked() = None;
                return Ok(stream.clone());
            }
        }
        let session = MediaSession::relay(&setting).map_err(|error| format!("{error:#}"))?;
        if picked.is_none() && self.recording.locked().contains(&camera.id) {
            self.record_into(&session, &camera.id);
        }
        let stream = Arc::new(Stream {
            setting,
            session: self.closing.hold(session),
            idle_since: Mutex::new(None),
            watch: Mutex::new(None),
        });
        // One it replaces stops as its last viewer lets go.
        streams.insert(key, stream.clone());
        Ok(stream)
    }

    /// Forgets `camera`'s sessions that gave up, so viewers reconnect, and lets one with no saved
    /// password be tried as it is now.
    pub(crate) fn try_again(&self, camera: &CameraConfig) {
        if camera.password.is_empty() {
            self.asked_for
                .locked()
                .insert(camera.id.clone(), camera.clone());
        }
        self.streams.locked().retain(|(id, _), stream| {
            *id != camera.id || !matches!(stream.status(), MediaStatus::Failed(_))
        });
    }

    pub(crate) fn states(&self) -> Vec<(CameraId, Option<String>)> {
        let mut states: HashMap<CameraId, Option<String>> = HashMap::new();
        for ((id, _), stream) in self.streams.locked().iter() {
            let problem = states.entry(id.clone()).or_default();
            if let (None, MediaStatus::Failed(why)) = (&problem, stream.status()) {
                *problem = Some(why);
            }
        }
        states.into_iter().collect()
    }

    /// Blocks while it connects and decodes, so never on the runtime's threads.
    pub(crate) fn snapshot(
        &self,
        camera: &CameraConfig,
        folder: PathBuf,
    ) -> Result<String, String> {
        let stream = self.open(camera, None)?;
        let picture = newest_picture(&stream)?;
        let path = snapshot::write_snapshot(&folder, &picture, now_seconds(), camera.id.as_str())
            .map_err(|error| format!("Snapshot failed: {error}"))?;
        Ok(file_name(&path))
    }

    pub(crate) fn record(&self, camera: &CameraConfig) -> Result<(), String> {
        let stream = self.open(camera, None)?;
        if let MediaStatus::Failed(why) = stream.status() {
            return Err(format!("{} can't record: {why}", camera.name));
        }
        self.problems.locked().remove(&camera.id);
        self.recording.locked().insert(camera.id.clone());
        self.record_into(stream.session(), &camera.id);
        Ok(())
    }

    fn record_into(&self, session: &MediaSession, id: &CameraId) {
        let config = self.config.clone();
        let feed = session.feed();
        let id = id.to_string();
        session.start_recording(RecordRules::default(), move |started| {
            let stamp = unix_ms(Ok(started)) / 1_000;
            let path = recording::file_for(&config.videos_dir(), stamp, &id, &[]);
            write_thumbnail_soon(path.clone(), feed.clone());
            path
        });
    }

    pub(crate) fn stop_recording(&self) -> u32 {
        let stopped: Vec<CameraId> = self.recording.locked().drain().collect();
        let streams = self.streams.locked();
        for id in stopped.iter().cloned() {
            if let Some(stream) = streams.get(&(id, None)) {
                stream.session().stop_recording();
            }
        }
        stopped.len() as u32
    }

    pub(crate) fn writing(&self) -> HashSet<String> {
        self.streams
            .locked()
            .values()
            .filter_map(|stream| stream.writing())
            .map(|path| file_name(&path))
            .collect()
    }

    pub(crate) fn recordings(&self) -> HashMap<CameraId, Recording> {
        let streams = self.streams.locked();
        let mut recording = self.recording.locked();
        let mut problems = self.problems.locked();
        recording.retain(|id| {
            let failed = streams
                .get(&(id.clone(), None))
                .and_then(|stream| stream.session().record_status().problem());
            if let Some(problem) = failed {
                problems.insert(id.clone(), problem);
            }
            !problems.contains_key(id)
        });
        let mut all: HashMap<CameraId, Recording> = problems
            .iter()
            .map(|(id, problem)| {
                (
                    id.clone(),
                    Recording {
                        on: false,
                        problem: problem.clone(),
                    },
                )
            })
            .collect();
        for id in recording.iter() {
            all.insert(
                id.clone(),
                Recording {
                    on: true,
                    problem: String::new(),
                },
            );
        }
        all
    }

    /// Starts a camera's watch once a session is live, so a wrong password costs one login.
    pub(crate) fn watched(&self) -> Vec<(CameraId, Watched)> {
        let streams = self.streams.locked();
        let mut watched: HashMap<CameraId, Watched> = HashMap::new();
        for ((id, _), stream) in streams.iter() {
            if let Some(watch) = stream.watch.locked().as_ref() {
                watched.entry(id.clone()).or_insert_with(|| watch.watched());
            }
        }
        for ((id, _), stream) in streams.iter() {
            if watched.contains_key(id) || !matches!(stream.status(), MediaStatus::Live) {
                continue;
            }
            let mut watch = stream.watch.locked();
            *watch = CameraWatch::start(&stream.setting);
            if let Some(watch) = watch.as_ref() {
                watched.insert(id.clone(), watch.watched());
            }
        }
        watched.into_iter().collect()
    }

    /// Wait for the answer off the runtime's threads.
    pub(crate) fn set_spotlight(
        &self,
        id: &CameraId,
        on: bool,
    ) -> Result<mpsc::Receiver<Result<(), String>>, String> {
        let streams = self.streams.locked();
        let watch = streams
            .iter()
            .filter(|((camera, _), _)| camera == id)
            .map(|(_, stream)| stream.watch.locked())
            .find(|watch| watch.is_some());
        let Some(watch) = watch.as_ref().and_then(|watch| watch.as_ref()) else {
            return Err("Show the camera first, then switch its spotlight.".to_string());
        };
        watch.switch_spotlight(on)
    }

    /// A session that refused for good stays, to tell the next viewer why.
    pub(crate) fn sweep(&self, library: &Library) {
        let now = Instant::now();
        // The streams' lock before the recordings', as everywhere here, or two callers deadlock.
        let mut streams = self.streams.locked();
        let mut recording = self.recording.locked();
        recording.retain(|id| library.camera(id).is_some());
        // A press of Connect holds for the settings it was pressed for, and no others.
        self.asked_for
            .locked()
            .retain(|id, asked| library.camera(id).as_ref() == Some(asked));
        streams.retain(|(id, picked), stream| {
            if library.camera(id).is_none() {
                return false;
            }
            let mut idle_since = stream.idle_since.locked();
            if stream.watched() || (picked.is_none() && recording.contains(id)) {
                *idle_since = None;
                return true;
            }
            if stream.session().refused() {
                return true;
            }
            now.duration_since(*idle_since.get_or_insert(now)) < LINGER
        });
    }

    pub(crate) fn restart(&self, id: &CameraId, camera: Option<&CameraConfig>) {
        self.streams.locked().retain(|(camera, _), _| camera != id);
        let records = self.recording.locked().contains(id);
        match camera {
            Some(camera) if records => {
                if let Err(why) = self.open(camera, None) {
                    self.recording.locked().remove(id);
                    self.problems
                        .locked()
                        .insert(id.clone(), format!("Recording stopped: {why}"));
                }
            }
            Some(_) => {}
            None => {
                self.recording.locked().remove(id);
                self.problems.locked().remove(id);
            }
        }
    }

    pub(crate) fn current(&self, key: &Key, stream: &Arc<Stream>) -> bool {
        self.streams
            .locked()
            .get(key)
            .is_some_and(|now| Arc::ptr_eq(now, stream))
    }

    pub(crate) fn follow(&self, library: &Library) {
        let changed: HashSet<CameraId> = self
            .streams
            .locked()
            .iter()
            .filter(|((id, picked), stream)| {
                library
                    .camera(id)
                    .map(|camera| camera.with_stream(picked.as_ref()))
                    .as_ref()
                    != Some(&stream.setting)
            })
            .map(|((id, _), _)| id.clone())
            .collect();
        for id in changed {
            self.restart(&id, library.camera(&id).as_ref());
        }
    }

    pub(crate) fn stop_all(&self) {
        self.recording.locked().clear();
        self.streams.locked().clear();
    }

    /// Blocks, so never on the runtime's threads.
    pub(crate) fn stopped_by(&self, until: Instant) -> bool {
        self.closing.wait(until)
    }
}

fn newest_picture(stream: &Stream) -> Result<DecodedFrame, String> {
    let deadline = Instant::now() + PICTURE_WITHIN;
    loop {
        if let MediaStatus::Failed(why) = stream.status() {
            return Err(format!("Snapshot failed: {why}"));
        }
        match stream.feed().picture() {
            Ok(picture) => return Ok(picture),
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Err(why) => {
                return Err(format!(
                    "The camera sent no picture within {} s ({why:#}).",
                    PICTURE_WITHIN.as_secs()
                ));
            }
        }
    }
}

/// The file opens as its keyframe passes the record tap, before the feed has it, so this waits for
/// the feed's next keyframe, on a thread of its own.
fn write_thumbnail_soon(video: PathBuf, feed: Feed) {
    let before = feed.keyframes();
    let _ = thread::Builder::new()
        .name("ipcamera-thumbnail".into())
        .spawn(move || {
            let deadline = Instant::now() + KEYFRAME_WITHIN;
            while feed.keyframes() == before {
                if Instant::now() >= deadline {
                    return;
                }
                thread::sleep(Duration::from_millis(20));
            }
            if let Ok(picture) = feed.keyframe_picture() {
                let _ = thumbnail::write(&video, &picture);
            }
        });
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
