//! The decode gate: a hidden live view stays connected but hands the decoder nothing.
//!
//! The decoder must never get a picture whose keyframe it missed (a decoding error ends the run),
//! so while hidden the gate keeps the pictures since the last keyframe and replays them when shown.

use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use g2g_core::{
    AsyncElement, Caps, ConfigureOutcome, ElementMetadata, Frame, FrameTiming, G2gError,
    MemoryDomain, OutputSink, PipelinePacket, SystemSlice, VideoCodec,
};
use ipcamera_core::sync::LockExt;

use crate::{DecodedFrame, FrameSlot, annexb::ParameterSets};

/// Keyframes further apart than this come back at the next keyframe instead.
const HELD: usize = 300;

#[derive(Default)]
pub(crate) struct Visibility {
    frames: FrameSlot,
    hidden: AtomicBool,
    wanted: AtomicBool,
    /// While the decoder catches up: the newest picture's presentation time.
    catch_up: Mutex<Option<u64>>,
}

impl Visibility {
    pub(crate) fn new(frames: FrameSlot) -> Self {
        Self {
            frames,
            ..Self::default()
        }
    }

    pub(crate) fn frames(&self) -> FrameSlot {
        self.frames.clone()
    }

    pub(crate) fn shown(&self) -> bool {
        !self.hidden.load(Ordering::SeqCst)
    }

    pub(crate) fn set_shown(&self, shown: bool) {
        self.hidden.store(!shown, Ordering::SeqCst);
        if !shown {
            *self.catch_up.locked() = None;
            self.frames.clear();
        }
    }

    /// Asks for the next keyframe's picture, even while hidden.
    pub(crate) fn want_picture(&self) {
        self.wanted.store(true, Ordering::SeqCst);
    }

    fn wants_picture(&self) -> bool {
        self.wanted.load(Ordering::SeqCst)
    }

    fn catch_up_to(&self, newest: Option<u64>) {
        *self.catch_up.locked() = newest;
    }

    /// Skips the pictures the decoder catches up through, once the slot has one.
    pub(crate) fn skips(&self, pts: u64) -> bool {
        let mut catch_up = self.catch_up.locked();
        match *catch_up {
            Some(newest) if pts < newest => self.frames.latest().is_some(),
            Some(_) => {
                *catch_up = None;
                false
            }
            None => false,
        }
    }

    /// While hidden the picture is let go at once, unless it was wanted.
    pub(crate) fn store(&self, picture: Arc<DecodedFrame>) {
        self.frames.store(picture);
        let wanted = self.wanted.swap(false, Ordering::SeqCst);
        // Checked after the store: a hide that lands in between empties the slot itself.
        if !self.shown() && !wanted {
            self.frames.clear();
        }
    }
}

pub(crate) struct DecodeGate {
    visibility: Arc<Visibility>,
    live: Arc<AtomicBool>,
    codec: Option<VideoCodec>,
    parameter_sets: ParameterSets,
    /// The decoder has had everything since a keyframe.
    in_step: bool,
    /// Since the last keyframe, that keyframe first.
    held: VecDeque<Frame>,
}

impl DecodeGate {
    pub(crate) fn new(visibility: Arc<Visibility>, live: Arc<AtomicBool>) -> Self {
        Self {
            visibility,
            live,
            codec: None,
            parameter_sets: ParameterSets::default(),
            in_step: false,
            held: VecDeque::new(),
        }
    }

    fn take_caps(&mut self, caps: &Caps) {
        if let Caps::CompressedVideo { codec, .. } = caps {
            self.codec = Some(*codec);
        }
    }

    fn admit(&mut self, frame: Frame) -> Vec<Frame> {
        let keyframe = frame.timing.keyframe;
        let carries_parameter_sets = match (self.codec, frame.domain.as_system_slice()) {
            (Some(codec), Some(bytes)) => self.parameter_sets.refresh(codec, bytes),
            _ => false,
        };
        let shown = self.visibility.shown();
        if shown && self.in_step {
            return vec![frame];
        }

        self.in_step = false;
        if keyframe {
            self.held.clear();
            if !shown {
                self.live.store(true, Ordering::Relaxed);
            }
        } else if self.held.is_empty() || self.held.len() >= HELD {
            // No keyframe to decode it against, or too far behind one: wait for the next.
            self.held.clear();
            return Vec::new();
        }
        let frame = if keyframe && !carries_parameter_sets {
            self.with_parameter_sets(frame)
        } else {
            frame
        };

        let mut admitted = Vec::new();
        if !shown && keyframe && self.visibility.wants_picture() {
            admitted.extend(copy(&frame));
        }
        self.held.push_back(frame);
        if shown {
            admitted.extend(self.catch_up());
        }
        admitted
    }

    fn catch_up(&mut self) -> Vec<Frame> {
        let newest = self
            .held
            .iter()
            .map(|frame| frame.timing.pts_ns)
            .filter(|&pts| pts != FrameTiming::PTS_NONE)
            .max();
        self.visibility
            .catch_up_to(newest.filter(|_| self.held.len() > 1));
        self.in_step = true;
        self.held.drain(..).collect()
    }

    fn with_parameter_sets(&self, keyframe: Frame) -> Frame {
        let complete = self
            .codec
            .is_some_and(|codec| self.parameter_sets.complete(codec));
        match keyframe.domain.as_system_slice() {
            Some(bytes) if complete => system_frame(
                self.parameter_sets.prepend(bytes).into_boxed_slice(),
                &keyframe,
            ),
            _ => keyframe,
        }
    }
}

fn system_frame(bytes: Box<[u8]>, like: &Frame) -> Frame {
    Frame::new(
        MemoryDomain::System(SystemSlice::from_boxed(bytes)),
        like.timing,
        like.sequence,
    )
}

fn copy(frame: &Frame) -> Option<Frame> {
    let bytes = frame.domain.as_system_slice()?;
    Some(system_frame(Box::from(bytes), frame))
}

impl AsyncElement for DecodeGate {
    type ProcessFuture<'a>
        = Pin<Box<dyn Future<Output = Result<(), G2gError>> + 'a>>
    where
        Self: 'a;

    fn intercept_caps(&self, upstream: &Caps) -> Result<Caps, G2gError> {
        Ok(upstream.clone())
    }

    fn configure_pipeline(&mut self, caps: &Caps) -> Result<ConfigureOutcome, G2gError> {
        self.take_caps(caps);
        Ok(ConfigureOutcome::Accepted)
    }

    fn process<'a>(
        &'a mut self,
        packet: PipelinePacket,
        out: &'a mut dyn OutputSink,
    ) -> Self::ProcessFuture<'a> {
        Box::pin(async move {
            match packet {
                PipelinePacket::DataFrame(frame) => {
                    for frame in self.admit(frame) {
                        out.push(PipelinePacket::DataFrame(frame)).await?;
                    }
                    return Ok(());
                }
                // The runner sends the one end of stream itself, so a transform must not.
                PipelinePacket::Eos => return Ok(()),
                PipelinePacket::CapsChanged(ref caps) => self.take_caps(caps),
                // The decoder starts over, so it needs a keyframe before anything else.
                PipelinePacket::Flush => {
                    self.in_step = false;
                    self.held.clear();
                }
                _ => {}
            }
            out.push(packet).await.map(|_| ())
        })
    }

    fn metadata(&self) -> ElementMetadata {
        ElementMetadata::new(
            "IP Camera decode gate",
            "Filter/Video",
            "Hands the decoder nothing while the picture is hidden, and catches it up from the last keyframe when it is shown.",
            "Drifting Prompts",
        )
    }
}

#[cfg(test)]
mod tests {
    use g2g_core::{BitstreamAlignment, Colorimetry, Dim, Rate};

    use super::*;
    use crate::annexb;

    const SPS: &[u8] = &[
        0x67, 0x42, 0xc0, 0x0a, 0xda, 0x0f, 0xd0, 0x80, 0x00, 0x00, 0x03, 0x00, 0x80, 0x00, 0x00,
        0x1e, 0x47, 0x89, 0x16, 0xa8,
    ];
    const PPS: &[u8] = &[0x68, 0xce, 0x3c, 0x80];
    const IDR: &[u8] = &[0x65, 0x88, 0x84, 0x00, 0x33];
    const SLICE: &[u8] = &[0x41, 0x9a, 0x02, 0x10];

    fn access_unit(nals: &[&[u8]], at_ms: u64, keyframe: bool) -> Frame {
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

    fn keyframe(at_ms: u64) -> Frame {
        access_unit(&[SPS, PPS, IDR], at_ms, true)
    }

    fn picture(at_ms: u64) -> Frame {
        access_unit(&[SLICE], at_ms, false)
    }

    struct Gated {
        gate: DecodeGate,
        visibility: Arc<Visibility>,
        live: Arc<AtomicBool>,
    }

    fn gated() -> Gated {
        let visibility = Arc::new(Visibility::default());
        let live = Arc::new(AtomicBool::new(false));
        let mut gate = DecodeGate::new(visibility.clone(), live.clone());
        gate.take_caps(&Caps::CompressedVideo {
            codec: VideoCodec::H264,
            width: Dim::Fixed(64),
            height: Dim::Fixed(64),
            framerate: Rate::Any,
            colorimetry: Colorimetry::UNKNOWN,
            alignment: BitstreamAlignment::Unspecified,
        });
        Gated {
            gate,
            visibility,
            live,
        }
    }

    fn times(frames: &[Frame]) -> Vec<u64> {
        frames
            .iter()
            .map(|frame| frame.timing.pts_ns / 1_000_000)
            .collect()
    }

    fn decoded() -> Arc<DecodedFrame> {
        Arc::new(DecodedFrame {
            width: 1,
            height: 1,
            bgra: vec![0; 4],
        })
    }

    #[test]
    fn a_shown_view_decodes_every_picture() {
        let mut gated = gated();

        assert_eq!(times(&gated.gate.admit(keyframe(0))), [0]);
        assert_eq!(times(&gated.gate.admit(picture(40))), [40]);
        assert_eq!(times(&gated.gate.admit(picture(80))), [80]);
    }

    #[test]
    fn a_hidden_view_decodes_nothing_and_is_live() {
        let mut gated = gated();
        gated.visibility.set_shown(false);

        assert!(gated.gate.admit(keyframe(0)).is_empty());
        assert!(gated.gate.admit(picture(40)).is_empty());
        assert!(gated.live.load(Ordering::Relaxed), "the stream is arriving");
    }

    #[test]
    fn shown_again_it_starts_at_the_last_keyframe() {
        let mut gated = gated();
        gated.gate.admit(keyframe(0));
        gated.visibility.set_shown(false);
        for frame in [picture(40), keyframe(80), picture(120)] {
            assert!(gated.gate.admit(frame).is_empty());
        }

        gated.visibility.set_shown(true);
        assert_eq!(times(&gated.gate.admit(picture(160))), [80, 120, 160]);
        assert_eq!(
            times(&gated.gate.admit(picture(200))),
            [200],
            "in step again"
        );
    }

    #[test]
    fn a_keyframe_without_parameter_sets_gets_them_from_earlier() {
        let mut gated = gated();
        gated.gate.admit(keyframe(0));
        gated.visibility.set_shown(false);
        gated.gate.admit(access_unit(&[IDR], 40, true));

        gated.visibility.set_shown(true);
        let sent = gated.gate.admit(picture(80));
        let first = sent[0].domain.as_system_slice().expect("bytes");
        assert_eq!(annexb::nal_units(first), [SPS, PPS, IDR]);
    }

    #[test]
    fn with_no_keyframe_held_it_waits_for_the_next() {
        let mut gated = gated();
        gated.visibility.set_shown(false);
        gated.gate.admit(picture(0));

        gated.visibility.set_shown(true);
        assert!(gated.gate.admit(picture(40)).is_empty());
        assert_eq!(times(&gated.gate.admit(keyframe(80))), [80]);
        assert_eq!(times(&gated.gate.admit(picture(120))), [120]);
    }

    #[test]
    fn too_far_from_its_keyframe_it_waits_for_the_next() {
        let mut gated = gated();
        gated.visibility.set_shown(false);
        gated.gate.admit(keyframe(0));
        for at in 1..=HELD as u64 {
            gated.gate.admit(picture(at * 40));
        }

        gated.visibility.set_shown(true);
        assert!(gated.gate.admit(picture(40_000)).is_empty());
        assert_eq!(times(&gated.gate.admit(keyframe(40_040))), [40_040]);
    }

    #[test]
    fn the_picture_a_recording_starts_on_is_decoded_while_hidden() {
        let mut gated = gated();
        gated.visibility.set_shown(false);
        gated.visibility.want_picture();

        assert_eq!(times(&gated.gate.admit(keyframe(0))), [0]);
        assert!(gated.gate.admit(picture(40)).is_empty());

        gated.visibility.set_shown(true);
        assert_eq!(
            times(&gated.gate.admit(picture(80))),
            [0, 40, 80],
            "the keyframe it kept as well"
        );
    }

    #[test]
    fn hiding_lets_go_of_the_picture_and_of_any_decoded_after() {
        let visibility = Visibility::default();
        visibility.store(decoded());
        assert!(visibility.frames().latest().is_some());

        visibility.set_shown(false);
        assert!(visibility.frames().latest().is_none());
        visibility.store(decoded());
        assert!(
            visibility.frames().latest().is_none(),
            "a picture still in the decoder"
        );

        visibility.want_picture();
        visibility.store(decoded());
        assert!(visibility.frames().latest().is_some(), "a wanted picture");
    }

    #[test]
    fn catching_up_shows_the_keyframe_then_skips_to_the_newest() {
        let mut gated = gated();
        gated.visibility.set_shown(false);
        for frame in [keyframe(0), picture(40), picture(80)] {
            gated.gate.admit(frame);
        }
        gated.visibility.set_shown(true);
        gated.gate.admit(picture(120));

        let visibility = &gated.visibility;
        let skips = |at_ms: u64| visibility.skips(at_ms * 1_000_000);
        assert!(!skips(0), "the slot is empty: the keyframe shows");
        visibility.store(decoded());
        assert!(skips(40));
        assert!(skips(80));
        assert!(!skips(120), "the newest");
        assert!(!skips(160), "caught up");
    }
}
