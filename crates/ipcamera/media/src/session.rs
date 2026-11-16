//! One session, over RTSP or a Reolink's FLV stream ([`crate::flv`]): a glass2glass pipeline on its
//! own thread, with reconnect and volume control.
//!
//! Threading rules this follows, and why:
//!
//! - **The session thread owns the pipeline.** It runs a current-thread tokio runtime that drives
//!   the whole graph, so an element stays on the thread that set it up: glass2glass doesn't ask
//!   its elements to be `Send`, and its Media Foundation ones aren't. Nothing else ever touches an
//!   element.
//! - **The UI thread only touches shared state.** It reads [`MediaStatus`] and the newest frame
//!   in the [`FrameSlot`], and asks for a new gain, all of which are thread-safe. The session thread
//!   passes the gain on to the running `volume` element.
//! - **A dead session is obvious.** Every failure becomes a `Failed` status sentence rather than
//!   a silent black pane. That includes a crash: a panic in the pipeline is caught and handled
//!   like any other failure, so the status never stays at `Live` over a frozen picture.

use std::{
    panic::{self, AssertUnwindSafe},
    path::PathBuf,
    pin::pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use g2g_core::{
    G2gError, HardwareError, NetworkFailure,
    property::PropValue,
    runtime::{LatencyProfile, run_graph_mutable},
};
use g2g_plugins::clock::WallClock;
use tokio::runtime::Runtime;

use ipcamera_core::{CameraConfig, StreamPath, rtsp_url, sync::LockExt};

use crate::{
    FrameSlot,
    error::{MediaError, Result, failed},
    flv,
    gate::Visibility,
    pipeline,
    record::{RecordControl, RecordRules, RecordStatus, Recorder},
    relay::Feed,
};

/// Give up after this many failed reconnects in a row rather than retrying a camera that is
/// unplugged.
const MAX_ATTEMPTS: u32 = 20;

/// How long the camera has to answer the first request before the attempt counts as failed.
const ANSWER_WITHIN: Duration = Duration::from_secs(10);

/// How often the session thread looks at what the UI asked for.
const CHECK_EVERY: Duration = Duration::from_millis(50);

/// What the session thread is doing. The window polls this and turns it into status text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaStatus {
    /// Making the first connection.
    Connecting,
    /// Pictures are arriving.
    Live,
    /// The stream failed and is being tried again.
    Reconnecting {
        /// Which try this is, from 1.
        attempt: u32,
    },
    /// [`MediaSession::stop`] ended it.
    Stopped,
    /// It gave up, and will not try again. The sentence says why.
    Failed(String),
}

impl MediaStatus {
    /// Whether it gave up because the camera refused the login: the one failure that says the
    /// password is wrong, which nothing else should then try by itself.
    pub fn login_rejected(&self) -> bool {
        matches!(self, Self::Failed(why) if *why == MediaError::LoginRejected.to_string())
    }
}

/// What a session does with the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Output {
    /// Decodes it into the frame slot and plays its sound: the desktop's live view.
    Display,
    /// Passes the compressed video on to the feed's viewers, decoding nothing: the server's.
    Relay,
}

struct Shared {
    output: Output,
    status: Mutex<MediaStatus>,
    /// The frame slot a live view decodes into, and whether anything shows it.
    visibility: Arc<Visibility>,
    /// The compressed video for viewers, which only a relaying session fills.
    feed: Feed,
    stop: AtomicBool,
    /// Desired linear gain, 0.0..=1.0, kept so a reconnected pipeline picks it up again.
    gain: Mutex<f64>,
    /// Whether to record, which a reconnected pipeline picks up again too.
    record: RecordControl,
    /// Set when it gave up for a reason a retry would only repeat ([`MediaSession::refused`]).
    refused: AtomicBool,
}

impl Shared {
    fn new(output: Output) -> Self {
        Self {
            output,
            status: Mutex::new(MediaStatus::Connecting),
            visibility: Arc::new(Visibility::default()),
            feed: Feed::default(),
            stop: AtomicBool::new(false),
            gain: Mutex::new(1.0),
            record: RecordControl::default(),
            refused: AtomicBool::new(false),
        }
    }

    fn set_status(&self, status: MediaStatus) {
        *self.status.locked() = status;
    }

    fn status(&self) -> MediaStatus {
        self.status.locked().clone()
    }

    fn stopping(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn gain(&self) -> f64 {
        *self.gain.locked()
    }
}

/// A live view of one camera, or a relay of its video. Dropping it stops the pipeline and joins its
/// thread.
pub struct MediaSession {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl MediaSession {
    /// Validates the settings and hands the connection to a worker thread, which decodes the
    /// pictures into [`Self::frames`] and plays the sound.
    pub fn start(camera: &CameraConfig) -> Result<Self> {
        Self::begin(camera, Output::Display)
    }

    /// Validates the settings and hands the connection to a worker thread, which passes the
    /// camera's video on to [`Self::feed`]'s viewers as the camera sent it ([`crate::relay`]).
    ///
    /// It decodes nothing and plays no sound, so it needs no screen and no speakers. It reconnects,
    /// stops for good, and records ([`Self::start_recording`]) exactly as a live view does, an AAC
    /// camera's sound included. A picture is decoded only when asked for ([`Feed::picture`]).
    pub fn relay(camera: &CameraConfig) -> Result<Self> {
        Self::begin(camera, Output::Relay)
    }

    fn begin(camera: &CameraConfig, output: Output) -> Result<Self> {
        camera.validate_stream()?;

        let shared = Arc::new(Shared::new(output));
        let worker = {
            let shared = shared.clone();
            let camera = camera.clone();
            thread::Builder::new()
                .name("ipcamera-rtsp".into())
                .spawn(move || {
                    // `supervise` catches a crash in the pipeline itself; this is for one anywhere
                    // else, so even then the thread never ends with the status left as it was.
                    if let Err(error) = contain(|| supervise(camera, &shared, play_once)) {
                        shared.set_status(MediaStatus::Failed(error.to_string()));
                    }
                })?
        };

        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    /// What the session is doing now.
    pub fn status(&self) -> MediaStatus {
        self.shared.status()
    }

    /// The slot the UI paints from. It outlives reconnects.
    pub fn frames(&self) -> FrameSlot {
        self.shared.visibility.frames()
    }

    /// Whether anything shows the pictures; a session starts shown. A hidden live view stays
    /// connected, live and recording, but decodes nothing and keeps nothing in [`Self::frames`];
    /// shown again, it starts from the stream's last keyframe, so its picture is back within a
    /// moment ([`crate::gate`]). A relay decodes nothing either way.
    pub fn set_shown(&self, shown: bool) {
        self.shared.visibility.set_shown(shown);
    }

    /// The compressed video a relaying session passes on. It outlives reconnects, and stays empty
    /// in a live view.
    pub fn feed(&self) -> Feed {
        self.shared.feed.clone()
    }

    /// Sets the playback gain. `0.0` is muted; `1.0` is the camera's own level.
    pub fn set_volume(&self, gain: f64) {
        *self.shared.gain.locked() = gain.clamp(0.0, 1.0);
    }

    /// Records the stream from its next keyframe on, or from what [`Self::set_preroll`] holds back
    /// where `rules` say so, until [`Self::stop_recording`], into files `next_file` names, given
    /// when each file's first picture arrived. It is called on the media thread whenever a file
    /// opens: when recording starts, each time a file goes on in the next by `rules`, and each time
    /// the stream reconnects. The next picture decoded after a file opens reaches
    /// [`Self::frames`] even while the view is hidden, so a thumbnail can be made of it.
    pub fn start_recording(
        &self,
        rules: RecordRules,
        next_file: impl Fn(SystemTime) -> PathBuf + Send + Sync + 'static,
    ) {
        let visibility = self.shared.visibility.clone();
        self.shared.record.start(
            Arc::new(move |started| {
                visibility.want_picture();
                next_file(started)
            }),
            rules,
        );
    }

    /// Holds back the last `preroll` of the stream as the camera sent it (whole keyframe
    /// intervals, up to [`PREROLL_MOST_BYTES`](crate::PREROLL_MOST_BYTES)), for a
    /// recording to start from ([`RecordRules::from_preroll`]); `None` holds nothing. It goes on
    /// across reconnects, starting again from the new stream's first keyframe.
    pub fn set_preroll(&self, preroll: Option<Duration>) {
        self.shared.record.set_preroll(preroll);
    }

    /// Asks for the picture of the stream's next keyframe in [`Self::frames`], even while the view
    /// is hidden: for an event's snapshot of a camera no tile shows. It comes within a keyframe
    /// interval of a live stream, and nothing comes while the stream isn't live.
    pub fn want_picture(&self) {
        self.shared.visibility.want_picture();
    }

    /// Stops recording. The file is finished at the stream's next packet, or within 50 ms.
    pub fn stop_recording(&self) {
        self.shared.record.stop();
    }

    /// Whether it gave up for a reason starting it again would only repeat: the camera rejected the
    /// login, or sends a stream this machine can't play. Starting it again after a rejected login
    /// does harm, since failed logins can lock the camera's account, so only a change to the
    /// camera's settings should. Any other failure may clear up by itself.
    pub fn refused(&self) -> bool {
        self.shared.refused.load(Ordering::SeqCst)
    }

    /// What the recording is doing now.
    pub fn record_status(&self) -> RecordStatus {
        self.shared.record.status()
    }

    /// Asks the worker to stop and waits for it, finishing any recording first. Safe to call twice.
    pub fn stop(&mut self) {
        self.shared.record.stop();
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.shared.set_status(MediaStatus::Stopped);
        self.shared.visibility.frames().clear();
        self.shared.feed.forget();
    }
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Keeps a pipeline running until the user stops it, reconnecting with a backoff after a failure.
///
/// `play` is [`play_once`]; the tests hand in their own, to crash on cue.
fn supervise(
    camera: CameraConfig,
    shared: &Shared,
    mut play: impl FnMut(&Runtime, &CameraConfig, &Shared) -> Ended,
) {
    let Some(mut runtime) = new_runtime(shared) else {
        return;
    };
    let mut attempt: u32 = 0;

    loop {
        if shared.stopping() {
            return;
        }
        shared.set_status(if attempt == 0 {
            MediaStatus::Connecting
        } else {
            MediaStatus::Reconnecting { attempt }
        });

        let ended = match contain(|| play(&runtime, &camera, shared)) {
            Ok(ended) => ended,
            // A crash, such as a decoder tripping over a corrupt packet, is retried like a broken
            // stream. It counts as a try that never went live, so one that crashes every time
            // still gives up after `MAX_ATTEMPTS`.
            Err(error) => {
                // The crash skipped the pipeline's own tidying up. Whatever it left on the runtime
                // goes with it, and the recording's file ends where it was cut, which a
                // fragmented MP4 plays up to.
                shared.record.closed();
                let Some(fresh) = new_runtime(shared) else {
                    return;
                };
                runtime = fresh;
                Ended::Failed {
                    error,
                    went_live: false,
                }
            }
        };
        let Ended::Failed { error, went_live } = ended else {
            return;
        };
        if shared.stopping() {
            return;
        }

        let refused = error.is_final();
        let next = if refused {
            None
        } else {
            next_attempt(attempt, went_live, camera.auto_reconnect)
        };
        let Some(next) = next else {
            shared.refused.store(refused, Ordering::SeqCst);
            shared.set_status(MediaStatus::Failed(error.to_string()));
            return;
        };
        attempt = next;
        shared.set_status(MediaStatus::Reconnecting { attempt });
        wait_before_retry(shared, backoff(attempt));
    }
}

/// The runtime that drives the pipeline on this thread, or `None` after saying why there is none.
fn new_runtime(shared: &Shared) -> Option<Runtime> {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => Some(runtime),
        Err(error) => {
            shared.set_status(MediaStatus::Failed(format!(
                "the media thread could not start: {error}"
            )));
            None
        }
    }
}

/// Runs `work`, turning a panic in it into an error. Like a C# `catch (Exception)` around it: the
/// panic's message still goes to the log as it happens, and the caller gets a sentence to show.
fn contain<R>(work: impl FnOnce() -> R) -> Result<R> {
    panic::catch_unwind(AssertUnwindSafe(work)).map_err(|panic| {
        let reason = panic
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("no reason given");
        failed!("the video pipeline crashed: {reason}")
    })
}

/// Sleeps for `delay`, waking early when the user stops the session.
fn wait_before_retry(shared: &Shared, delay: Duration) {
    let mut waited = Duration::ZERO;
    while waited < delay && !shared.stopping() {
        thread::sleep(CHECK_EVERY);
        waited += CHECK_EVERY;
    }
}

/// The number of the next try after a failure, or `None` when it is time to give up.
///
/// The limit is for a camera that has gone away, so it counts failures in a row. A stream that
/// showed a picture had recovered, which makes its drop a new outage. Counting on from there would
/// stop a camera for good after twenty drops spread over days.
fn next_attempt(attempt: u32, went_live: bool, auto_reconnect: bool) -> Option<u32> {
    let failed_in_a_row = if went_live { 0 } else { attempt };
    (auto_reconnect && failed_in_a_row < MAX_ATTEMPTS).then_some(failed_in_a_row + 1)
}

/// 2 s, 5 s, 10 s, then every 15 s.
fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(match attempt {
        1 => 2,
        2 => 5,
        3 => 10,
        _ => 15,
    })
}

/// How one pipeline ended.
enum Ended {
    /// The user asked to stop, so there is nothing to reconnect.
    Stopped,
    Failed {
        error: MediaError,
        /// Whether a picture arrived first, which makes this a new outage rather than one more
        /// failed try ([`next_attempt`]).
        went_live: bool,
    },
}

/// Asks the camera what it sends, builds a pipeline for that, and plays it until it fails or the
/// user stops. The pipeline is torn down when this returns, because the run that owns it ends.
///
/// An RTSP stream is asked with a DESCRIBE. A Reolink's FLV stream is opened with a token from a
/// login to its API first ([`flv::log_in`]), which blocks, so it happens here before the runtime
/// rather than on it; the stream's head then says what it sends.
fn play_once(runtime: &Runtime, camera: &CameraConfig, shared: &Shared) -> Ended {
    let gave_up = |error| Ended::Failed {
        error,
        went_live: false,
    };
    let login = match camera.stream_path() {
        StreamPath::Rtsp(_) => None,
        StreamPath::ReolinkFlv(name) => match flv::log_in(camera, name) {
            Ok(login) => Some(login),
            Err(error) => return gave_up(error),
        },
    };
    runtime.block_on(async {
        let asked = async {
            match login {
                None => {
                    let url = rtsp_url::build(camera);
                    let offer = pipeline::describe(&url, camera).await?;
                    Ok((pipeline::Source::Rtsp(url), offer))
                }
                Some(login) => flv::open(login)
                    .await
                    .map(|(answer, offer)| (pipeline::Source::Flv(answer), offer)),
            }
        };

        let (source, offer) = match tokio::time::timeout(ANSWER_WITHIN, asked).await {
            // A relay passes the video on, and keeps only the sound a recording can hold.
            Ok(Ok((source, offer))) if shared.output == Output::Relay => (
                source,
                pipeline::Offer {
                    audio: offer
                        .audio
                        .filter(|audio| *audio == pipeline::AudioCodec::Aac),
                    ..offer
                },
            ),
            Ok(Ok(asked)) => asked,
            Ok(Err(error)) => return gave_up(error),
            Err(_) => {
                return gave_up(failed!(
                    "the camera did not answer within {} s",
                    ANSWER_WITHIN.as_secs()
                ));
            }
        };
        if shared.stopping() {
            return Ended::Stopped;
        }

        let live = Arc::new(AtomicBool::new(false));
        let recorder = Recorder::new(shared.record.clone());
        let built = match shared.output {
            Output::Display => pipeline::build(
                source,
                camera,
                &offer,
                shared.visibility.clone(),
                live.clone(),
                shared.gain(),
                recorder.clone(),
            ),
            Output::Relay => {
                // The stream before this one has ended, and its pictures with it.
                shared.feed.forget();
                pipeline::build_relay(
                    source,
                    camera,
                    &offer,
                    shared.feed.clone(),
                    live.clone(),
                    recorder.clone(),
                )
            }
        };
        let graph = match built {
            Ok(graph) => graph,
            // Nothing in building a graph touches the network, so it fails the same way every time.
            Err(error) => return gave_up(MediaError::CannotPlay(error.to_string())),
        };

        let clock = WallClock::new();
        let (mutator, run) = run_graph_mutable(graph, &clock, LatencyProfile::Live);
        let run = pin!(run);

        // Polled alongside the run, never instead of it: a gain change is carried out by the run
        // itself, so the run has to keep going while this waits for it.
        let control = async {
            let mut applied = shared.gain();
            let mut went_live = false;
            let mut tick = tokio::time::interval(CHECK_EVERY);
            loop {
                tick.tick().await;
                if shared.stopping() {
                    return;
                }
                // A stopped recording finishes its file even when the stream has gone quiet.
                recorder.locked().check();
                if !went_live && live.load(Ordering::Relaxed) {
                    went_live = true;
                    shared.set_status(MediaStatus::Live);
                }
                let wanted = shared.gain();
                // A relay's sound only goes to its recordings, so it has no volume to change.
                if offer.audio.is_some()
                    && shared.output == Output::Display
                    && wanted != applied
                    && mutator
                        .set_property(pipeline::VOLUME, "volume", PropValue::Double(wanted))
                        .await
                        .is_ok()
                {
                    applied = wanted;
                }
            }
        };

        let ended = tokio::select! {
            ended = run => Ended::Failed {
                error: match ended {
                    Ok(_) => failed!("the camera ended the stream"),
                    Err(error) if cannot_play(&error) => MediaError::CannotPlay(sentence(&error)),
                    Err(error) => MediaError::Failed(sentence(&error)),
                },
                went_live: live.load(Ordering::Relaxed),
            },
            () = control => Ended::Stopped,
        };
        // The file this pipeline was writing ends with it. A recording still asked for starts a
        // new file in the reconnected stream.
        recorder.locked().close();
        ended
    })
}

/// Whether a run ended in a way a retry would only repeat.
fn cannot_play(error: &G2gError) -> bool {
    matches!(error, G2gError::CapsMismatch | G2gError::FixationFailed)
}

/// A glass2glass failure as a sentence for the status line. Its details, retina's message among
/// them, go to glass2glass's own log.
fn sentence(error: &G2gError) -> String {
    match error {
        G2gError::Network(NetworkFailure::Connect) => {
            "the camera would not set up the stream".to_string()
        }
        G2gError::Network(_) | G2gError::RtspStream => "the camera stream broke off".to_string(),
        G2gError::Timeout => "the camera stopped sending video".to_string(),
        G2gError::Hardware(HardwareError::MediaFoundation(code)) => {
            format!("Media Foundation failed with 0x{code:08X}")
        }
        G2gError::Hardware(device) => format!("the {device:?} device failed"),
        G2gError::CapsMismatch | G2gError::FixationFailed => {
            "the pipeline could not agree on a picture or sound format".to_string()
        }
        other => format!("the pipeline failed: {other:?}"),
    }
}

#[cfg(test)]
mod tests;
