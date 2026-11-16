//! The relay sessions, one per camera stream: started when a viewer comes, a snapshot is asked for
//! or a recording starts, and stopped when nothing has needed one for a while.
//!
//! Every viewer of a stream watches the same session, so a camera shown in several browser tabs is
//! still one RTSP connection. A session nobody watches stays up for [`LINGER`], so going from one
//! page of tiles to another doesn't reconnect a camera that is on both.
//!
//! **Streams.** A camera's session is of the stream its settings name, unless a tile picks another
//! (`ipcamera_core::TileStream`): that one is a session of its own, beside the camera's, found by
//! the camera and the stream ([`Key`]), with the camera's login and every rule here. Snapshots and
//! recording are of the camera's own stream; a camera's watch goes with whichever of its sessions
//! went live first.
//!
//! A session that gave up because the camera rejected the login, or sends a stream the server
//! can't play ([`MediaSession::refused`]), stays as it is, failure and all, until the camera's
//! settings change or someone presses Connect ([`Streams::try_again`]). Each new viewer is told
//! why at once, and the server never tries that login again by itself: failed logins can lock the
//! camera's account. A session that gave up for any other reason starts afresh for the next
//! viewer.
//!
//! **Snapshots.** A relay decodes nothing, so a snapshot decodes the newest picture from the
//! pictures the session keeps since the last keyframe ([`Feed::picture`]), and saves it as the
//! desktop does (`ipcamera_storage`). A camera nobody watches is connected for it, and lingers
//! after.
//!
//! **Recording.** The session records as the desktop's live view does, into the videos folder
//! `settings.json` names, each file with a thumbnail of the keyframe it starts on. A camera that
//! records keeps its session with nobody watching, until recording stops. One whose settings
//! change starts a new session that records on into a new file, as on the desktop. Recording is
//! one switch for all, as on the desktop: it starts camera by camera and stops for every camera
//! at once.
//!
//! **Watching.** A camera the app can watch (a Reolink with a password) gets a `CameraWatch` beside
//! its session, as on the desktop, started once the session is live: its login worked, so a wrong
//! password costs one failed login rather than two. It goes with the session.
//!
//! Nothing here tries a camera with no password by itself: a blank login fails, and failed logins
//! can lock the camera's account. [`Streams::open`] refuses one, for a viewer as for a snapshot or
//! a recording, until someone presses Connect for it, as on the desktop: a camera that really has
//! no password plays from then on, and one that has refuses the one login and stays refused.
//!
//! **Another program's changes.** The desktop app can share these files. A camera it changed or
//! deleted is restarted or stopped here as one changed through the server is
//! ([`Streams::follow`]), once the server has read the files again.

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

/// How long a session nobody watches stays up.
const LINGER: Duration = Duration::from_secs(30);

/// How long a snapshot waits for a camera's first picture.
const PICTURE_WITHIN: Duration = Duration::from_secs(10);

/// How long a recording's thumbnail waits for the keyframe the recording starts on.
const KEYFRAME_WITHIN: Duration = Duration::from_secs(10);

/// Why a camera with no saved password isn't connected to for a snapshot, a recording or PTZ.
pub(crate) const NO_PASSWORD: &str = "Add the camera's password in Settings first.";

/// Where a relay session is found: its camera, and which of its streams (`None` for its own).
pub(crate) type Key = (CameraId, Option<TileStream>);

/// One camera stream's relay session, and the settings it was started with: the camera's, with
/// the stream a tile picked in place of its own.
pub(crate) struct Stream {
    setting: CameraConfig,
    /// Stopped off the runtime's threads when the stream is dropped: stopping waits for the
    /// session's thread, which finishes a recording's file first.
    session: OffThread<MediaSession>,
    /// Since when nobody has watched it.
    idle_since: Mutex<Option<Instant>>,
    /// What the camera sees and its spotlight, once the session is live, for a camera the app
    /// can watch: in one of the camera's sessions only. Dropping it never waits.
    watch: Mutex<Option<CameraWatch>>,
}

impl Stream {
    fn session(&self) -> &MediaSession {
        &self.session
    }

    /// The camera's video, for a new viewer to watch.
    pub(crate) fn feed(&self) -> Feed {
        self.session().feed()
    }

    /// What the session is doing now.
    pub(crate) fn status(&self) -> MediaStatus {
        self.session().status()
    }

    fn watched(&self) -> bool {
        self.session().feed().viewers() > 0
    }

    /// The file it records into right now, if any.
    fn writing(&self) -> Option<PathBuf> {
        match self.session().record_status() {
            RecordStatus::Writing(path) => Some(path),
            _ => None,
        }
    }
}

/// One camera's recording, as the status reports it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Recording {
    /// Whether it records, or waits for its stream to.
    pub(crate) on: bool,
    /// Why its last recording stopped by itself, until the next one starts; `""` when none did.
    pub(crate) problem: String,
}

/// Every camera stream's relay session that is running, or that refused for good, and which
/// cameras record.
pub(crate) struct Streams {
    /// Where the recordings go: `settings.json`'s, read as each file opens.
    config: Arc<ConfigStore>,
    streams: Mutex<HashMap<Key, Arc<Stream>>>,
    /// The cameras asked to record, until recording stops or fails.
    recording: Mutex<HashSet<CameraId>>,
    /// Why a camera's recording stopped by itself, until it records again.
    problems: Mutex<HashMap<CameraId, String>>,
    /// The cameras with no saved password that someone pressed Connect for, with the settings
    /// they had then: the only ones [`open`](Self::open) tries with a blank login.
    asked_for: Mutex<HashMap<CameraId, CameraConfig>>,
    /// The sessions that are open or still stopping, for the server to wait for as it ends.
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

    /// The relay of `camera` on `picked`, a stream a tile picks, or its own for `None`: started if
    /// it isn't running, or started again if the camera's settings have changed or it gave up on
    /// something that may have cleared since. The error is a sentence for the viewer: the camera
    /// has no saved password, or its settings can't reach a camera.
    ///
    /// Every way to a camera's streams comes through here, a viewer's included, so this is where a
    /// camera with no saved password is refused, unless someone pressed Connect for it as it is
    /// now ([`try_again`](Self::try_again)): nothing can go around it.
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
        // A camera recording when its session stopped records on in the new one.
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

    /// Someone pressed Connect for `camera`: each session of it that gave up, for good or not, is
    /// forgotten, so its viewers, who see it is no longer [`current`](Self::current), connect
    /// again and start a new one; and a camera with no saved password may be tried as it is now.
    /// That is one login a viewer, as the desktop's Connect makes one a stream on screen, never one
    /// the server makes by itself.
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

    /// Each camera that has a session, and why one of its sessions gave up when one has: what the
    /// status tells the web build, which shows Connect for one that did.
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

    /// Saves `camera`'s newest picture in `folder` as a PNG, returning the file's name. It connects
    /// to a camera nobody watches, and waits for its first picture. It decodes and writes a file,
    /// so it runs off the runtime's threads.
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

    /// Starts recording `camera`'s own stream from its next keyframe on, connecting to it if
    /// nobody watches.
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

    /// Has `session` record camera `id` into the videos folder, each file named as it opens, with
    /// a thumbnail of its first picture.
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

    /// Stops every recording, returning how many cameras were recording. Their files finish at
    /// the next packet. The sessions they kept up linger, as an unwatched one does.
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

    /// The names of the files being recorded right now.
    pub(crate) fn writing(&self) -> HashSet<String> {
        self.streams
            .locked()
            .values()
            .filter_map(|stream| stream.writing())
            .map(|path| file_name(&path))
            .collect()
    }

    /// Each camera's recording that records, or stopped by itself: one that failed to write its
    /// file stops recording, and says why until it records again.
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

    /// What each watched camera's watch has learned, starting the watch of each camera that has
    /// none and a session that has gone live since the last call. One watch a camera, however
    /// many of its streams run. The status asks about once a second.
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

    /// Asks camera `id`'s watch to switch its spotlight; the answer comes on the returned channel,
    /// to wait for off the runtime's threads.
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

    /// Stops the sessions nobody has watched for [`LINGER`] and that don't record, and those of
    /// cameras that are gone. One that refused for good stays, to tell the next viewer why.
    pub(crate) fn sweep(&self, library: &Library) {
        let now = Instant::now();
        // The streams before the recordings, as everywhere here, so two callers never wait on each
        // other.
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

    /// Stops camera `id`'s sessions, after its settings changed (`camera` is them now) or it was
    /// deleted (`None`). Their viewers see that they are no longer [`current`](Self::current) and
    /// reconnect, which starts them again as the camera is now. A camera that records starts its
    /// own stream again at once, recording into a new file; a deleted one stops recording.
    pub(crate) fn restart(&self, id: &CameraId, camera: Option<&CameraConfig>) {
        self.streams.locked().retain(|(camera, _), _| camera != id);
        let records = self.recording.locked().contains(id);
        match camera {
            Some(camera) if records => {
                // A camera that can't start again says why where its recording is shown.
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

    /// Whether `stream` is still the session at `key`, rather than one stopped since.
    pub(crate) fn current(&self, key: &Key, stream: &Arc<Stream>) -> bool {
        self.streams
            .locked()
            .get(key)
            .is_some_and(|now| Arc::ptr_eq(now, stream))
    }

    /// Stops the sessions of every camera whose settings are no longer what one was started with,
    /// or that is gone: what [`restart`](Self::restart) does for a change made here, for one
    /// another program made to the files.
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

    /// Stops every session, as the server ends. Each stops on a thread of its own, once its last
    /// viewer has let go, finishing any recording: [`stopped_by`](Self::stopped_by) waits for them.
    pub(crate) fn stop_all(&self) {
        self.recording.locked().clear();
        self.streams.locked().clear();
    }

    /// Waits until every session has stopped, its recording's file finished, or `until`. Returns
    /// whether they all have. It blocks, so it runs off the runtime's threads.
    pub(crate) fn stopped_by(&self, until: Instant) -> bool {
        self.closing.wait(until)
    }
}

/// The newest picture `stream` has, once one has come: up to [`PICTURE_WITHIN`] for a session
/// just started. One that gave up says why.
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

/// Writes the thumbnail of the recording at `video` from the keyframe it starts on, on a thread
/// of its own, so the media thread that opens the file is never held up. The file opens as that
/// keyframe passes the record tap, before the feed has it, so this waits for the feed's next
/// keyframe. Nothing is written if none comes.
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
