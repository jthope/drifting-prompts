//! Recording a stream as the camera sent it, into fragmented MP4s that still play when cut short.
//!
//! The muxer is driven by hand and never waits ([`FileOut`] writes each fragment at once), so the
//! recorder's lock is never held across a wait.

mod mp4;

use std::{
    collections::VecDeque,
    future::Future,
    path::PathBuf,
    pin::pin,
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll, Waker},
    time::{Duration, Instant, SystemTime},
};

use g2g_core::{
    AudioFormat, Caps, ElementMetadata, Frame, G2gError, MemoryDomain, MultiInputElement,
    PipelinePacket, SystemSlice,
};
use g2g_plugins::mp4muxn::Mp4MuxN;
use ipcamera_core::sync::LockExt;

use crate::{
    annexb::ParameterSets,
    sink::fixed_size,
    tap::{Tap, Watch},
};
use mp4::FileOut;

const FRAGMENT_MS: u64 = 1_000;

/// A file gets sound only if some came this recently, or stopped sound holds the picture back.
const SOUND_IS_FLOWING: Duration = Duration::from_secs(2);

/// Sound that stops for this long ends the file's sound track.
const SOUND_STALLED: Duration = Duration::from_secs(3);

/// The most bytes held back for a pre-roll; a keyframe interval larger than this holds none.
pub const PREROLL_MOST_BYTES: usize = 32 * 1024 * 1024;

/// How a recording asked for now goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordRules {
    /// Start from what the session holds back, opening the first file at once.
    pub from_preroll: bool,
    /// The longest a file runs before the recording goes on in a new one; `None` for no limit.
    pub split_after: Option<Duration>,
    /// The bytes a file grows to before the recording goes on in a new one; `None` for no limit.
    pub split_bytes: Option<u64>,
}

/// What a recording is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordStatus {
    /// Not recording.
    Off,
    /// Asked to record, and waiting for a keyframe or for the stream.
    Waiting,
    /// Writing this file.
    Writing(PathBuf),
    /// Stopped because a file could not be written; the sentence says why.
    Failed(String),
}

impl RecordStatus {
    /// What the user reads when recording stopped by itself.
    pub fn problem(&self) -> Option<String> {
        match self {
            Self::Failed(why) => Some(format!("Recording stopped: {why}")),
            _ => None,
        }
    }
}

/// Names each new file from when its first picture arrived; it runs on the media thread, and may
/// create the file empty.
pub type NextFile = Arc<dyn Fn(SystemTime) -> PathBuf + Send + Sync>;

#[derive(Default)]
struct ControlState {
    target: Option<NextFile>,
    status: Option<RecordStatus>,
    rules: RecordRules,
    preroll_pending: bool,
    preroll: Option<Duration>,
}

struct Plan {
    target: Option<NextFile>,
    rules: RecordRules,
    preroll: Option<Duration>,
}

/// Outlives each pipeline, so a recording carries on into a reconnected stream.
#[derive(Clone, Default)]
pub(crate) struct RecordControl(Arc<Mutex<ControlState>>);

impl RecordControl {
    fn lock(&self) -> MutexGuard<'_, ControlState> {
        self.0.locked()
    }

    pub(crate) fn start(&self, next_file: NextFile, rules: RecordRules) {
        let mut state = self.lock();
        let writing = matches!(state.status, Some(RecordStatus::Writing(_)));
        state.target = Some(next_file);
        state.rules = rules;
        state.preroll_pending = rules.from_preroll;
        if !writing {
            state.status = Some(RecordStatus::Waiting);
        }
    }

    pub(crate) fn set_preroll(&self, preroll: Option<Duration>) {
        self.lock().preroll = preroll;
    }

    fn plan(&self) -> Plan {
        let state = self.lock();
        Plan {
            target: state.target.clone(),
            rules: state.rules,
            preroll: state.preroll,
        }
    }

    fn preroll(&self) -> Option<Duration> {
        self.lock().preroll
    }

    fn take_preroll(&self) -> bool {
        std::mem::take(&mut self.lock().preroll_pending)
    }

    pub(crate) fn stop(&self) {
        let mut state = self.lock();
        state.target = None;
        if !matches!(state.status, Some(RecordStatus::Writing(_))) {
            state.status = Some(RecordStatus::Off);
        }
    }

    pub(crate) fn status(&self) -> RecordStatus {
        self.lock().status.clone().unwrap_or(RecordStatus::Off)
    }

    fn target(&self) -> Option<NextFile> {
        self.lock().target.clone()
    }

    fn set_status(&self, status: RecordStatus) {
        self.lock().status = Some(status);
    }

    pub(crate) fn closed(&self) {
        let mut state = self.lock();
        state.status = Some(if state.target.is_some() {
            RecordStatus::Waiting
        } else {
            RecordStatus::Off
        });
    }

    /// Stops recording rather than failing again at every keyframe, as a full disk would.
    fn failed(&self, why: String) {
        let mut state = self.lock();
        state.target = None;
        state.status = Some(RecordStatus::Failed(why));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Track {
    Video,
    Sound,
}

struct Held {
    track: Track,
    frame: Frame,
    bytes: usize,
    arrived: Instant,
    at: SystemTime,
    carries_parameter_sets: bool,
}

struct OpenFile {
    path: PathBuf,
    mux: Mp4MuxN,
    out: FileOut,
    start_pts: u64,
    last_pts: u64,
    frame_ns: u64,
    opened: Instant,
    size: Option<(u32, u32)>,
    sound: Option<SoundTrack>,
}

impl OpenFile {
    fn due(&self, rules: &RecordRules, pts: u64) -> bool {
        let ran = Duration::from_nanos(pts.saturating_sub(self.start_pts));
        rules.split_after.is_some_and(|longest| ran >= longest)
            || rules.split_bytes.is_some_and(|most| self.out.bytes >= most)
    }
}

struct SoundTrack {
    last: Option<Instant>,
    ended: bool,
}

const VIDEO_PAD: usize = 0;
const SOUND_PAD: usize = 1;

pub(crate) struct Recorder {
    control: RecordControl,
    video_caps: Option<Caps>,
    sound_caps: Option<Caps>,
    sound_seen: Option<Instant>,
    sound_unreliable: bool,
    parameter_sets: ParameterSets,
    file: Option<OpenFile>,
    /// Oldest first, from a keyframe.
    held: VecDeque<Held>,
    held_bytes: usize,
}

impl Recorder {
    pub(crate) fn new(control: RecordControl) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            control,
            video_caps: None,
            sound_caps: None,
            sound_seen: None,
            sound_unreliable: false,
            parameter_sets: ParameterSets::default(),
            file: None,
            held: VecDeque::new(),
            held_bytes: 0,
        }))
    }

    fn note_caps(&mut self, track: Track, caps: &Caps) {
        match track {
            Track::Video => {
                // Another size needs another header: a new file, and nothing held of the old size.
                let size = fixed_size(caps);
                if size.is_some()
                    && let Some(file) = &self.file
                    && file.size.is_some()
                    && file.size != size
                {
                    self.close();
                }
                if size.is_some() && self.video_caps.as_ref().and_then(fixed_size) != size {
                    self.let_go();
                }
                self.video_caps = Some(caps.clone());
            }
            Track::Sound => {
                if matches!(
                    caps,
                    Caps::Audio {
                        format: AudioFormat::Aac,
                        ..
                    }
                ) {
                    self.sound_caps = Some(caps.clone());
                }
            }
        }
    }

    fn offer(&mut self, track: Track, frame: &Frame) {
        match track {
            Track::Video => self.offer_video(frame),
            Track::Sound => self.offer_sound(frame),
        }
    }

    fn offer_video(&mut self, frame: &Frame) {
        let codec = match &self.video_caps {
            Some(Caps::CompressedVideo { codec, .. }) => *codec,
            _ => return,
        };
        let Some(bytes) = frame.domain.as_system_slice() else {
            return;
        };
        let carries_parameter_sets = self.parameter_sets.refresh(codec, bytes);
        let plan = self.control.plan();
        self.hold(
            Track::Video,
            frame,
            bytes.len(),
            carries_parameter_sets,
            plan.preroll,
        );

        let Some(next_file) = plan.target else {
            self.close();
            return;
        };
        self.check_sound();

        if frame.timing.keyframe
            && self
                .file
                .as_ref()
                .is_some_and(|file| file.due(&plan.rules, frame.timing.pts_ns))
        {
            self.close();
            if self.control.target().is_none() {
                // The file could not be finished, and recording stopped.
                return;
            }
        }

        if self.file.is_none() {
            if self.control.take_preroll() && self.open_from_held(&next_file, codec) {
                return;
            }
            if !frame.timing.keyframe || !self.parameter_sets.complete(codec) {
                return;
            }
            if !self.open_named(&next_file, SystemTime::now(), frame.timing.pts_ns) {
                return;
            }
        }
        self.put_video(frame, carries_parameter_sets);
    }

    fn open_named(&mut self, next_file: &NextFile, at: SystemTime, start_pts: u64) -> bool {
        let path = next_file(at);
        if let Err(why) = self.open(path.clone(), start_pts) {
            // Naming the file may have made it empty: don't leave a recording of nothing.
            if std::fs::metadata(&path).is_ok_and(|file| file.len() == 0) {
                let _ = std::fs::remove_file(&path);
            }
            self.control.failed(why);
            return false;
        }
        true
    }

    /// False when nothing held can start a file, so the next keyframe opens one instead.
    fn open_from_held(&mut self, next_file: &NextFile, codec: g2g_core::VideoCodec) -> bool {
        let Some(first) = self.held.front() else {
            return false;
        };
        if first.track != Track::Video
            || !first.frame.timing.keyframe
            || !self.parameter_sets.complete(codec)
        {
            return false;
        }
        let (at, start_pts) = (first.at, first.frame.timing.pts_ns);
        if !self.open_named(next_file, at, start_pts) {
            return true;
        }
        let held: Vec<(Track, Frame, bool)> = self
            .held
            .iter()
            .map(|held| (held.track, held.frame.share(), held.carries_parameter_sets))
            .collect();
        for (track, frame, carries_parameter_sets) in held {
            if self.file.is_none() {
                // A write failed, and recording stopped.
                break;
            }
            match track {
                Track::Video => self.put_video(&frame, carries_parameter_sets),
                Track::Sound => self.put_sound(&frame),
            }
        }
        true
    }

    fn put_video(&mut self, frame: &Frame, carries_parameter_sets: bool) {
        let first = self
            .file
            .as_ref()
            .is_some_and(|file| file.out.bytes == 0 && file.start_pts == frame.timing.pts_ns);
        let copy = match frame.domain.as_system_slice() {
            // The muxer reads the parameter sets from the file's first frame.
            Some(bytes) if first && !carries_parameter_sets => {
                with_bytes(frame, self.parameter_sets.prepend(bytes))
            }
            _ => frame.share(),
        };
        let pts = frame.timing.pts_ns;
        self.write(VIDEO_PAD, PipelinePacket::DataFrame(copy));
        if let Some(file) = &mut self.file
            && pts > file.last_pts
        {
            file.frame_ns = pts - file.last_pts;
            file.last_pts = pts;
        }
    }

    fn offer_sound(&mut self, frame: &Frame) {
        self.sound_seen = Some(Instant::now());
        let bytes = frame.domain.as_system_slice().map_or(0, <[u8]>::len);
        let preroll = self.control.preroll();
        self.hold(Track::Sound, frame, bytes, false, preroll);
        self.put_sound(frame);
    }

    fn put_sound(&mut self, frame: &Frame) {
        let Some(file) = &mut self.file else { return };
        let Some(sound) = &mut file.sound else { return };
        if sound.ended || frame.timing.pts_ns < file.start_pts {
            return;
        }
        sound.last = Some(Instant::now());
        self.write(SOUND_PAD, PipelinePacket::DataFrame(frame.share()));
    }

    /// Keeps whole keyframe intervals reaching back `preroll`, up to [`PREROLL_MOST_BYTES`].
    fn hold(
        &mut self,
        track: Track,
        frame: &Frame,
        bytes: usize,
        carries_parameter_sets: bool,
        preroll: Option<Duration>,
    ) {
        let Some(length) = preroll else {
            self.let_go();
            return;
        };
        let keyframe = track == Track::Video && frame.timing.keyframe;
        if self.held.is_empty() && !keyframe {
            return;
        }
        self.held.push_back(Held {
            track,
            frame: frame.share(),
            bytes,
            arrived: Instant::now(),
            at: SystemTime::now(),
            carries_parameter_sets,
        });
        self.held_bytes += bytes;

        while let Some(next) = self
            .held
            .iter()
            .skip(1)
            .position(|held| held.track == Track::Video && held.frame.timing.keyframe)
            .map(|at| at + 1)
        {
            let reaches = self.held[next].arrived.elapsed() >= length;
            if !reaches && self.held_bytes <= PREROLL_MOST_BYTES {
                break;
            }
            for gone in self.held.drain(..next) {
                self.held_bytes -= gone.bytes;
            }
        }
        if self.held_bytes > PREROLL_MOST_BYTES {
            // One keyframe interval holds more than may be held: hold none of it.
            self.let_go();
        }
    }

    fn let_go(&mut self) {
        self.held.clear();
        self.held_bytes = 0;
    }

    /// Gives up on sound that never came, which would otherwise hold every picture back in memory.
    fn check_sound(&mut self) {
        let Some(file) = &mut self.file else { return };
        let Some(sound) = &mut file.sound else { return };
        if sound.ended {
            return;
        }
        match sound.last {
            Some(last) if last.elapsed() >= SOUND_STALLED => {
                sound.ended = true;
                self.write(SOUND_PAD, PipelinePacket::Eos);
            }
            None if file.opened.elapsed() >= SOUND_STALLED => {
                // The muxer writes nothing until every track starts: start again without sound.
                self.discard();
                self.sound_unreliable = true;
            }
            _ => {}
        }
    }

    fn discard(&mut self) {
        if let Some(file) = self.file.take() {
            drop(file.out);
            let _ = std::fs::remove_file(&file.path);
            self.control.closed();
        }
    }

    fn open(&mut self, path: PathBuf, start_pts: u64) -> Result<(), String> {
        let video = self
            .video_caps
            .clone()
            .ok_or("the video has no format yet")?;
        let sound = self
            .sound_caps
            .clone()
            .filter(|_| !self.sound_unreliable)
            .filter(|_| {
                self.sound_seen
                    .is_some_and(|seen| seen.elapsed() < SOUND_IS_FLOWING)
            });

        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)
                .map_err(|error| format!("{}: {error}", folder.display()))?;
        }
        let out = FileOut::create(&path)?;
        let mut mux = Mp4MuxN::new(if sound.is_some() { 2 } else { 1 })
            .with_fragment_duration_ms(FRAGMENT_MS);
        let refused = |error| format!("the recording could not start: {error:?}");
        MultiInputElement::configure_pipeline(&mut mux, VIDEO_PAD, &video).map_err(refused)?;
        if let Some(sound) = &sound {
            MultiInputElement::configure_pipeline(&mut mux, SOUND_PAD, sound).map_err(refused)?;
        }

        self.control.set_status(RecordStatus::Writing(path.clone()));
        self.file = Some(OpenFile {
            path,
            mux,
            out,
            start_pts,
            last_pts: start_pts,
            frame_ns: 0,
            opened: Instant::now(),
            size: fixed_size(&video),
            sound: sound.map(|_| SoundTrack {
                last: None,
                ended: false,
            }),
        });
        Ok(())
    }

    fn write(&mut self, pad: usize, packet: PipelinePacket) {
        let Some(file) = &mut self.file else { return };
        let result = at_once(MultiInputElement::process(
            &mut file.mux,
            pad,
            packet,
            &mut file.out,
        ));
        if let Err(error) = result {
            let why = file
                .out
                .error
                .take()
                .unwrap_or_else(|| format!("the muxer failed: {error:?}"));
            let path = file.path.display().to_string();
            self.file = None;
            self.control.failed(format!("{path}: {why}"));
        }
    }

    pub(crate) fn close(&mut self) {
        let Some(mut file) = self.file.take() else {
            return;
        };
        let mut pads = vec![VIDEO_PAD];
        match &file.sound {
            // Sound never started, so the muxer wrote nothing: the file is empty.
            Some(SoundTrack { last: None, .. }) => {
                self.file = Some(file);
                self.discard();
                return;
            }
            Some(SoundTrack { ended: false, .. }) => pads.push(SOUND_PAD),
            _ => {}
        }
        for pad in pads {
            let ended = at_once(MultiInputElement::process(
                &mut file.mux,
                pad,
                PipelinePacket::Eos,
                &mut file.out,
            ));
            if let Err(error) = ended {
                let why = file
                    .out
                    .error
                    .take()
                    .unwrap_or_else(|| format!("the muxer failed: {error:?}"));
                self.control
                    .failed(format!("{}: {why}", file.path.display()));
                return;
            }
        }
        let length_ns = file.last_pts + file.frame_ns - file.start_pts;
        if let Err(error) = file
            .out
            .write_length(length_ns)
            .and_then(|()| file.out.file.sync_all())
        {
            self.control
                .failed(format!("{}: {error}", file.path.display()));
            return;
        }
        self.control.closed();
    }

    pub(crate) fn check(&mut self) {
        if self.control.target().is_none() && self.file.is_some() {
            self.close();
        }
    }
}

pub(crate) struct RecordTap {
    track: Track,
    recorder: Arc<Mutex<Recorder>>,
}

impl RecordTap {
    pub(crate) fn new(track: Track, recorder: Arc<Mutex<Recorder>>) -> Tap<Self> {
        Tap(Self { track, recorder })
    }

    fn recorder(&self) -> MutexGuard<'_, Recorder> {
        self.recorder.locked()
    }
}

impl Watch for RecordTap {
    fn metadata(&self) -> ElementMetadata {
        ElementMetadata::new(
            "IP Camera record tap",
            "Filter",
            "Passes the stream on, copying it into the recording while one runs.",
            "Drifting Prompts",
        )
    }

    fn caps(&self, caps: &Caps) {
        self.recorder().note_caps(self.track, caps);
    }

    fn frame(&self, frame: &Frame) {
        self.recorder().offer(self.track, frame);
    }
}

/// One poll finishes a muxer call, since [`FileOut`] never waits; a wait would leave it half way.
fn at_once<F: Future<Output = Result<(), G2gError>>>(future: F) -> Result<(), G2gError> {
    let future = pin!(future);
    match future.poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => Err(G2gError::Timeout),
    }
}

fn with_bytes(frame: &Frame, bytes: Vec<u8>) -> Frame {
    Frame::new(
        MemoryDomain::System(SystemSlice::from_boxed(bytes.into_boxed_slice())),
        frame.timing,
        frame.sequence,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use g2g_core::{
        BitstreamAlignment, ChannelLayout, Colorimetry, Dim, FrameTiming, Rate, VideoCodec,
    };
    use tempfile::TempDir;

    fn video_caps() -> Caps {
        Caps::CompressedVideo {
            codec: VideoCodec::H264,
            width: Dim::Fixed(64),
            height: Dim::Fixed(64),
            framerate: Rate::Any,
            colorimetry: Colorimetry::UNKNOWN,
            alignment: BitstreamAlignment::Unspecified,
        }
    }

    fn sound_caps() -> Caps {
        Caps::Audio {
            format: AudioFormat::Aac,
            channels: 1,
            sample_rate: 8_000,
            channel_layout: ChannelLayout::UNSPECIFIED,
        }
    }

    const SPS: &[u8] = &[
        0x67, 0x42, 0xc0, 0x0a, 0xda, 0x0f, 0xd0, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00, 0x00,
        0x1e, 0x47, 0x89, 0x16, 0xa8,
    ];
    const PPS: &[u8] = &[0x68, 0xce, 0x3c, 0x80];

    fn annex_b(nals: &[&[u8]]) -> Vec<u8> {
        nals.iter()
            .flat_map(|nal| [&[0u8, 0, 0, 1][..], nal].concat())
            .collect()
    }

    fn frame(bytes: Vec<u8>, pts_ms: u64, keyframe: bool) -> Frame {
        Frame::new(
            MemoryDomain::System(SystemSlice::from_boxed(bytes.into_boxed_slice())),
            FrameTiming {
                pts_ns: pts_ms * 1_000_000,
                dts_ns: pts_ms * 1_000_000,
                duration_ns: 40_000_000,
                capture_ns: 0,
                arrival_ns: 0,
                keyframe,
            },
            0,
        )
    }

    fn keyframe(pts_ms: u64, with_parameter_sets: bool) -> Frame {
        let idr: &[u8] = &[0x65, 0x88, 0x84, 0x00, 0x33];
        let nals: Vec<&[u8]> = if with_parameter_sets {
            vec![SPS, PPS, idr]
        } else {
            vec![idr]
        };
        frame(annex_b(&nals), pts_ms, true)
    }

    fn delta(pts_ms: u64) -> Frame {
        frame(annex_b(&[&[0x41, 0x9a, 0x02, 0x10]]), pts_ms, false)
    }

    fn adts(pts_ms: u64) -> Frame {
        let payload = [0x21u8, 0x10, 0x05];
        let length = 7 + payload.len();
        let header = [
            0xff,
            0xf1,
            // AAC LC, 8 kHz (index 11), mono.
            (1 << 6) | (11 << 2),
            (1 << 6) | ((length >> 11) as u8 & 0x03),
            ((length >> 3) & 0xff) as u8,
            (((length & 0x07) << 5) as u8) | 0x1f,
            0xfc,
        ];
        frame([&header[..], &payload].concat(), pts_ms, true)
    }

    fn naming(folder: &TempDir) -> NextFile {
        let folder = folder.path().to_path_buf();
        let next = std::sync::atomic::AtomicU32::new(0);
        Arc::new(move |_| {
            let number = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            folder.join(format!("clip-{number}.mp4"))
        })
    }

    fn recorder_into(folder: &TempDir) -> (RecordControl, Arc<Mutex<Recorder>>) {
        let control = RecordControl::default();
        let recorder = Recorder::new(control.clone());
        control.start(naming(folder), RecordRules::default());
        (control, recorder)
    }

    fn count(haystack: &[u8], needle: &[u8]) -> usize {
        haystack
            .windows(needle.len())
            .filter(|window| *window == needle)
            .count()
    }

    #[test]
    fn a_recording_waits_for_a_keyframe_and_writes_a_playable_header() {
        let folder = TempDir::new().unwrap();
        let (control, recorder) = recorder_into(&folder);
        let mut recorder = recorder.lock().unwrap();
        recorder.note_caps(Track::Video, &video_caps());

        recorder.offer(Track::Video, &delta(0));
        assert_eq!(
            control.status(),
            RecordStatus::Waiting,
            "nothing can start mid-picture"
        );

        for at in 0..75 {
            let pts = 40 + at * 40;
            let picture = if at % 25 == 0 {
                keyframe(pts, true)
            } else {
                delta(pts)
            };
            recorder.offer(Track::Video, &picture);
        }
        let RecordStatus::Writing(path) = control.status() else {
            panic!("it should be writing, not {:?}", control.status());
        };

        control.stop();
        recorder.check();
        assert_eq!(control.status(), RecordStatus::Off);

        let bytes = std::fs::read(&path).expect("the file should be there");
        assert_eq!(&bytes[4..8], b"ftyp");
        assert_eq!(count(&bytes, b"moov"), 1);
        assert_eq!(count(&bytes, b"trak"), 1, "video only");
        assert_eq!(
            count(&bytes, b"moof"),
            3,
            "a fragment a second, each closed at a keyframe"
        );
        assert_eq!(
            movie_length(&bytes),
            (3_000, 3_000),
            "75 frames of 40 ms, in milliseconds"
        );
    }

    fn movie_length(bytes: &[u8]) -> (u64, u64) {
        let at = |kind: &[u8]| {
            bytes
                .windows(4)
                .position(|window| window == kind)
                .expect("the box should be there")
                - 4
        };
        let mehd = at(b"mehd");
        let mvhd = at(b"mvhd");
        let wide = u64::from_be_bytes(bytes[mehd + 12..mehd + 20].try_into().unwrap());
        let narrow = u32::from_be_bytes(bytes[mvhd + 24..mvhd + 28].try_into().unwrap());
        (wide, u64::from(narrow))
    }

    #[test]
    fn sound_goes_in_beside_the_picture() {
        let folder = TempDir::new().unwrap();
        let (control, recorder) = recorder_into(&folder);
        let mut recorder = recorder.lock().unwrap();
        recorder.note_caps(Track::Video, &video_caps());
        recorder.note_caps(Track::Sound, &sound_caps());

        recorder.offer(Track::Sound, &adts(0));
        recorder.offer(Track::Video, &keyframe(0, true));
        for at in 1..20 {
            recorder.offer(Track::Sound, &adts(at * 128));
            recorder.offer(Track::Video, &delta(at * 128));
        }
        let RecordStatus::Writing(path) = control.status() else {
            panic!("it should be writing");
        };
        control.stop();
        recorder.check();

        let bytes = std::fs::read(&path).expect("the file should be there");
        assert_eq!(count(&bytes, b"trak"), 2, "a video track and a sound track");
        assert_eq!(count(&bytes, b"mp4a"), 1);
        assert_eq!(count(&bytes, b"avc1"), 1);
    }

    #[test]
    fn a_keyframe_without_parameter_sets_gets_them_from_earlier() {
        let folder = TempDir::new().unwrap();
        let control = RecordControl::default();
        let recorder = Recorder::new(control.clone());
        let mut recorder = recorder.lock().unwrap();
        recorder.note_caps(Track::Video, &video_caps());
        recorder.offer(Track::Video, &keyframe(0, true));
        assert_eq!(control.status(), RecordStatus::Off);

        control.start(naming(&folder), RecordRules::default());
        recorder.offer(Track::Video, &keyframe(40, false));
        recorder.offer(Track::Video, &delta(80));
        let RecordStatus::Writing(path) = control.status() else {
            panic!("it should be writing, not {:?}", control.status());
        };
        control.stop();
        recorder.check();

        let bytes = std::fs::read(&path).expect("the file should be there");
        assert_eq!(
            count(&bytes, b"avcC"),
            1,
            "the header holds the parameter sets"
        );
    }

    #[test]
    fn sound_that_never_comes_gives_way_to_the_picture() {
        let folder = TempDir::new().unwrap();
        let (control, recorder) = recorder_into(&folder);
        let mut recorder = recorder.lock().unwrap();
        recorder.note_caps(Track::Video, &video_caps());
        recorder.note_caps(Track::Sound, &sound_caps());
        recorder.offer(Track::Sound, &adts(0));
        recorder.offer(Track::Video, &keyframe(0, true));
        assert!(matches!(control.status(), RecordStatus::Writing(_)));

        recorder.file.as_mut().unwrap().opened -= SOUND_STALLED;
        recorder.offer(Track::Video, &delta(40));
        assert_eq!(
            control.status(),
            RecordStatus::Waiting,
            "the soundless file went"
        );

        recorder.offer(Track::Video, &keyframe(80, true));
        recorder.offer(Track::Video, &delta(120));
        let RecordStatus::Writing(path) = control.status() else {
            panic!("it should be writing again");
        };
        control.stop();
        recorder.check();
        let bytes = std::fs::read(&path).expect("the file should be there");
        assert_eq!(count(&bytes, b"trak"), 1, "video only from then on");
        assert_eq!(
            std::fs::read_dir(folder.path()).unwrap().count(),
            1,
            "the empty file was removed"
        );
    }

    #[test]
    fn a_file_that_cannot_be_made_stops_the_recording_and_says_why() {
        let folder = TempDir::new().unwrap();
        let blocked = folder.path().join("videos");
        std::fs::write(&blocked, "in the way").unwrap();

        let control = RecordControl::default();
        let recorder = Recorder::new(control.clone());
        let mut recorder = recorder.lock().unwrap();
        let target = blocked.join("clip.mp4");
        control.start(Arc::new(move |_| target.clone()), RecordRules::default());
        recorder.note_caps(Track::Video, &video_caps());
        recorder.offer(Track::Video, &keyframe(0, true));

        let RecordStatus::Failed(why) = control.status() else {
            panic!("it should have failed, not {:?}", control.status());
        };
        assert!(why.contains("videos"), "{why}");
        recorder.offer(Track::Video, &keyframe(40, true));
        assert!(
            matches!(control.status(), RecordStatus::Failed(_)),
            "it does not try again"
        );
        recorder.offer(Track::Video, &delta(80));
        assert!(
            matches!(control.status(), RecordStatus::Failed(_)),
            "the reason stays"
        );
    }

    fn offer_seconds(recorder: &mut Recorder, from_ms: u64, seconds: u64) {
        for at in 0..seconds * 25 {
            let pts = from_ms + at * 40;
            let picture = if at % 25 == 0 {
                keyframe(pts, true)
            } else {
                delta(pts)
            };
            recorder.offer(Track::Video, &picture);
        }
    }

    fn files_in(folder: &TempDir) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(folder.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_recording_from_the_preroll_starts_with_what_was_held_back() {
        let folder = TempDir::new().unwrap();
        let control = RecordControl::default();
        let recorder = Recorder::new(control.clone());
        let mut recorder = recorder.lock().unwrap();
        recorder.note_caps(Track::Video, &video_caps());
        control.set_preroll(Some(Duration::from_secs(60)));

        recorder.offer(Track::Video, &delta(0));
        assert!(recorder.held.is_empty());
        offer_seconds(&mut recorder, 40, 3);
        assert_eq!(control.status(), RecordStatus::Off, "nothing is written");

        let named = Arc::new(Mutex::new(None));
        recorder.held[0].at = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let seen = named.clone();
        let target = folder.path().join("clip.mp4");
        control.start(
            Arc::new(move |at| {
                *seen.lock().unwrap() = Some(at);
                target.clone()
            }),
            RecordRules {
                from_preroll: true,
                ..RecordRules::default()
            },
        );
        recorder.offer(Track::Video, &delta(3_040));
        assert!(matches!(control.status(), RecordStatus::Writing(_)));
        assert_eq!(
            *named.lock().unwrap(),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(100))
        );
        assert_eq!(
            recorder.file.as_ref().unwrap().start_pts,
            40_000_000,
            "it opened at the oldest keyframe held, not the next"
        );

        offer_seconds(&mut recorder, 3_080, 1);
        control.stop();
        recorder.check();
        let bytes = std::fs::read(folder.path().join("clip.mp4")).unwrap();
        assert_eq!(
            movie_length(&bytes),
            (4_040, 4_040),
            "three seconds held, then the rest"
        );
    }

    #[test]
    fn what_is_held_is_whole_keyframe_intervals_reaching_back_far_enough() {
        let control = RecordControl::default();
        let recorder = Recorder::new(control.clone());
        let mut recorder = recorder.lock().unwrap();
        recorder.note_caps(Track::Video, &video_caps());
        control.set_preroll(Some(Duration::from_secs(1)));

        offer_seconds(&mut recorder, 0, 2);
        for held in &mut recorder.held {
            held.arrived -= Duration::from_secs(10);
        }
        recorder.offer(Track::Video, &keyframe(2_000, true));
        assert_eq!(
            recorder.held.front().unwrap().frame.timing.pts_ns,
            1_000_000_000,
            "the first second went: the second alone reaches back a second"
        );
        assert_eq!(recorder.held.len(), 26);

        control.set_preroll(None);
        recorder.offer(Track::Video, &delta(2_040));
        assert!(recorder.held.is_empty());
        assert_eq!(recorder.held_bytes, 0);
    }

    #[test]
    fn a_file_that_runs_its_length_goes_on_in_the_next_from_the_same_keyframe() {
        let folder = TempDir::new().unwrap();
        let control = RecordControl::default();
        let recorder = Recorder::new(control.clone());
        let mut recorder = recorder.lock().unwrap();
        recorder.note_caps(Track::Video, &video_caps());
        control.start(
            naming(&folder),
            RecordRules {
                split_after: Some(Duration::from_secs(1)),
                ..RecordRules::default()
            },
        );
        offer_seconds(&mut recorder, 0, 3);
        assert!(matches!(control.status(), RecordStatus::Writing(_)));
        control.stop();
        recorder.check();

        assert_eq!(
            files_in(&folder),
            ["clip-0.mp4", "clip-1.mp4", "clip-2.mp4"]
        );
        for name in files_in(&folder) {
            let bytes = std::fs::read(folder.path().join(&name)).unwrap();
            assert_eq!(movie_length(&bytes), (1_000, 1_000), "{name}");
        }
    }

    #[test]
    fn a_file_that_grows_too_large_goes_on_in_the_next() {
        let folder = TempDir::new().unwrap();
        let control = RecordControl::default();
        let recorder = Recorder::new(control.clone());
        let mut recorder = recorder.lock().unwrap();
        recorder.note_caps(Track::Video, &video_caps());
        control.start(
            naming(&folder),
            RecordRules {
                split_bytes: Some(1),
                ..RecordRules::default()
            },
        );
        offer_seconds(&mut recorder, 0, 4);
        control.stop();
        recorder.check();
        assert!(
            files_in(&folder).len() > 1,
            "a keyframe after the first bytes are written starts the next file"
        );
        assert!(
            !matches!(control.status(), RecordStatus::Failed(_)),
            "it goes on, not stops"
        );
    }
}
