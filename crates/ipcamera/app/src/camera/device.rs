//! One camera: its settings, its streams while they are connected, its PTZ, sound and recording.
//!
//! A camera has its own stream, the one its settings name, and any other a tile picks instead
//! (`ipcamera_core::TileStream`), each found by its [`StreamKey`]. Each stream has an [`Output`],
//! where its pictures go for the window, and a feed of its own while it runs. The rest is the
//! camera's, once whatever streams it shows: its settings, PTZ, the watch of what it sees, the
//! sound's level, and recording, which is always of its own stream.
//!
//! While events are logged (Settings → Automation, on by default) the watch follows the camera,
//! not its video: once started it runs, connected or not, until the camera's settings change or it
//! goes ([`Camera::tend_watch`]), so its events are logged with nothing on screen. With events off
//! it runs only alongside a stream, for the badge and the spotlight. A camera whose stream or watch had its login refused isn't logged into by
//! itself again until its settings change.
//!
//! The rule for the locks: nothing slow runs while `inner`, `outputs` or an output's frames are
//! held, because the window reads them while it draws. Starting and stopping a pipeline wait on
//! another thread, so those take `transition` instead, which only other transitions wait for. A
//! thread that holds `inner` may take `outputs` or an output's frames, never the other way round.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use ipcamera_core::{
    CameraConfig, CameraWatch, EventRules, SeenEvent, TileStream, Watched,
    config::ConfigStore,
    merged_tags,
    ptz::{PtzSession, PtzVelocity},
    sync::LockExt,
};
use ipcamera_media::{DecodedFrame, FrameSlot, MediaStatus, RecordRules, RecordStatus};

use super::{
    feed::Feed,
    hub::{Source, Wanted},
    stats::StreamStats,
    status,
};
use crate::{
    bridge::{CameraForm, CameraState, CameraSummary, RunningStream, StatusTone},
    form,
};

/// The slider is 0..100; the mixer wants a gain. Squaring it makes the slider feel linear to the
/// ear. A camera that is not selected is silent.
pub(super) fn audio_gain(muted: bool, volume: f32, audible: bool) -> f64 {
    if muted || !audible {
        0.0
    } else {
        f64::from(volume / 100.0).powi(2)
    }
}

/// How long releasing a camera's PTZ waits for its last stop to go out. A camera on the network
/// takes a stop in a few milliseconds; this is the most the window ever waits for one that doesn't.
const PTZ_RELEASE_WITHIN: Duration = Duration::from_millis(500);

/// Which of a camera's streams: `None` for the one its settings name, or one a tile picks.
pub type StreamKey = Option<TileStream>;

/// What one of a camera's streams is doing, behind `Camera::inner`.
#[derive(Default)]
pub(super) struct Run {
    pub(super) feed: Option<Feed>,
    /// Why its last connection never started, until the next try.
    pub(super) start_error: Option<String>,
    pub(super) stats: StreamStats,
}

/// What a camera holds behind `Camera::inner`.
pub(super) struct Inner {
    pub(super) setting: CameraConfig,
    /// Each stream that runs, or that could not start, by stream.
    pub(super) runs: HashMap<StreamKey, Run>,
    /// Opened on first use, so a camera that offers no PTZ is not logged into for nothing.
    pub(super) ptz: Option<PtzSession>,
    /// What the camera sees and its spotlight, for a camera the app can watch
    /// (`ipcamera_core::watches`): from when it may first log in until its settings change.
    pub(super) watch: Option<CameraWatch>,
    pub(super) muted: bool,
    pub(super) volume: f32,
    /// The stream of this camera that the selected tile shows, the only one heard; `None` when
    /// the selected tile shows another camera.
    pub(super) audible: Option<StreamKey>,
    /// Why the last recording stopped by itself, until the next one starts.
    pub(super) record_problem: Option<String>,
}

impl Inner {
    /// The gain of the camera's own stream.
    #[cfg(test)]
    pub(super) fn gain(&self) -> f64 {
        self.gain_of(&None)
    }

    /// The gain of stream `key`: the selected tile's at the camera's level, any other silent.
    pub(super) fn gain_of(&self, key: &StreamKey) -> f64 {
        audio_gain(self.muted, self.volume, self.audible.as_ref() == Some(key))
    }

    /// Stream `key`'s feed, while it runs.
    pub(super) fn feed(&self, key: &StreamKey) -> Option<&Feed> {
        self.runs.get(key)?.feed.as_ref()
    }

    /// The streams that run.
    fn running(&self) -> impl Iterator<Item = (&StreamKey, &Feed)> {
        self.runs
            .iter()
            .filter_map(|(key, run)| run.feed.as_ref().map(|feed| (key, feed)))
    }

    /// The stream whose status and stats stand for the camera: the selected tile's while it
    /// runs, else the camera's own, else any that runs.
    fn focus(&self) -> Option<StreamKey> {
        self.audible
            .iter()
            .chain([&None])
            .find(|key| self.feed(key).is_some())
            .cloned()
            .or_else(|| self.running().map(|(key, _)| key.clone()).next())
    }

    /// Why a stream could not start: the selected tile's, else the camera's own, else any's.
    fn start_error(&self) -> Option<&String> {
        let error = |key: &StreamKey| self.runs.get(key)?.start_error.as_ref();
        self.audible
            .as_ref()
            .and_then(error)
            .or_else(|| error(&None))
            .or_else(|| self.runs.values().find_map(|run| run.start_error.as_ref()))
    }
}

/// Where one of a camera's streams puts its pictures, for the window. A camera keeps each for as
/// long as it is held, so a tile showing one keeps it across reconnects: a new session puts a new
/// slot in it.
pub struct Output {
    /// The newest frames, read by the window on every draw.
    frames: Mutex<FrameSlot>,
    /// Distinct frames that reached the screen, for the dropped count.
    painted: AtomicU64,
    /// Whether a tile shows it, as TSX last said; until TSX says, it counts as shown, so a stream
    /// connected before then decodes as it always has. A feed takes it when it starts.
    shown: AtomicBool,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            frames: Mutex::default(),
            painted: AtomicU64::new(0),
            shown: AtomicBool::new(true),
        }
    }
}

impl Output {
    /// The slot holding the newest frame.
    pub fn frames(&self) -> FrameSlot {
        self.frames.locked().clone()
    }

    /// Counts a new picture reaching the screen.
    pub fn count_painted(&self) {
        self.painted.fetch_add(1, Ordering::Relaxed);
    }
}

/// How long an event's snapshot waits for a picture from a stream no tile shows: its connection,
/// then its next keyframe, which some cameras send only every few seconds.
const EVENT_PICTURE_WAIT: Duration = Duration::from_secs(12);

/// One camera.
pub struct Camera {
    pub(super) id: String,
    pub(super) source: Source,
    pub(super) config: Arc<ConfigStore>,
    pub(super) inner: Mutex<Inner>,
    /// Where each of its streams puts its pictures. One is added the first time a tile or the
    /// screen asks for it, and stays.
    outputs: Mutex<HashMap<StreamKey, Arc<Output>>>,
    /// Held while a stream starts or stops, so those never overlap.
    pub(super) transition: Mutex<()>,
    /// Set when the user disconnects, so coming on screen doesn't connect it again behind their
    /// back. Connecting clears it.
    pub(super) held: AtomicBool,
    /// Whether the user asked this camera to record. It outlives the feed, so a stream restarted
    /// by a settings change records again.
    pub(super) recording: AtomicBool,
    /// Whether that recording started by itself, as the camera started seeing something (Settings
    /// → Automation), so it stops as the sighting ends. One the user started never does.
    pub(super) recording_by_itself: AtomicBool,
    /// The event a recording that started by itself is for.
    recording_for: Mutex<Option<String>>,
    /// What the recording's files are tagged with as each opens: what the camera saw since an
    /// event's recording started, growing as it sees more; none for the user's. Each file's namer
    /// reads it.
    recording_tags: Arc<Mutex<Vec<String>>>,
    /// Which of its streams records: its own for the user's recording, the one an event's video
    /// settings name for an event's.
    recording_key: Mutex<StreamKey>,
    /// How the recording goes: from the pre-roll or not, and when a file goes on in the next.
    recording_rules: Mutex<RecordRules>,
    /// The stream kept connected, though no tile shows it, to hold back the seconds before an
    /// event for its video, and how long: while events record videos with a pre-roll.
    armed: Mutex<Option<(StreamKey, Duration)>>,
    /// Set when the camera refused a stream's login, so the watch doesn't log in by itself with
    /// the same password; a settings change clears it. (A refused watch says so itself.)
    gave_up: AtomicBool,
}

impl Camera {
    pub(super) fn new(setting: CameraConfig, config: Arc<ConfigStore>, source: Source) -> Self {
        let ui = config.current().ui;
        Self {
            id: setting.id.to_string(),
            source,
            inner: Mutex::new(Inner {
                setting,
                runs: HashMap::new(),
                ptz: None,
                watch: None,
                muted: ui.muted,
                volume: ui.volume,
                audible: Some(None),
                record_problem: None,
            }),
            config,
            outputs: Mutex::default(),
            transition: Mutex::new(()),
            held: AtomicBool::new(false),
            recording: AtomicBool::new(false),
            recording_by_itself: AtomicBool::new(false),
            recording_for: Mutex::new(None),
            recording_tags: Arc::default(),
            recording_key: Mutex::new(None),
            recording_rules: Mutex::default(),
            armed: Mutex::new(None),
            gave_up: AtomicBool::new(false),
        }
    }

    /// Where stream `key` puts its pictures.
    pub fn output(&self, key: &StreamKey) -> Arc<Output> {
        self.outputs
            .locked()
            .entry(key.clone())
            .or_default()
            .clone()
    }

    /// The slot holding the newest frame of the camera's own stream.
    #[cfg(test)]
    pub fn frames(&self) -> FrameSlot {
        self.output(&None).frames()
    }

    pub(super) fn summary(&self) -> CameraSummary {
        let inner = self.inner.locked();
        CameraSummary {
            id: self.id.clone(),
            name: inner.setting.name.clone(),
            stream_label: inner.setting.stream_label(),
        }
    }

    /// Whether this camera may connect by itself.
    ///
    /// Without a password the login would fail and `MediaSession` would keep retrying, and repeated
    /// failed logins can lock a camera's account. Pressing Connect still tries, so a camera that
    /// really has no password works.
    ///
    /// A camera whose watch's login was refused doesn't either: the same password would only be
    /// refused again, and each refusal brings the account closer to being locked.
    pub(super) fn ready(&self) -> bool {
        let inner = self.inner.locked();
        self.source == Source::Pattern
            || (!inner.setting.password.is_empty() && refused_watch(&inner).is_none())
    }

    /// Whether the user disconnected the camera, which holds until they connect it.
    pub(super) fn held(&self) -> bool {
        self.held.load(Ordering::SeqCst)
    }

    /// Starts the camera's watch, for a camera the app can watch that has none, once it may log
    /// in: when a stream of its is live, whose login worked, or, while events are logged, at once
    /// when no stream is on its way, so the camera isn't sent two logins together. `stream_coming`
    /// says the screen is about to connect one. `events` is set while events are logged, grouped
    /// by those rules; without it the camera is watched only alongside a stream, as it was before
    /// events, and keeps none of them. Never for the test picture, which would log in to the real
    /// camera, nor for a camera that refused a stream's login. A stream that failed for another
    /// reason (a path the camera doesn't have, video the app can't play) doesn't hold the watch
    /// back. Returns whether it started one.
    pub(super) fn tend_watch(&self, stream_coming: bool, events: Option<EventRules>) -> bool {
        let log_events = events.is_some();
        if self.source != Source::Stream {
            return false;
        }
        let mut inner = self.inner.locked();
        if !log_events && inner.running().next().is_none() {
            // Dropping a watch never waits: its thread ends at its next poll.
            inner.watch = None;
            return false;
        }
        if inner.watch.is_some() || self.gave_up.load(Ordering::SeqCst) {
            return false;
        }
        let mut live = false;
        let mut on_its_way = stream_coming;
        for (_, feed) in inner.running() {
            match feed.media_status() {
                MediaStatus::Live => live = true,
                status if status.login_rejected() => self.gave_up.store(true, Ordering::SeqCst),
                MediaStatus::Failed(_) => {}
                _ => on_its_way = true,
            }
        }
        let may_log_in = live || (log_events && !on_its_way);
        if self.gave_up.load(Ordering::SeqCst) || !may_log_in {
            return false;
        }
        // Starting one only spawns its thread.
        inner.watch = match events {
            Some(rules) => CameraWatch::start_logging(&inner.setting, rules),
            None => CameraWatch::start(&inner.setting),
        };
        inner.watch.is_some()
    }

    /// Whether the camera's watch is still asking it what it sees.
    pub(super) fn watching(&self) -> bool {
        let inner = self.inner.locked();
        inner.watch.is_some() && refused_watch(&inner).is_none()
    }

    /// What the camera's watch saw start, change or end since this was last called.
    pub(super) fn take_events(&self) -> Vec<SeenEvent> {
        self.inner
            .locked()
            .watch
            .as_ref()
            .map(CameraWatch::take_events)
            .unwrap_or_default()
    }

    /// Ends the camera's watch, for a camera that goes. Never waits: its thread ends at its next
    /// poll.
    pub(super) fn release_watch(&self) {
        self.inner.locked().watch = None;
    }

    /// The newest picture of a stream of the camera's that a tile shows, for an event's picture:
    /// a stream no tile shows decodes nothing, so its last picture may be long gone by.
    pub(super) fn shown_picture(&self) -> Option<Arc<DecodedFrame>> {
        let keys: Vec<StreamKey> = self
            .inner
            .locked()
            .running()
            .map(|(key, _)| key.clone())
            .collect();
        keys.iter()
            .map(|key| self.output(key))
            .filter(|output| output.shown.load(Ordering::SeqCst))
            .find_map(|output| output.frames().latest())
    }

    pub(super) fn state(&self) -> CameraState {
        let ready = self.ready();

        let mut inner = self.inner.locked();
        let focus = inner.focus();
        // Every running stream's numbers, for the tiles that show each.
        let now = Instant::now();
        let running: Vec<StreamKey> = inner.running().map(|(key, _)| key.clone()).collect();
        for key in &running {
            let output = self.output(key);
            let painted = output.painted.load(Ordering::Relaxed);
            if let Some(run) = inner.runs.get_mut(key) {
                run.stats.sample(&output.frames(), painted, now);
            }
        }
        let use_tcp = inner.setting.use_tcp;
        let clock = SystemTime::now();
        let mut streams: Vec<RunningStream> = running
            .iter()
            .filter_map(|key| {
                let run = inner.runs.get(key)?;
                let (trouble, problem) = status::trouble(&run.feed.as_ref()?.media_status());
                Some(RunningStream {
                    stream: key
                        .as_ref()
                        .map(|stream| stream.key().to_string())
                        .unwrap_or_default(),
                    stats: run.stats.line(use_tcp),
                    brief: run.stats.brief(),
                    waiting: run.stats.waiting(),
                    trouble,
                    problem,
                    last_picture_at: run.stats.last_picture_at(now, clock),
                })
            })
            .collect();
        // One order, so a state that hasn't changed compares equal.
        streams.sort_by(|a, b| a.stream.cmp(&b.stream));
        // The settings of the stream that stands for the camera, for its status and label.
        let shown_setting = inner
            .setting
            .with_stream(focus.as_ref().and_then(Option::as_ref));
        let focused = focus.as_ref().and_then(|key| inner.runs.get(key));

        let (status, tone) = match (
            focused.and_then(|run| run.feed.as_ref()),
            inner.start_error(),
        ) {
            (Some(feed), _) => feed.status(&shown_setting),
            (None, Some(error)) => (format!("Could not connect: {error}"), StatusTone::Warn),
            (None, None) if ready => ("Idle.".to_string(), StatusTone::Normal),
            (None, None) => match refused_watch(&inner) {
                Some(why) => (format!("Not connecting: {why}"), StatusTone::Warn),
                None => (
                    "Add the camera's password in Settings to connect.".to_string(),
                    StatusTone::Normal,
                ),
            },
        };
        let stats = focused
            .map(|run| run.stats.line(inner.setting.use_tcp))
            .unwrap_or_default();

        let ptz_status = inner
            .ptz
            .as_ref()
            .map(PtzSession::status)
            .filter(|status| !status.is_empty())
            .unwrap_or_else(|| "PTZ: not checked yet".to_string());

        // A recording that could not write its file has stopped: say why, once.
        let record = inner
            .feed(&None)
            .map_or(RecordStatus::Off, Feed::record_status);
        if let Some(problem) = record.problem()
            && self.recording.swap(false, Ordering::SeqCst)
        {
            self.recording_by_itself.store(false, Ordering::SeqCst);
            inner.record_problem = Some(problem);
        }

        let watched = inner.watch.as_ref().map(CameraWatch::watched);
        let spotlight = watched
            .as_ref()
            .and_then(|watched| watched.activity.spotlight);

        CameraState {
            id: self.id.clone(),
            name: inner.setting.name.clone(),
            stream_label: shown_setting.stream_label(),
            address: inner.setting.host.clone(),
            streams,
            connected: inner.running().next().is_some(),
            ready,
            status,
            tone,
            stats,
            ptz_status,
            muted: inner.muted,
            volume: f64::from(inner.volume),
            recording: inner.feed(&None).is_some() && self.recording.load(Ordering::SeqCst),
            record_problem: inner.record_problem.clone().unwrap_or_default(),
            motion: watched
                .as_ref()
                .is_some_and(|watched| watched.activity.motion),
            detected: watched
                .as_ref()
                .map(|watched| {
                    watched
                        .activity
                        .detected
                        .iter()
                        .map(|detection| detection.key().to_string())
                        .collect()
                })
                .unwrap_or_default(),
            spotlight: watched
                .as_ref()
                .and_then(|watched| watched.features.as_ref())
                .is_some_and(|features| features.spotlight),
            spotlight_on: spotlight.is_some_and(|spotlight| spotlight.on),
            watch_status: watched.as_ref().map(Watched::summary).unwrap_or_default(),
        }
    }

    /// Asks the camera's watch to switch its spotlight; the answer comes on the returned channel.
    pub(super) fn set_spotlight(
        &self,
        on: bool,
    ) -> Result<mpsc::Receiver<Result<(), String>>, String> {
        let inner = self.inner.locked();
        inner
            .watch
            .as_ref()
            .ok_or_else(|| "Connect the camera first, then switch its spotlight.".to_string())?
            .switch_spotlight(on)
    }

    /// Starts recording the camera's own stream. A camera whose tiles show only another of its
    /// streams connects its own for the recording, which keeps it connected until recording stops.
    /// A file that grows to Settings → Automation's largest goes on in a new one.
    pub(super) fn start_recording(&self) -> Result<(), String> {
        let _transition = self.transition.locked();
        let (own, any) = {
            let inner = self.inner.locked();
            (
                inner.feed(&None).is_some(),
                inner.running().next().is_some(),
            )
        };
        if !any {
            return Err("Connect first, then record.".to_string());
        }
        if !own {
            self.start(&None)?;
        }
        self.recording_tags.locked().clear();
        *self.recording_key.locked() = None;
        *self.recording_rules.locked() = RecordRules {
            split_bytes: self.config.current().automation.video_limit(),
            ..RecordRules::default()
        };
        self.begin_recording()
    }

    /// Starts recording stream `key` for event `event` because the camera started seeing
    /// something, as `rules` say, each file tagged with `tags` and whatever the camera sees while
    /// it records ([`note_event_tags`](Self::note_event_tags)), connecting the stream when it isn't:
    /// it then stays connected for the recording, on screen or not, until the sighting ends
    /// (`stop_recording`). A camera already recording carries on as it was, and stays the user's.
    pub(super) fn record_by_itself(
        &self,
        event: &str,
        tags: &[&str],
        key: StreamKey,
        rules: RecordRules,
    ) -> Result<(), String> {
        if self.is_recording() {
            return Ok(());
        }
        let _transition = self.transition.locked();
        *self.recording_for.locked() = Some(event.to_string());
        *self.recording_tags.locked() = merged_tags(tags.iter().copied());
        *self.recording_key.locked() = key.clone();
        *self.recording_rules.locked() = rules;
        if let Err(why) = self.start(&key).and_then(|()| self.begin_recording()) {
            *self.recording_for.locked() = None;
            *self.recording_key.locked() = None;
            return Err(why);
        }
        self.recording_by_itself.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Adds `tags`, what the camera sees now, to those of the recording an event started, for the
    /// files that open from now on and for each file's name once it closes. Nothing for the user's
    /// recording, or none.
    pub(super) fn note_event_tags(&self, tags: &[&str]) {
        if tags.is_empty() || !self.is_recording_by_itself() {
            return;
        }
        let mut held = self.recording_tags.locked();
        *held = merged_tags(held.iter().map(String::as_str).chain(tags.iter().copied()));
    }

    /// Everything the camera saw since the recording an event started began, as its files' tags.
    pub(super) fn event_tags(&self) -> Vec<String> {
        self.recording_tags.locked().clone()
    }

    /// The event a recording that started by itself is for, and the file it writes now, once the
    /// file has opened: for the hub to link each file to its event.
    pub(super) fn event_recording(&self) -> Option<(String, PathBuf)> {
        if !self.is_recording_by_itself() {
            return None;
        }
        let event = self.recording_for.locked().clone()?;
        Some((event, self.recording_file()?))
    }

    /// Starts the recording stream's feed recording, which runs, as the recording's rules say.
    /// The caller holds `transition`.
    fn begin_recording(&self) -> Result<(), String> {
        let key = self.recording_key.locked().clone();
        let rules = *self.recording_rules.locked();
        let mut inner = self.inner.locked();
        let feed = inner
            .feed(&key)
            .ok_or_else(|| "Connect first, then record.".to_string())?;
        feed.start_recording(
            self.config.videos_dir(),
            self.id.clone(),
            self.recording_tags.clone(),
            rules,
        )?;
        inner.record_problem = None;
        self.recording.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Keeps stream `armed` connected, though no tile shows it, holding back its last seconds for
    /// an event's video to start from, or lets one kept for that go with `None`. Returns whether
    /// this changed what is kept, so the caller brings the streams in line (`reconcile`).
    pub(super) fn set_armed(&self, armed: Option<(StreamKey, Duration)>) -> bool {
        {
            let mut held = self.armed.locked();
            if *held == armed {
                return false;
            }
            held.clone_from(&armed);
        }
        let inner = self.inner.locked();
        for (key, feed) in inner.running() {
            let preroll = armed
                .as_ref()
                .filter(|(kept, _)| kept == key)
                .map(|(_, length)| *length);
            feed.set_preroll(preroll);
        }
        true
    }

    /// Whether the camera records, or was asked to and waits for its stream.
    pub(super) fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }

    /// Whether it records because it started seeing something, rather than because the user
    /// asked.
    pub(super) fn is_recording_by_itself(&self) -> bool {
        self.recording_by_itself.load(Ordering::SeqCst)
    }

    /// Says why a recording stopped by itself, or never started, once, as a full disk's does.
    pub(super) fn set_record_problem(&self, why: String) {
        self.inner.locked().record_problem = Some(why);
    }

    /// Stops recording because a limit was reached, saying `why`, when it records.
    pub(super) fn stop_recording_because(&self, why: String) {
        if self.stop_recording() {
            self.set_record_problem(why);
        }
    }

    /// Stops recording, returning whether it was.
    pub(super) fn stop_recording(&self) -> bool {
        self.recording_by_itself.store(false, Ordering::SeqCst);
        *self.recording_for.locked() = None;
        let was = self.recording.swap(false, Ordering::SeqCst);
        let key = self.recording_key.locked().clone();
        if let Some(feed) = self.inner.locked().feed(&key) {
            feed.stop_recording();
        }
        was
    }

    /// The file being written right now, if any.
    pub(super) fn recording_file(&self) -> Option<PathBuf> {
        let key = self.recording_key.locked().clone();
        match self.inner.locked().feed(&key)?.record_status() {
            RecordStatus::Writing(path) => Some(path),
            _ => None,
        }
    }

    /// Starts the camera's own stream; nothing happens when it is already running.
    #[cfg(test)]
    pub(super) fn connect(&self) -> Result<(), String> {
        self.connect_to(&[None])
    }

    /// Starts each of `streams` that isn't running, password or not, because the user asked, and
    /// starts again each that gave up: nothing else tries one of those again. Every one is
    /// tried; the first failure is the answer.
    pub(super) fn connect_to(&self, streams: &[StreamKey]) -> Result<(), String> {
        let _transition = self.transition.locked();
        let mut first_problem = None;
        for key in streams {
            let gave_up = self
                .inner
                .locked()
                .feed(key)
                .is_some_and(|feed| matches!(feed.media_status(), MediaStatus::Failed(_)));
            if gave_up {
                self.stop(key);
            }
            if let Err(problem) = self.start(key) {
                first_problem.get_or_insert(problem);
            }
        }
        first_problem.map_or(Ok(()), Err)
    }

    /// Opens stream `key`'s feed. The caller holds `transition`.
    pub(super) fn start(&self, key: &StreamKey) -> Result<(), String> {
        let (setting, gain) = {
            let inner = self.inner.locked();
            if inner.feed(key).is_some() {
                return Ok(());
            }
            (inner.setting.with_stream(key.as_ref()), inner.gain_of(key))
        };
        let output = self.output(key);

        let feed = match Feed::open(self.source, &setting) {
            Ok(feed) => feed,
            Err(error) => {
                self.inner
                    .locked()
                    .runs
                    .entry(key.clone())
                    .or_default()
                    .start_error = Some(error.clone());
                return Err(error);
            }
        };

        feed.set_gain(gain);
        feed.set_shown(output.shown.load(Ordering::SeqCst));
        // The stream kept for an event's pre-roll holds it back from its first keyframe.
        let armed = self.armed.locked().clone();
        if let Some((kept, length)) = armed
            && kept == *key
        {
            feed.set_preroll(Some(length));
        }
        // The recording stream restarted while recording, as saving the settings does, records
        // again.
        let records_here = *self.recording_key.locked() == *key;
        let rules = *self.recording_rules.locked();
        if records_here
            && self.recording.load(Ordering::SeqCst)
            && feed
                .start_recording(
                    self.config.videos_dir(),
                    self.id.clone(),
                    self.recording_tags.clone(),
                    rules,
                )
                .is_err()
        {
            self.recording.store(false, Ordering::SeqCst);
            self.recording_by_itself.store(false, Ordering::SeqCst);
            *self.recording_for.locked() = None;
        }
        *output.frames.locked() = feed.frames();
        // The counters belong to the slot that just replaced the old one.
        output.painted.store(0, Ordering::Relaxed);

        self.inner.locked().runs.insert(
            key.clone(),
            Run {
                feed: Some(feed),
                ..Run::default()
            },
        );
        Ok(())
    }

    /// Stops every stream, and with the camera's own any recording.
    pub(super) fn disconnect(&self) {
        let _transition = self.transition.locked();
        self.recording.store(false, Ordering::SeqCst);
        self.recording_by_itself.store(false, Ordering::SeqCst);
        let keys: Vec<StreamKey> = self.inner.locked().runs.keys().cloned().collect();
        for key in &keys {
            self.stop(key);
        }
    }

    /// Closes stream `key`'s feed, and forgets why it could not start. The caller holds
    /// `transition`.
    pub(super) fn stop(&self, key: &StreamKey) {
        let feed = {
            let mut inner = self.inner.locked();
            if inner
                .feed(key)
                .is_some_and(|feed| feed.media_status().login_rejected())
            {
                // Remembered past the stream, which is about to go.
                self.gave_up.store(true, Ordering::SeqCst);
            }
            inner.runs.remove(key).and_then(|run| run.feed)
        };
        // This waits for the pipeline's thread, so it stays outside the lock.
        drop(feed);
        self.output(key).frames().clear();
    }

    /// Connects or disconnects each stream to match the screen, and decodes only those a tile
    /// shows. `wanted` says what the screen asks of this camera; it is asked once the transition
    /// lock is held. A camera that is recording keeps its recording stream connected off screen,
    /// and one armed for events' pre-roll ([`set_armed`](Self::set_armed)) the stream it holds
    /// back.
    pub(super) fn reconcile(&self, wanted: impl FnOnce() -> Wanted) {
        let _transition = self.transition.locked();
        let Wanted {
            shown,
            kept,
            audible,
        } = wanted();
        self.set_audible(audible);

        let recording = self
            .recording
            .load(Ordering::SeqCst)
            .then(|| self.recording_key.locked().clone());
        let armed = self.armed.locked().clone().map(|(key, _)| key);
        // Every stream the screen names, the one recording, the one armed, and those the camera
        // has now, which may need to stop.
        let mut keys: HashSet<StreamKey> = shown.union(&kept).cloned().collect();
        keys.extend(self.inner.locked().runs.keys().cloned());
        keys.extend(recording.clone());
        keys.extend(armed.clone());

        for key in keys {
            let on_screen = shown.contains(&key);
            self.output(&key).shown.store(on_screen, Ordering::SeqCst);
            let connected = on_screen
                || kept.contains(&key)
                || recording.as_ref() == Some(&key)
                || armed.as_ref() == Some(&key);
            let running = {
                let mut inner = self.inner.locked();
                if let Some(feed) = inner.feed(&key) {
                    feed.set_shown(on_screen);
                }
                let running = inner.feed(&key).is_some();
                // Why a tile's stream couldn't start means nothing once nothing wants it.
                if !running && !connected && key.is_some() {
                    inner.runs.remove(&key);
                }
                running
            };

            if connected && !running && self.ready() && !self.held.load(Ordering::SeqCst) {
                // A stream that cannot connect says so in the camera's status.
                let _ = self.start(&key);
            } else if !connected && running {
                self.stop(&key);
            }
        }
    }

    /// Makes `audible`, a stream of this camera, the only one heard; `None` silences them all.
    pub(super) fn set_audible(&self, audible: Option<StreamKey>) {
        let mut inner = self.inner.locked();
        inner.audible = audible;
        let inner = &*inner;
        for (key, feed) in inner.running() {
            feed.set_gain(inner.gain_of(key));
        }
    }

    /// A picture from the camera's own stream for an event's snapshot when no tile shows one: the
    /// stream connects for it, as a recording's does, and its next keyframe's picture is decoded
    /// though nothing shows it. Waits for that, up to [`EVENT_PICTURE_WAIT`], so the caller is off
    /// the main thread, and reconciles afterwards: a stream nothing else wants disconnects again.
    pub(super) fn picture_for_event(&self) -> Result<Arc<DecodedFrame>, String> {
        {
            let _transition = self.transition.locked();
            self.start(&None)?;
        }
        let (frames, before) = {
            let inner = self.inner.locked();
            let feed = inner
                .feed(&None)
                .ok_or_else(|| "its stream didn't start".to_string())?;
            let frames = feed.frames();
            let before = frames.produced();
            feed.want_picture();
            (frames, before)
        };
        let until = Instant::now() + EVENT_PICTURE_WAIT;
        while Instant::now() < until {
            if frames.produced() > before
                && let Some(picture) = frames.latest()
            {
                return Ok(picture);
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err("no picture came in time".to_string())
    }

    /// The newest picture on screen, for a snapshot: the camera's own stream's, else the selected
    /// tile's, else any's. A stream no tile shows decodes nothing, so it has none to give.
    pub(super) fn picture(&self) -> Result<Arc<DecodedFrame>, String> {
        let keys: Vec<StreamKey> = {
            let inner = self.inner.locked();
            if inner.running().next().is_none() {
                return Err("Connect first, then take a snapshot.".to_string());
            }
            let mut keys = vec![None];
            keys.extend(inner.audible.clone());
            keys.extend(inner.running().map(|(key, _)| key.clone()));
            keys
        };
        keys.iter()
            .find_map(|key| self.output(key).frames().latest())
            .ok_or_else(|| "Snapshot failed: no frame has arrived yet".to_string())
    }

    pub(super) fn set_audio(&self, muted: bool, volume: f32) {
        {
            let mut inner = self.inner.locked();
            inner.muted = muted;
            inner.volume = volume;
            let inner = &*inner;
            for (key, feed) in inner.running() {
                feed.set_gain(inner.gain_of(key));
            }
        }

        let _ = self.config.update(|config| {
            config.ui.muted = muted;
            config.ui.volume = volume;
        });
    }

    pub(super) fn ptz_move(&self, pan: f64, tilt: f64, zoom: f64, speed: f64) {
        // Only a stream has a camera behind it. Anything else would log in to the real one.
        if self.source != Source::Stream {
            return;
        }

        let mut inner = self.inner.locked();
        let setting = inner.setting.clone();
        inner
            .ptz
            .get_or_insert_with(|| PtzSession::connect(&setting))
            .move_by(PtzVelocity::new(pan * speed, tilt * speed, zoom * speed));
    }

    pub(super) fn ptz_stop(&self) {
        if let Some(ptz) = &self.inner.locked().ptz {
            ptz.stop();
        }
    }

    /// Stops any move and closes the PTZ connection, returning once the stop has gone out, or
    /// after [`PTZ_RELEASE_WITHIN`] when the camera is slow to take it.
    ///
    /// A plain `ptz_stop` only queues the stop, and a closing app can exit before it is sent,
    /// leaving the camera moving with nothing left to stop it. So this waits for it, which a
    /// camera that answers makes a few milliseconds. The window's thread calls this as a tile
    /// goes, so the wait has an end: a camera that stopped answering can take tens of seconds
    /// over its retries, and it gets them on a thread of its own, while the window carries on.
    /// The next move reconnects.
    pub fn release_ptz(&self) {
        let Some(ptz) = self.inner.locked().ptz.take() else {
            return;
        };
        let (done, closed) = mpsc::channel();
        // Closing waits for PTZ's thread, so it happens outside the lock and off this thread. If
        // the thread can't start, the session closes right here instead, as it used to.
        let _ = std::thread::Builder::new()
            .name("ipcamera-ptz-release".into())
            .spawn(move || {
                drop(ptz);
                let _ = done.send(());
            });
        let _ = closed.recv_timeout(PTZ_RELEASE_WITHIN);
    }

    pub(super) fn form(&self) -> CameraForm {
        form::from_config(&self.setting())
    }

    /// The settings it is using, which are what was saved.
    pub(super) fn setting(&self) -> CameraConfig {
        self.inner.locked().setting.clone()
    }

    /// Saves `setting` with `persist`, which returns what was saved, then starts using it. A
    /// failed save changes nothing, so the running app is never using settings that are not on
    /// disk.
    pub(super) fn save(
        &self,
        setting: CameraConfig,
        persist: impl FnOnce(CameraConfig) -> Result<CameraConfig, String>,
    ) -> Result<(), String> {
        self.adopt(|| persist(setting))
    }

    /// Starts using the settings `saved` returns, which are on disk by the time it returns: each
    /// running stream restarts with them. Several cameras saved in one write each take theirs
    /// this way. When `saved` fails, nothing changes.
    pub(super) fn adopt(
        &self,
        saved: impl FnOnce() -> Result<CameraConfig, String>,
    ) -> Result<(), String> {
        let _transition = self.transition.locked();
        let saved = saved()?;

        let (running, ptz) = {
            let mut inner = self.inner.locked();
            inner.setting = saved;
            let running: Vec<StreamKey> = inner.running().map(|(key, _)| key.clone()).collect();
            // The watch logged in with the old settings too, and starts again with the new ones;
            // dropping it never waits.
            inner.watch = None;
            // PTZ logged in with the old settings. It reconnects on the next move.
            (running, inner.ptz.take())
        };
        // This waits for PTZ's thread, so it stays outside the lock.
        drop(ptz);

        for key in &running {
            self.stop(key);
        }
        // New settings get a new try, the login's included.
        self.gave_up.store(false, Ordering::SeqCst);
        for key in &running {
            // A stream that cannot restart says so in the status line, and the save still stands.
            let _ = self.start(key);
        }
        Ok(())
    }
}

/// Why the camera's watch stopped for good, when it did: the camera refused its login.
fn refused_watch(inner: &Inner) -> Option<String> {
    let watched = inner.watch.as_ref()?.watched();
    watched.gave_up.then(|| watched.problem.unwrap_or_default())
}

pub(super) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
