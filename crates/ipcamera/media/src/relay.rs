//! Passing a camera's compressed video on to any number of viewers, as the camera sent it.
//!
//! A viewer starts at a keyframe carrying the parameter sets, and one that falls behind skips to
//! the next keyframe rather than queue.

use std::{
    collections::VecDeque,
    future::{Ready, ready},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use g2g_core::{
    AsyncElement, Caps, ConfigureOutcome, ElementMetadata, Frame, G2gError, OutputSink,
    PipelinePacket, VideoCodec,
};
use ipcamera_core::sync::LockExt;
use tokio::sync::broadcast::{self, error::RecvError};

use crate::{
    DecodedFrame,
    annexb::{self, ParameterSets},
    error::{Result, failed},
    sink::fixed_size,
    still::{self, Backlog},
};

/// Pictures that can wait for a viewer before it counts as fallen behind.
const WAITING: usize = 64;

/// Keyframes further apart than this send late viewers to the next keyframe instead.
const BACKLOG: usize = 300;

/// What a viewer's decoder is set up with, sent before the first picture and on every change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoConfig {
    /// The WebCodecs codec string, such as `avc1.42C00A`.
    pub codec: String,
    /// The picture's width and height, when the stream declares them.
    pub size: Option<(u32, u32)>,
}

/// One access unit: one picture's compressed video.
#[derive(Debug, PartialEq, Eq)]
pub struct Picture {
    /// Whether it decodes on its own; a keyframe carries the parameter sets ahead of it.
    pub keyframe: bool,
    /// When the camera took it, in microseconds on the stream's clock.
    pub timestamp_us: u64,
    /// The access unit in Annex-B form.
    pub bytes: Vec<u8>,
}

/// What a viewer gets from the feed, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Relayed {
    /// Set the decoder up again for the pictures after this.
    Config(VideoConfig),
    /// The next picture.
    Picture(Arc<Picture>),
}

#[derive(Default)]
struct Memory {
    config: Option<VideoConfig>,
    /// Since the last keyframe, that keyframe first.
    backlog: Vec<Arc<Picture>>,
    keyframes: u64,
}

struct Inner {
    memory: Mutex<Memory>,
    sender: broadcast::Sender<Relayed>,
}

/// A relaying session's compressed video, for any number of viewers; it outlives reconnects.
#[derive(Clone)]
pub struct Feed(Arc<Inner>);

impl Default for Feed {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(WAITING);
        Self(Arc::new(Inner {
            memory: Mutex::new(Memory::default()),
            sender,
        }))
    }
}

impl Feed {
    /// A new viewer, starting with the set-up and the pictures since the last keyframe.
    pub fn watch(&self) -> Viewer {
        // Under the lock, so no picture falls between the backlog and the live ones.
        let memory = self.0.memory.locked();
        let mut pending = VecDeque::with_capacity(memory.backlog.len() + 1);
        if let Some(config) = &memory.config {
            pending.push_back(Relayed::Config(config.clone()));
        }
        pending.extend(memory.backlog.iter().cloned().map(Relayed::Picture));
        Viewer {
            pending,
            receiver: self.0.sender.subscribe(),
            awaiting_keyframe: true,
        }
    }

    /// How many viewers are watching.
    pub fn viewers(&self) -> usize {
        self.0.sender.receiver_count()
    }

    /// How many keyframes have gone out: a number to wait past for the next one.
    pub fn keyframes(&self) -> u64 {
        self.0.memory.locked().keyframes
    }

    /// The newest picture, decoded; blocks while it decodes.
    pub fn picture(&self) -> Result<DecodedFrame> {
        still::decode(self.backlog()?)
    }

    /// The last keyframe alone, decoded; blocks while it decodes.
    pub fn keyframe_picture(&self) -> Result<DecodedFrame> {
        let mut backlog = self.backlog()?;
        backlog.pictures.truncate(1);
        still::decode(backlog)
    }

    fn backlog(&self) -> Result<Backlog> {
        let memory = self.0.memory.locked();
        let config = memory
            .config
            .as_ref()
            .filter(|_| !memory.backlog.is_empty())
            .ok_or_else(|| failed!("no picture has arrived yet"))?;
        let codec =
            codec_of(config).ok_or_else(|| failed!("the stream's codec is {}", config.codec))?;
        Ok(Backlog {
            codec,
            size: config.size,
            pictures: memory.backlog.clone(),
        })
    }

    fn configure(&self, config: VideoConfig) {
        let mut memory = self.0.memory.locked();
        if memory.config.as_ref() == Some(&config) {
            return;
        }
        memory.config = Some(config.clone());
        memory.backlog.clear();
        // No receivers is no failure: nobody is watching.
        let _ = self.0.sender.send(Relayed::Config(config));
    }

    fn publish(&self, picture: Picture) {
        let picture = Arc::new(picture);
        let mut memory = self.0.memory.locked();
        if picture.keyframe || memory.backlog.len() >= BACKLOG {
            memory.backlog.clear();
        }
        if picture.keyframe {
            memory.keyframes += 1;
        }
        // A backlog only helps when it starts at a keyframe.
        if picture.keyframe || !memory.backlog.is_empty() {
            memory.backlog.push(picture.clone());
        }
        let _ = self.0.sender.send(Relayed::Picture(picture));
    }

    pub(crate) fn forget(&self) {
        self.0.memory.locked().backlog.clear();
    }
}

fn codec_of(config: &VideoConfig) -> Option<VideoCodec> {
    match config.codec.split('.').next()? {
        "avc1" | "avc3" => Some(VideoCodec::H264),
        "hev1" | "hvc1" => Some(VideoCodec::H265),
        _ => None,
    }
}

/// One viewer of a [`Feed`].
pub struct Viewer {
    pending: VecDeque<Relayed>,
    receiver: broadcast::Receiver<Relayed>,
    /// Pictures are skipped until a keyframe, since nothing before one decodes.
    awaiting_keyframe: bool,
}

impl Viewer {
    /// The next thing to send this viewer; `None` once the feed has gone.
    pub async fn next(&mut self) -> Option<Relayed> {
        loop {
            let next = match self.pending.pop_front() {
                Some(next) => next,
                None => match self.receiver.recv().await {
                    Ok(next) => next,
                    Err(RecvError::Lagged(_)) => {
                        self.awaiting_keyframe = true;
                        continue;
                    }
                    Err(RecvError::Closed) => return None,
                },
            };
            match &next {
                Relayed::Config(_) => self.awaiting_keyframe = true,
                Relayed::Picture(picture) if picture.keyframe => self.awaiting_keyframe = false,
                Relayed::Picture(_) if self.awaiting_keyframe => continue,
                Relayed::Picture(_) => {}
            }
            return Some(next);
        }
    }
}

pub(crate) struct FeedSink {
    feed: Feed,
    live: Arc<AtomicBool>,
    codec: Option<VideoCodec>,
    size: Option<(u32, u32)>,
    parameter_sets: ParameterSets,
    started: bool,
}

impl FeedSink {
    pub(crate) fn new(feed: Feed, live: Arc<AtomicBool>) -> Self {
        Self {
            feed,
            live,
            codec: None,
            size: None,
            parameter_sets: ParameterSets::default(),
            started: false,
        }
    }

    fn take_caps(&mut self, caps: &Caps) {
        if let Caps::CompressedVideo { codec, .. } = caps {
            self.codec = Some(*codec);
        }
        if let Some(size) = fixed_size(caps) {
            self.size = Some(size);
        }
    }

    fn offer(&mut self, frame: &Frame) {
        let Some(codec) = self.codec else { return };
        let Some(bytes) = frame.domain.as_system_slice() else {
            return;
        };
        let carries_parameter_sets = self.parameter_sets.refresh(codec, bytes);
        let keyframe = frame.timing.keyframe;
        if keyframe
            && self.parameter_sets.complete(codec)
            && let Some(codec) = annexb::codec_string(codec, &self.parameter_sets)
        {
            self.feed.configure(VideoConfig {
                codec,
                size: self.size,
            });
            self.started = true;
        }
        if !self.started {
            return;
        }
        let bytes = if keyframe && !carries_parameter_sets {
            self.parameter_sets.prepend(bytes)
        } else {
            bytes.to_vec()
        };
        self.feed.publish(Picture {
            keyframe,
            timestamp_us: frame.timing.pts_ns / 1_000,
            bytes,
        });
        self.live.store(true, Ordering::Relaxed);
    }
}

impl AsyncElement for FeedSink {
    type ProcessFuture<'a>
        = Ready<Result<(), G2gError>>
    where
        Self: 'a;

    fn intercept_caps(&self, upstream: &Caps) -> Result<Caps, G2gError> {
        match upstream {
            Caps::CompressedVideo {
                codec: VideoCodec::H264 | VideoCodec::H265,
                ..
            } => Ok(upstream.clone()),
            _ => Err(G2gError::CapsMismatch),
        }
    }

    fn configure_pipeline(&mut self, caps: &Caps) -> Result<ConfigureOutcome, G2gError> {
        self.take_caps(caps);
        Ok(ConfigureOutcome::Accepted)
    }

    fn process<'a>(
        &'a mut self,
        packet: PipelinePacket,
        _out: &'a mut dyn OutputSink,
    ) -> Self::ProcessFuture<'a> {
        match packet {
            PipelinePacket::CapsChanged(caps) => self.take_caps(&caps),
            PipelinePacket::DataFrame(frame) => self.offer(&frame),
            _ => {}
        }
        ready(Ok(()))
    }

    fn metadata(&self) -> ElementMetadata {
        ElementMetadata::new(
            "IP Camera feed",
            "Sink/Video",
            "Passes the compressed video on to the viewers of a relaying session.",
            "Drifting Prompts",
        )
    }
}

pub(crate) struct Discard;

impl AsyncElement for Discard {
    type ProcessFuture<'a>
        = Ready<Result<(), G2gError>>
    where
        Self: 'a;

    fn intercept_caps(&self, upstream: &Caps) -> Result<Caps, G2gError> {
        Ok(upstream.clone())
    }

    fn configure_pipeline(&mut self, _caps: &Caps) -> Result<ConfigureOutcome, G2gError> {
        Ok(ConfigureOutcome::Accepted)
    }

    fn process<'a>(
        &'a mut self,
        _packet: PipelinePacket,
        _out: &'a mut dyn OutputSink,
    ) -> Self::ProcessFuture<'a> {
        ready(Ok(()))
    }

    fn metadata(&self) -> ElementMetadata {
        ElementMetadata::new(
            "IP Camera discard",
            "Sink",
            "Drops what reaches it: a branch kept only for what taps it on the way.",
            "Drifting Prompts",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_codec_string_names_the_codec() {
        let named = |codec: &str| {
            codec_of(&VideoConfig {
                codec: codec.into(),
                size: None,
            })
        };
        assert_eq!(named("avc1.42C00A"), Some(VideoCodec::H264));
        assert_eq!(named("hev1.1.6.L93.B0"), Some(VideoCodec::H265));
        assert_eq!(named("vp09.00.10.08"), None);
    }

    #[test]
    fn a_feed_with_no_picture_has_none_to_decode() {
        let feed = Feed::default();
        assert!(feed.picture().is_err());
        feed.configure(config());
        feed.publish(picture(false, 0));
        assert!(feed.picture().is_err(), "nothing before a keyframe decodes");
        assert_eq!(feed.keyframes(), 0);
        feed.publish(picture(true, 1));
        assert_eq!(feed.keyframes(), 1);
    }

    fn picture(keyframe: bool, at: u64) -> Picture {
        Picture {
            keyframe,
            timestamp_us: at,
            bytes: vec![0, 0, 0, 1, if keyframe { 0x65 } else { 0x41 }],
        }
    }

    fn config() -> VideoConfig {
        VideoConfig {
            codec: "avc1.42C00A".into(),
            size: Some((64, 64)),
        }
    }

    fn waiting(viewer: &mut Viewer) -> Vec<Relayed> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut got = Vec::new();
        runtime.block_on(async {
            while let Ok(Some(next)) =
                tokio::time::timeout(std::time::Duration::from_millis(20), viewer.next()).await
            {
                got.push(next);
            }
        });
        got
    }

    fn times(relayed: &[Relayed]) -> Vec<u64> {
        relayed
            .iter()
            .filter_map(|next| match next {
                Relayed::Picture(picture) => Some(picture.timestamp_us),
                Relayed::Config(_) => None,
            })
            .collect()
    }

    #[test]
    fn a_late_viewer_starts_at_the_last_keyframe() {
        let feed = Feed::default();
        feed.configure(config());
        feed.publish(picture(true, 0));
        feed.publish(picture(false, 1));
        feed.publish(picture(true, 2));
        feed.publish(picture(false, 3));

        let mut viewer = feed.watch();
        feed.publish(picture(false, 4));
        let got = waiting(&mut viewer);
        assert_eq!(got.first(), Some(&Relayed::Config(config())));
        assert_eq!(times(&got), [2, 3, 4], "from the last keyframe on");
    }

    #[test]
    fn a_viewer_waits_for_a_keyframe() {
        let feed = Feed::default();
        feed.configure(config());
        feed.publish(picture(false, 0));
        let mut viewer = feed.watch();
        feed.publish(picture(false, 1));
        feed.publish(picture(true, 2));
        feed.publish(picture(false, 3));
        assert_eq!(times(&waiting(&mut viewer)), [2, 3]);
    }

    #[test]
    fn a_viewer_that_falls_behind_skips_to_the_next_keyframe() {
        let feed = Feed::default();
        feed.configure(config());
        feed.publish(picture(true, 0));
        let mut viewer = feed.watch();
        for at in 1..=(WAITING as u64 * 2) {
            feed.publish(picture(false, at));
        }
        let keyframe = WAITING as u64 * 2 + 1;
        feed.publish(picture(true, keyframe));
        feed.publish(picture(false, keyframe + 1));

        let got = times(&waiting(&mut viewer));
        assert_eq!(
            got.first(),
            Some(&0),
            "the backlog it joined with stays whole"
        );
        let after_backlog = &got[1..];
        assert_eq!(
            after_backlog,
            [keyframe, keyframe + 1],
            "what it fell behind on is skipped up to the next keyframe"
        );
    }

    #[test]
    fn a_new_config_starts_over_at_a_keyframe() {
        let feed = Feed::default();
        feed.configure(config());
        feed.publish(picture(true, 0));
        let mut viewer = feed.watch();
        let bigger = VideoConfig {
            size: Some((128, 128)),
            ..config()
        };
        feed.configure(bigger.clone());
        feed.publish(picture(false, 1));
        feed.publish(picture(true, 2));

        let got = waiting(&mut viewer);
        assert_eq!(got[2], Relayed::Config(bigger));
        assert_eq!(times(&got), [0, 2], "the picture between can't be decoded");
        assert_eq!(times(&waiting(&mut feed.watch())), [2]);
    }

    const SPS: &[u8] = &[
        0x67, 0x42, 0xc0, 0x0a, 0xda, 0x0f, 0xd0, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00, 0x00,
        0x1e, 0x47, 0x89, 0x16, 0xa8,
    ];
    const PPS: &[u8] = &[0x68, 0xce, 0x3c, 0x80];
    const IDR: &[u8] = &[0x65, 0x88, 0x84, 0x00, 0x33];
    const SLICE: &[u8] = &[0x41, 0x9a, 0x02, 0x10];

    fn access_unit(nals: &[&[u8]], at_ms: u64, keyframe: bool) -> Frame {
        use g2g_core::{FrameTiming, MemoryDomain, SystemSlice};
        let bytes: Vec<u8> = nals
            .iter()
            .flat_map(|nal| [&[0u8, 0, 0, 1][..], nal].concat())
            .collect();
        Frame::new(
            MemoryDomain::System(SystemSlice::from_boxed(bytes.into_boxed_slice())),
            FrameTiming {
                pts_ns: at_ms * 1_000_000,
                dts_ns: at_ms * 1_000_000,
                duration_ns: 40_000_000,
                capture_ns: 0,
                arrival_ns: 0,
                keyframe,
            },
            0,
        )
    }

    fn h264_caps() -> Caps {
        use g2g_core::{BitstreamAlignment, Colorimetry, Dim, Rate};
        Caps::CompressedVideo {
            codec: VideoCodec::H264,
            width: Dim::Fixed(64),
            height: Dim::Fixed(64),
            framerate: Rate::Any,
            colorimetry: Colorimetry::UNKNOWN,
            alignment: BitstreamAlignment::Unspecified,
        }
    }

    #[test]
    fn the_sink_starts_at_a_keyframe_and_gives_it_the_parameter_sets() {
        let feed = Feed::default();
        let live = Arc::new(AtomicBool::new(false));
        let mut sink = FeedSink::new(feed.clone(), live.clone());
        sink.take_caps(&h264_caps());
        let mut viewer = feed.watch();

        sink.offer(&access_unit(&[SLICE], 0, false));
        assert!(!live.load(Ordering::Relaxed), "nothing before a keyframe");
        sink.offer(&access_unit(&[SPS, PPS, IDR], 40, true));
        sink.offer(&access_unit(&[SLICE], 80, false));
        sink.offer(&access_unit(&[IDR], 120, true));
        assert!(live.load(Ordering::Relaxed));

        let got = waiting(&mut viewer);
        assert_eq!(
            got[0],
            Relayed::Config(VideoConfig {
                codec: "avc1.42C00A".into(),
                size: Some((64, 64)),
            })
        );
        assert_eq!(times(&got), [40_000, 80_000, 120_000]);
        let Relayed::Picture(last) = &got[3] else {
            panic!("a picture, not {:?}", got[3]);
        };
        let units = annexb::nal_units(&last.bytes);
        assert_eq!(units, [SPS, PPS, IDR], "the parameter sets go ahead of it");
    }
}
