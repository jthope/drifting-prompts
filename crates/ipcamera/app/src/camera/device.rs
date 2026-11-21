//! One camera: its settings, its streams while connected, its PTZ, sound and recording.
//! Nothing slow runs while `inner`, `outputs` or an output's frames are held: the window reads them
//! as it draws. A thread holding `inner` may take `outputs` or frames, never the other way round.

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

/// Squared, so the 0..100 slider feels linear to the ear; a camera not selected is silent.
pub(super) fn audio_gain(muted: bool, volume: f32, audible: bool) -> f64 {
    if muted || !audible {
        0.0
    } else {
        f64::from(volume / 100.0).powi(2)
    }
}

/// The most the window waits for a camera's last PTZ stop to go out.
const PTZ_RELEASE_WITHIN: Duration = Duration::from_millis(500);

/// Which of a camera's streams: `None` for its own, or one a tile picks.
pub type StreamKey = Option<TileStream>;

#[derive(Default)]
pub(super) struct Run {
    pub(super) feed: Option<Feed>,
    pub(super) start_error: Option<String>,
    pub(super) stats: StreamStats,
}

pub(super) struct Inner {
    pub(super) setting: CameraConfig,
    pub(super) runs: HashMap<StreamKey, Run>,
    /// Opened on first use, so a camera without PTZ isn't logged into for nothing.
    pub(super) ptz: Option<PtzSession>,
    pub(super) watch: Option<CameraWatch>,
    pub(super) muted: bool,
    pub(super) volume: f32,
    pub(super) audible: Option<StreamKey>,
    pub(super) record_problem: Option<String>,
}

impl Inner {
    #[cfg(test)]
    pub(super) fn gain(&self) -> f64 {
        self.gain_of(&None)
    }

    pub(super) fn gain_of(&self, key: &StreamKey) -> f64 {
        audio_gain(self.muted, self.volume, self.audible.as_ref() == Some(key))
    }

    pub(super) fn feed(&self, key: &StreamKey) -> Option<&Feed> {
        self.runs.get(key)?.feed.as_ref()
    }

    fn running(&self) -> impl Iterator<Item = (&StreamKey, &Feed)> {
        self.runs
            .iter()
            .filter_map(|(key, run)| run.feed.as_ref().map(|feed| (key, feed)))
    }

    fn focus(&self) -> Option<StreamKey> {
        self.audible
            .iter()
            .chain([&None])
            .find(|key| self.feed(key).is_some())
            .cloned()
            .or_else(|| self.running().map(|(key, _)| key.clone()).next())
    }

    fn start_error(&self) -> Option<&String> {
        let error = |key: &StreamKey| self.runs.get(key)?.start_error.as_ref();
        self.audible
            .as_ref()
            .and_then(error)
            .or_else(|| error(&None))
            .or_else(|| self.runs.values().find_map(|run| run.start_error.as_ref()))
    }
}

/// Where one of a camera's streams puts its pictures; kept across reconnects, each with a new slot.
pub struct Output {
    frames: Mutex<FrameSlot>,
    painted: AtomicU64,
    /// Whether a tile shows it; shown until TSX first says.
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

/// Its connection, then its next keyframe, which some cameras send only every few seconds.
const EVENT_PICTURE_WAIT: Duration = Duration::from_secs(12);

/// One camera.
pub struct Camera {
    pub(super) id: String,
    pub(super) source: Source,
    pub(super) config: Arc<ConfigStore>,
    pub(super) inner: Mutex<Inner>,
    outputs: Mutex<HashMap<StreamKey, Arc<Output>>>,
    /// Held while a stream starts or stops, so those never overlap.
    pub(super) transition: Mutex<()>,
    pub(super) held: AtomicBool,
    /// Outlives the feed, so a stream restarted by a settings change records again.
    pub(super) recording: AtomicBool,
    /// Started by an event, so it stops as the sighting ends; the user's never does.
    pub(super) recording_by_itself: AtomicBool,
    recording_for: Mutex<Option<String>>,
    recording_tags: Arc<Mutex<Vec<String>>>,
    recording_key: Mutex<StreamKey>,
    recording_rules: Mutex<RecordRules>,
    armed: Mutex<Option<(StreamKey, Duration)>>,
    /// Set when the camera refused a stream's login, so the watch doesn't retry the same password.
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

    /// Whether this camera may connect by itself: never without a password or after a refused watch
    /// login, since failed logins can lock its account. Pressing Connect still tries.
    pub(super) fn ready(&self) -> bool {
        let inner = self.inner.locked();
        self.source == Source::Pattern
            || (!inner.setting.password.is_empty() && refused_watch(&inner).is_none())
    }

    pub(super) fn held(&self) -> bool {
        self.held.load(Ordering::SeqCst)
    }

    /// Starts the camera's watch once it may log in: never beside a stream's first login, for the
    /// test picture, or after a refused login. Returns whether it started one.
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

    pub(super) fn watching(&self) -> bool {
        let inner = self.inner.locked();
        inner.watch.is_some() && refused_watch(&inner).is_none()
    }

    pub(super) fn take_events(&self) -> Vec<SeenEvent> {
        self.inner
            .locked()
            .watch
            .as_ref()
            .map(CameraWatch::take_events)
            .unwrap_or_default()
    }

    pub(super) fn release_watch(&self) {
        self.inner.locked().watch = None;
    }

    /// The newest picture of a stream a tile shows: one no tile shows decodes nothing.
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

    /// Records stream `key` for event `event`; a camera already recording carries on as the user's.
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

    pub(super) fn note_event_tags(&self, tags: &[&str]) {
        if tags.is_empty() || !self.is_recording_by_itself() {
            return;
        }
        let mut held = self.recording_tags.locked();
        *held = merged_tags(held.iter().map(String::as_str).chain(tags.iter().copied()));
    }

    pub(super) fn event_tags(&self) -> Vec<String> {
        self.recording_tags.locked().clone()
    }

    pub(super) fn event_recording(&self) -> Option<(String, PathBuf)> {
        if !self.is_recording_by_itself() {
            return None;
        }
        let event = self.recording_for.locked().clone()?;
        Some((event, self.recording_file()?))
    }

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

    /// Keeps stream `armed` connected for an event's pre-roll; returns whether to reconcile.
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

    pub(super) fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }

    pub(super) fn is_recording_by_itself(&self) -> bool {
        self.recording_by_itself.load(Ordering::SeqCst)
    }

    pub(super) fn set_record_problem(&self, why: String) {
        self.inner.locked().record_problem = Some(why);
    }

    pub(super) fn stop_recording_because(&self, why: String) {
        if self.stop_recording() {
            self.set_record_problem(why);
        }
    }

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

    pub(super) fn recording_file(&self) -> Option<PathBuf> {
        let key = self.recording_key.locked().clone();
        match self.inner.locked().feed(&key)?.record_status() {
            RecordStatus::Writing(path) => Some(path),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn connect(&self) -> Result<(), String> {
        self.connect_to(&[None])
    }

    /// Starts each of `streams` because the user asked, those that gave up included, since nothing
    /// else retries them. The first failure is the answer.
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

    /// The caller holds `transition`.
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
        let armed = self.armed.locked().clone();
        if let Some((kept, length)) = armed
            && kept == *key
        {
            feed.set_preroll(Some(length));
        }
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

    pub(super) fn disconnect(&self) {
        let _transition = self.transition.locked();
        self.recording.store(false, Ordering::SeqCst);
        self.recording_by_itself.store(false, Ordering::SeqCst);
        let keys: Vec<StreamKey> = self.inner.locked().runs.keys().cloned().collect();
        for key in &keys {
            self.stop(key);
        }
    }

    /// The caller holds `transition`.
    pub(super) fn stop(&self, key: &StreamKey) {
        let feed = {
            let mut inner = self.inner.locked();
            if inner
                .feed(key)
                .is_some_and(|feed| feed.media_status().login_rejected())
            {
                self.gave_up.store(true, Ordering::SeqCst);
            }
            inner.runs.remove(key).and_then(|run| run.feed)
        };
        // This waits for the pipeline's thread, so it stays outside the lock.
        drop(feed);
        self.output(key).frames().clear();
    }

    /// Connects or disconnects each stream to match the screen; asks `wanted` under `transition`.
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

    pub(super) fn set_audible(&self, audible: Option<StreamKey>) {
        let mut inner = self.inner.locked();
        inner.audible = audible;
        let inner = &*inner;
        for (key, feed) in inner.running() {
            feed.set_gain(inner.gain_of(key));
        }
    }

    /// A picture from the camera's own stream for an event, connecting it if no tile shows one.
    /// Blocks up to `EVENT_PICTURE_WAIT`.
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
        // Only a stream has a camera behind it; anything else would log in to the real one.
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

    /// Stops any move and closes PTZ, waiting up to `PTZ_RELEASE_WITHIN` for the stop to go out: a
    /// closing app could exit first and leave the camera moving.
    pub fn release_ptz(&self) {
        let Some(ptz) = self.inner.locked().ptz.take() else {
            return;
        };
        let (done, closed) = mpsc::channel();
        // Closing waits for PTZ's thread, so it happens outside the lock and off this thread.
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

    pub(super) fn setting(&self) -> CameraConfig {
        self.inner.locked().setting.clone()
    }

    /// Saves `setting` with `persist`, then uses it; a failed save changes nothing.
    pub(super) fn save(
        &self,
        setting: CameraConfig,
        persist: impl FnOnce(CameraConfig) -> Result<CameraConfig, String>,
    ) -> Result<(), String> {
        self.adopt(|| persist(setting))
    }

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
            // The watch restarts with the new settings; dropping it never waits.
            inner.watch = None;
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
            // A stream that can't restart says so in its status; the save stands.
            let _ = self.start(key);
        }
        Ok(())
    }
}

fn refused_watch(inner: &Inner) -> Option<String> {
    let watched = inner.watch.as_ref()?.watched();
    watched.gave_up.then(|| watched.problem.unwrap_or_default())
}

pub(super) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
