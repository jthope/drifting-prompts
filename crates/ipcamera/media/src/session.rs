//! One session, over RTSP or a Reolink's FLV stream, on a thread of its own: reconnect and volume.
//!
//! Only the session thread touches the graph (glass2glass's elements need not be `Send`), and a
//! panic in it becomes a `Failed` status, never a frozen `Live`.

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

/// Failed reconnects in a row before giving up on a camera that is unplugged.
const MAX_ATTEMPTS: u32 = 20;

const ANSWER_WITHIN: Duration = Duration::from_secs(10);

const CHECK_EVERY: Duration = Duration::from_millis(50);

/// What the session thread is doing.
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
    /// It gave up for good; the sentence says why.
    Failed(String),
}

impl MediaStatus {
    /// Whether it gave up because the camera refused the login, which nothing should then retry.
    pub fn login_rejected(&self) -> bool {
        matches!(self, Self::Failed(why) if *why == MediaError::LoginRejected.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Output {
    Display,
    Relay,
}

struct Shared {
    output: Output,
    status: Mutex<MediaStatus>,
    visibility: Arc<Visibility>,
    feed: Feed,
    stop: AtomicBool,
    /// Linear, 0.0..=1.0.
    gain: Mutex<f64>,
    record: RecordControl,
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

/// A live view of one camera, or a relay of its video; dropping it stops and joins its thread.
pub struct MediaSession {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl MediaSession {
    /// Decodes the camera's pictures into [`Self::frames`] and plays its sound, on a thread of its
    /// own.
    pub fn start(camera: &CameraConfig) -> Result<Self> {
        Self::begin(camera, Output::Display)
    }

    /// Passes the camera's video on to [`Self::feed`]'s viewers as sent, decoding nothing.
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
                    // A crash outside the pipeline must not leave the status as it was either.
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

    /// The slot the UI paints from; it outlives reconnects.
    pub fn frames(&self) -> FrameSlot {
        self.shared.visibility.frames()
    }

    /// Whether anything shows the pictures; a hidden live view stays connected but decodes nothing.
    pub fn set_shown(&self, shown: bool) {
        self.shared.visibility.set_shown(shown);
    }

    /// The compressed video a relay passes on; it outlives reconnects.
    pub fn feed(&self) -> Feed {
        self.shared.feed.clone()
    }

    /// Sets the playback gain: `0.0` is muted, `1.0` the camera's own level.
    pub fn set_volume(&self, gain: f64) {
        *self.shared.gain.locked() = gain.clamp(0.0, 1.0);
    }

    /// Records from the next keyframe, or the pre-roll where `rules` say so, into files
    /// `next_file` names; `next_file` runs on the media thread.
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

    /// Holds back the last `preroll` of the stream for a recording to start from; `None` holds
    /// nothing.
    pub fn set_preroll(&self, preroll: Option<Duration>) {
        self.shared.record.set_preroll(preroll);
    }

    /// Asks for the next keyframe's picture in [`Self::frames`], even while hidden.
    pub fn want_picture(&self) {
        self.shared.visibility.want_picture();
    }

    /// Stops recording; the file is finished at the next packet, or within 50 ms.
    pub fn stop_recording(&self) {
        self.shared.record.stop();
    }

    /// Whether it gave up for a reason a restart would only repeat; restarting after a rejected
    /// login can lock the camera's account.
    pub fn refused(&self) -> bool {
        self.shared.refused.load(Ordering::SeqCst)
    }

    /// What the recording is doing now.
    pub fn record_status(&self) -> RecordStatus {
        self.shared.record.status()
    }

    /// Stops the worker and waits for it, finishing any recording first; safe to call twice.
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
            // A crash counts as a try that never went live, so one that always crashes gives up.
            Err(error) => {
                // The crash skipped the pipeline's tidying up, so its runtime is replaced.
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

fn wait_before_retry(shared: &Shared, delay: Duration) {
    let mut waited = Duration::ZERO;
    while waited < delay && !shared.stopping() {
        thread::sleep(CHECK_EVERY);
        waited += CHECK_EVERY;
    }
}

/// Counts failures in a row: a stream that went live had recovered, so its drop is a new outage.
fn next_attempt(attempt: u32, went_live: bool, auto_reconnect: bool) -> Option<u32> {
    let failed_in_a_row = if went_live { 0 } else { attempt };
    (auto_reconnect && failed_in_a_row < MAX_ATTEMPTS).then_some(failed_in_a_row + 1)
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(match attempt {
        1 => 2,
        2 => 5,
        3 => 10,
        _ => 15,
    })
}

enum Ended {
    Stopped,
    Failed { error: MediaError, went_live: bool },
}

/// The FLV login blocks, so it happens before the runtime, not on it.
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
            // A relay keeps only the sound a recording can hold.
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

        // Polled alongside the run, never instead of it: the run itself carries out a gain change.
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
                // A relay's sound only goes to its recordings.
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
        recorder.locked().close();
        ended
    })
}

fn cannot_play(error: &G2gError) -> bool {
    matches!(error, G2gError::CapsMismatch | G2gError::FixationFailed)
}

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
