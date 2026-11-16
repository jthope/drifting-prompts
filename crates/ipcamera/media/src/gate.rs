//! The decode gate: a live view that stays connected while nothing on screen shows it, without
//! being decoded.
//!
//! Decoding is most of what a camera costs. A live view can stay connected while no tile shows it
//! ([`MediaSession::set_shown`]), so its status stays current and its picture comes back at once.
//! The gate sits between the size tap and the decoder ([`crate::pipeline`]), after the record tap,
//! so a recording never notices it. While the view is shown it passes everything on. While it is
//! hidden it hands the decoder nothing. The stream still arrives: the session stays live, a
//! recording goes on, and a camera that stops sending is still lost after the RTSP source's stall
//! timeout.
//!
//! Coming back must not wait for the camera's next keyframe, which can be seconds away, and must
//! never hand the decoder a picture whose keyframe it missed: a decoding error ends the run, and
//! the session reconnects. So while hidden the gate keeps the pictures since the last keyframe, as
//! the relay keeps them for a late viewer (`relay`), and the newest parameter sets. Shown again, it
//! sends that keyframe, with the parameter sets ahead of it if it came without them, then the
//! pictures after it, and the decoder catches up in one burst. The sink keeps the keyframe's
//! picture and skips the rest of the burst ([`Visibility::skips`]) until the newest, so the tile
//! has a picture at once and the live one a moment later, and the burst doesn't count as frames
//! the window never drew.
//!
//! While hidden, the frame slot stays empty, so a view nobody sees holds no decoded picture (8 MB
//! at 1080p). The one exception is the picture a recording's file starts on, which its thumbnail
//! is made from ([`Visibility::want_picture`]): the gate lets that keyframe through.
//!
//! [`MediaSession::set_shown`]: crate::MediaSession::set_shown

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

/// The most pictures the gate keeps while hidden, as many as the relay keeps for a late viewer. A
/// camera whose keyframes are further apart than this comes back at its next keyframe instead.
const HELD: usize = 300;

/// Whether a live view is shown, shared by the session that is told, the gate that acts on it and
/// the sink that keeps its pictures.
#[derive(Default)]
pub(crate) struct Visibility {
    /// Where the pictures go.
    frames: FrameSlot,
    hidden: AtomicBool,
    /// Set while hidden when the next keyframe's picture is wanted: the one a recording's new file
    /// starts on.
    wanted: AtomicBool,
    /// While the decoder catches up, the presentation time of the newest picture it was sent.
    catch_up: Mutex<Option<u64>>,
}

impl Visibility {
    /// Shown, putting its pictures in `frames`.
    pub(crate) fn new(frames: FrameSlot) -> Self {
        Self {
            frames,
            ..Self::default()
        }
    }

    /// The slot the pictures go in.
    pub(crate) fn frames(&self) -> FrameSlot {
        self.frames.clone()
    }

    /// Whether anything shows the pictures.
    pub(crate) fn shown(&self) -> bool {
        !self.hidden.load(Ordering::SeqCst)
    }

    /// Shows the pictures again, or hides them and lets go of the one in the slot.
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

    /// Notes the newest picture the decoder was sent to catch up, or that it has nothing to catch
    /// up through.
    fn catch_up_to(&self, newest: Option<u64>) {
        *self.catch_up.locked() = newest;
    }

    /// Whether the sink skips the picture shown at `pts`: one the decoder is catching up through,
    /// once the slot has a picture.
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

    /// Puts a decoded picture in the slot. While hidden it is let go at once, unless it was wanted.
    pub(crate) fn store(&self, picture: Arc<DecodedFrame>) {
        self.frames.store(picture);
        let wanted = self.wanted.swap(false, Ordering::SeqCst);
        // Checked after the store: a hide that lands in between empties the slot itself.
        if !self.shown() && !wanted {
            self.frames.clear();
        }
    }
}

/// Hands the decoder the stream while the view is shown, and nothing while it is hidden.
pub(crate) struct DecodeGate {
    visibility: Arc<Visibility>,
    /// Set by the first keyframe while hidden, so the session reports the camera live although
    /// nothing is decoded.
    live: Arc<AtomicBool>,
    codec: Option<VideoCodec>,
    parameter_sets: ParameterSets,
    /// Whether the decoder has had everything since a keyframe, so the next picture can go to it.
    in_step: bool,
    /// While out of step: the pictures since the last keyframe, that keyframe first.
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

    /// What goes on to the decoder now that `frame` has arrived: nothing, `frame`, or the pictures
    /// since the last keyframe.
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
            // Nothing to decode it against, or too far from its keyframe to catch up through:
            // wait for the next keyframe.
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

    /// The pictures since the last keyframe, all at once, after which the decoder is in step.
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

    /// `keyframe` with the newest parameter sets ahead of it, when there are all of them.
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

/// A frame of `bytes` with `like`'s timing.
fn system_frame(bytes: Box<[u8]>, like: &Frame) -> Frame {
    Frame::new(
        MemoryDomain::System(SystemSlice::from_boxed(bytes)),
        like.timing,
        like.sequence,
    )
}

/// A copy of `frame`, which the gate both sends on and keeps.
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

    /// The SPS and PPS of a 64 x 64 H.264 stream, and a picture of each kind.
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

    /// A keyframe that carries its parameter sets, as cameras send them.
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

    /// The times, in ms, of what went on to the decoder.
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
