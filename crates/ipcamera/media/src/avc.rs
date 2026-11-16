//! An H.264 decoder element built on `rust_h264`, a decoder written in Rust by the author of the
//! `rust_h265` that [`crate::hevc`] is built on.
//!
//! Windows' own H.264 decoder, which glass2glass's Media Foundation element creates with its
//! defaults, cost about 160 MB per camera: it held pictures back to put them in display order, and
//! started a worker thread per logical processor, each with pictures of its own. glass2glass has no
//! setting for either. This decoder runs on the session's own thread,
//! keeps only the pictures the stream refers back to, and needs nothing installed on any platform,
//! at the cost of doing the work on the CPU.
//!
//! Each access unit's picture comes out as soon as the access unit is in. On its own, `rust_h264`
//! finishes a picture only when the next one starts, a picture late, so the element finishes it
//! after the access unit's last slice. A camera that codes the two fields of an interlaced picture
//! as separate pictures therefore shows each field on its own, at half the height; IP cameras send
//! whole frames.
//!
//! Cameras send pictures in the order they are shown, unless they use B-frames, which arrive after
//! a picture that is shown after them. [`Reorder`] puts those back in order by their presentation
//! times, holding back only as many pictures as the stream has turned out to need: none for a
//! camera without B-frames. While it learns, at the start of a stream, the B-frames that arrive
//! after a later picture has gone out are dropped.
//!
//! Like [`crate::hevc`], it follows glass2glass's own OpenH264 element: H.264 access units in, I420
//! pictures out, announced by a caps change whenever the picture size changes. The pictures are
//! already cropped to the size the stream declares.

use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
};

use g2g_core::{
    AsyncElement, BitstreamAlignment, Caps, CapsConstraint, CapsSet, Colorimetry, ConfigureOutcome,
    Dim, ElementMetadata, Frame, FrameTiming, G2gError, Interlace, MemoryDomain, OutputSink,
    PipelinePacket, Rate, RawVideoFormat, SystemSlice, VideoCodec,
};
use rust_h264::{
    decoder::{ChromaFormat, Decoder},
    error::DecodeError,
    nal::parse_annex_b,
};

/// The most pictures [`Reorder`] holds back: the most H.264 lets a stream hold back.
const MOST_HELD: usize = 16;

/// How many of the latest presentation times [`Reorder`] remembers, to tell how far out of order a
/// picture arrived: more than it may ever hold back.
const REMEMBERED: usize = 2 * MOST_HELD;

/// A presentation time this far behind the last picture shown is a jump in the camera's clock,
/// not a B-frame: B-frames are a few pictures apart.
const CLOCK_JUMP_NS: u64 = 2_000_000_000;

/// One decoded picture, as I420 bytes.
struct Picture {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Packs a decoded picture as I420, or says why it cannot be shown.
fn pack(frame: rust_h264::decoder::Frame) -> Result<Picture, G2gError> {
    if frame.bit_depth != 8 || frame.chroma_format != ChromaFormat::Yuv420 {
        // 10-bit and 4:2:2 cameras exist, but none of the converters here take them.
        return Err(G2gError::CapsMismatch);
    }

    let format = RawVideoFormat::I420;
    let expected = |plane: usize| {
        let stride = format.plane_stride(plane, frame.width)?;
        let rows = format.plane_rows(plane, frame.height)?;
        usize::try_from(u64::from(stride) * u64::from(rows)).ok()
    };
    if [&frame.y, &frame.u, &frame.v]
        .iter()
        .enumerate()
        .any(|(plane, samples)| expected(plane) != Some(samples.len()))
    {
        return Err(G2gError::CapsMismatch);
    }

    let mut pixels = frame.y;
    pixels.extend_from_slice(&frame.u);
    pixels.extend_from_slice(&frame.v);
    Ok(Picture {
        width: frame.width,
        height: frame.height,
        pixels,
    })
}

/// Puts pictures that arrive in decoding order into the order they are shown, by their
/// presentation times, holding back no more of them than the stream has needed so far.
struct Reorder<T> {
    /// The pictures held back, by presentation time, and in arrival order for equal times.
    held: BTreeMap<(u64, u64), T>,
    /// How many pictures to hold back.
    depth: usize,
    /// The presentation times of the latest pictures to arrive, oldest first.
    latest: VecDeque<u64>,
    /// The presentation time of the last picture let out.
    shown: Option<u64>,
    /// Counts the pictures that arrived, to keep pictures with equal times in arrival order.
    arrivals: u64,
}

impl<T> Reorder<T> {
    fn new() -> Self {
        Self {
            held: BTreeMap::new(),
            depth: 0,
            latest: VecDeque::with_capacity(REMEMBERED),
            shown: None,
            arrivals: 0,
        }
    }

    /// Takes a picture shown at `pts` and returns the pictures now due, in the order they are
    /// shown.
    fn push(&mut self, pts: u64, picture: T) -> Vec<(u64, T)> {
        let mut due = Vec::new();
        if self
            .shown
            .is_some_and(|shown| pts.saturating_add(CLOCK_JUMP_NS) < shown)
        {
            // Everything held back belongs before the jump.
            due = self.drain();
            self.latest.clear();
            self.shown = None;
        }

        // Every picture that arrived before this one but is shown after it had to be held back
        // for this one to go first.
        let overtaken = self.latest.iter().filter(|&&earlier| earlier > pts).count();
        self.depth = self.depth.max(overtaken).min(MOST_HELD);
        if self.latest.len() == REMEMBERED {
            self.latest.pop_front();
        }
        self.latest.push_back(pts);

        if self.shown.is_some_and(|shown| pts < shown) {
            // A picture shown after this one has gone out already.
            return due;
        }
        self.held.insert((pts, self.arrivals), picture);
        self.arrivals += 1;
        while self.held.len() > self.depth {
            let Some(((pts, _), picture)) = self.held.pop_first() else {
                break;
            };
            self.shown = Some(pts);
            due.push((pts, picture));
        }
        due
    }

    /// Lets out every picture held back, in the order they are shown.
    fn drain(&mut self) -> Vec<(u64, T)> {
        let due: Vec<(u64, T)> = std::mem::take(&mut self.held)
            .into_iter()
            .map(|((pts, _), picture)| (pts, picture))
            .collect();
        if let Some((pts, _)) = due.last() {
            self.shown = Some(*pts);
        }
        due
    }
}

fn input_template() -> Caps {
    Caps::CompressedVideo {
        codec: VideoCodec::H264,
        width: Dim::Any,
        height: Dim::Any,
        framerate: Rate::Any,
        colorimetry: Colorimetry::UNKNOWN,
        alignment: BitstreamAlignment::Unspecified,
    }
}

fn output_caps(width: Dim, height: Dim, framerate: Rate, colorimetry: Colorimetry) -> Caps {
    Caps::RawVideo {
        format: RawVideoFormat::I420,
        width,
        height,
        framerate,
        interlace: Interlace::Progressive,
        colorimetry,
    }
}

/// Decodes H.264 to I420.
pub(crate) struct AvcDec {
    decoder: Decoder,
    reorder: Reorder<Picture>,
    /// Whether a picture has come out of this stream yet.
    decoded_any: bool,
    framerate: Rate,
    colorimetry: Colorimetry,
    /// The size last announced downstream.
    geometry: Option<(u32, u32)>,
    last_timing: FrameTiming,
    sequence: u64,
    configured: bool,
}

impl AvcDec {
    pub(crate) fn new() -> Self {
        Self {
            decoder: Decoder::new(),
            reorder: Reorder::new(),
            decoded_any: false,
            framerate: Rate::Any,
            colorimetry: Colorimetry::UNKNOWN,
            geometry: None,
            last_timing: FrameTiming::default(),
            sequence: 0,
            configured: false,
        }
    }

    /// Feeds one access unit in, finishes its picture, and returns the pictures now due, each with
    /// its presentation time.
    ///
    /// A slice the decoder rejects is skipped rather than ending the stream: a camera recovers at
    /// its next key frame, as a player decoding through a lost packet does. A stream that uses
    /// something the decoder lacks before it has shown anything fails instead, since it would
    /// never show anything.
    fn decode(&mut self, access_unit: &[u8], pts_ns: u64) -> Result<Vec<(u64, Picture)>, G2gError> {
        let mut frames = Vec::new();
        for nal in parse_annex_b(access_unit) {
            match self.decoder.decode_nal(&nal) {
                Ok(frame) => frames.extend(frame),
                Err(DecodeError::Unsupported(_)) if !self.decoded_any => {
                    return Err(G2gError::CapsMismatch);
                }
                Err(_) => {}
            }
        }
        frames.extend(self.decoder.flush());

        let mut due = Vec::new();
        for frame in frames {
            let picture = pack(frame)?;
            self.decoded_any = true;
            if pts_ns == FrameTiming::PTS_NONE {
                due.push((pts_ns, picture));
            } else {
                due.extend(self.reorder.push(pts_ns, picture));
            }
        }
        Ok(due)
    }

    async fn emit(
        &mut self,
        pictures: Vec<(u64, Picture)>,
        timing: FrameTiming,
        out: &mut dyn OutputSink,
    ) -> Result<(), G2gError> {
        for (pts_ns, picture) in pictures {
            let geometry = (picture.width, picture.height);
            if self.geometry != Some(geometry) {
                let caps = output_caps(
                    Dim::Fixed(picture.width),
                    Dim::Fixed(picture.height),
                    self.framerate.clone(),
                    self.colorimetry,
                );
                out.push(PipelinePacket::CapsChanged(caps)).await?;
                self.geometry = Some(geometry);
            }
            let frame = Frame::new(
                MemoryDomain::System(SystemSlice::from_boxed(picture.pixels.into_boxed_slice())),
                FrameTiming {
                    pts_ns,
                    dts_ns: pts_ns,
                    ..timing
                },
                self.sequence,
            );
            self.sequence += 1;
            out.push(PipelinePacket::DataFrame(frame)).await?;
        }
        Ok(())
    }
}

impl AsyncElement for AvcDec {
    type ProcessFuture<'a>
        = Pin<Box<dyn Future<Output = Result<(), G2gError>> + 'a>>
    where
        Self: 'a;

    fn intercept_caps(&self, upstream: &Caps) -> Result<Caps, G2gError> {
        upstream.intersect(&input_template())
    }

    fn caps_constraint_as_transform(&self) -> CapsConstraint<'_> {
        CapsConstraint::DerivedOutput(Box::new(|input: &Caps| match input {
            Caps::CompressedVideo {
                codec: VideoCodec::H264,
                width,
                height,
                framerate,
                colorimetry,
                ..
            } => CapsSet::one(output_caps(
                width.clone(),
                height.clone(),
                framerate.clone(),
                *colorimetry,
            )),
            _ => CapsSet::from_alternatives(Vec::new()),
        }))
    }

    fn configure_pipeline(&mut self, caps: &Caps) -> Result<ConfigureOutcome, G2gError> {
        let Caps::CompressedVideo {
            codec: VideoCodec::H264,
            framerate,
            colorimetry,
            ..
        } = caps
        else {
            return Err(G2gError::CapsMismatch);
        };
        if self.configured && self.framerate != *framerate {
            self.geometry = None;
        }
        self.framerate = framerate.clone();
        self.colorimetry = *colorimetry;
        self.configured = true;
        Ok(ConfigureOutcome::Accepted)
    }

    fn process<'a>(
        &'a mut self,
        packet: PipelinePacket,
        out: &'a mut dyn OutputSink,
    ) -> Self::ProcessFuture<'a> {
        Box::pin(async move {
            if !self.configured {
                return Err(G2gError::NotConfigured);
            }
            match packet {
                PipelinePacket::DataFrame(frame) => {
                    let access_unit = frame.domain.require_system_bytes("ipcamera avc")?;
                    let timing = frame.timing;
                    let pictures = self.decode(&access_unit, timing.pts_ns)?;
                    self.last_timing = timing;
                    self.emit(pictures, timing, out).await?;
                }
                // The decoder announces its own caps; it only needs the colors from upstream.
                PipelinePacket::CapsChanged(Caps::CompressedVideo { colorimetry, .. }) => {
                    self.colorimetry = colorimetry;
                }
                PipelinePacket::CapsChanged(_) => {}
                // The runner sends the one end of stream itself; this only hands over the
                // pictures still held back.
                PipelinePacket::Eos => {
                    let pictures = self.reorder.drain();
                    let timing = self.last_timing;
                    self.emit(pictures, timing, out).await?;
                }
                PipelinePacket::Flush => {
                    self.decoder = Decoder::new();
                    self.reorder = Reorder::new();
                    out.push(PipelinePacket::Flush).await?;
                }
                other => {
                    out.push(other).await?;
                }
            }
            Ok(())
        })
    }

    fn metadata(&self) -> ElementMetadata {
        ElementMetadata::new(
            "rust_h264 video decoder",
            "Codec/Decoder/Video",
            "Decodes H.264 to I420 in Rust",
            "Drifting Prompts",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pushes pictures that arrive at `times`, and returns the times of those that come out and of
    /// those still held back at the end.
    fn order(times: &[u64]) -> (Vec<u64>, Vec<u64>) {
        let mut reorder = Reorder::new();
        let shown = times
            .iter()
            .flat_map(|&pts| reorder.push(pts, ()))
            .map(|(pts, ())| pts)
            .collect();
        let held = reorder.drain().into_iter().map(|(pts, ())| pts).collect();
        (shown, held)
    }

    #[test]
    fn pictures_in_order_come_out_at_once() {
        let mut reorder = Reorder::new();
        for pts in [0, 40, 80, 120] {
            let due: Vec<u64> = reorder.push(pts, ()).into_iter().map(|(t, ())| t).collect();
            assert_eq!(due, [pts], "nothing is held back without B-frames");
        }
    }

    #[test]
    fn b_frames_are_put_back_in_order_after_the_first_ones() {
        // I P B B P B B P, in decoding order: each P is shown after the two Bs that follow it.
        let (shown, held) = order(&[0, 3, 1, 2, 6, 4, 5, 9, 7, 8]);
        assert_eq!(
            shown,
            [0, 3, 4, 5, 6, 7, 8],
            "the first two Bs came after their P had gone out; the rest are in order"
        );
        assert_eq!(held, [9], "one picture held back is all the stream needs");
    }

    #[test]
    fn a_b_frame_pyramid_holds_back_two() {
        // I P B b b, where the middle B is a reference for the two after it.
        let (shown, held) = order(&[0, 4, 2, 1, 3, 8, 6, 5, 7, 12, 10, 9, 11]);
        assert_eq!(shown, [0, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(held, [11, 12], "two pictures held back");
    }

    #[test]
    fn a_clock_that_jumps_back_starts_again() {
        let ten_seconds = 10_000_000_000;
        let (shown, held) = order(&[ten_seconds, ten_seconds + 40, 0, 40, 80]);
        assert_eq!(
            shown,
            [ten_seconds, ten_seconds + 40, 0, 40, 80],
            "nothing after the jump is dropped as late"
        );
        assert!(held.is_empty());
    }

    #[test]
    fn equal_times_keep_their_arrival_order() {
        let mut reorder = Reorder::new();
        reorder.depth = 2;
        reorder.push(40, "first");
        reorder.push(40, "second");
        let due: Vec<&str> = reorder.drain().into_iter().map(|(_, p)| p).collect();
        assert_eq!(due, ["first", "second"]);
    }

    fn frame(chroma_format: ChromaFormat, bit_depth: u8) -> rust_h264::decoder::Frame {
        rust_h264::decoder::Frame {
            width: 4,
            height: 2,
            y: vec![1; 8],
            u: vec![2; 2],
            v: vec![3; 2],
            pic_order_cnt: 0,
            chroma_format,
            bit_depth,
        }
    }

    #[test]
    fn a_picture_packs_as_i420() {
        let picture = pack(frame(ChromaFormat::Yuv420, 8)).unwrap();
        assert_eq!((picture.width, picture.height), (4, 2));
        assert_eq!(picture.pixels, [1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn a_picture_the_converters_cannot_take_is_refused() {
        assert!(matches!(
            pack(frame(ChromaFormat::Yuv420, 10)),
            Err(G2gError::CapsMismatch)
        ));
        assert!(matches!(
            pack(frame(ChromaFormat::Yuv422, 8)),
            Err(G2gError::CapsMismatch)
        ));
    }

    /// The pretend camera's pictures, relayed as a camera sends them and decoded here: each access
    /// unit gives its own picture at once, with its own time, where Windows' decoder held pictures
    /// back.
    #[cfg(windows)]
    #[test]
    fn each_access_unit_gives_its_picture_at_once() {
        use std::time::Duration;

        use crate::{MediaSession, Relayed, pretend};

        let camera = pretend::start(pretend::Pattern {
            width: 640,
            height: 360,
            fps: 25,
            frames: 250,
        })
        .expect("the pretend camera should start");
        let mut session = MediaSession::relay(&camera).expect("the session should start");
        let mut viewer = session.feed().watch();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut decoder = AvcDec::new();

        let mut decoded = 0;
        while decoded < 20 {
            let next = runtime
                .block_on(async {
                    tokio::time::timeout(Duration::from_secs(10), viewer.next()).await
                })
                .expect("the feed should keep coming")
                .expect("the feed should not end");
            let Relayed::Picture(access_unit) = next else {
                continue;
            };
            let pts = access_unit.timestamp_us * 1_000;
            let due = decoder
                .decode(&access_unit.bytes, pts)
                .expect("the access unit should decode");
            assert_eq!(
                due.len(),
                1,
                "access unit {decoded} gave {} pictures",
                due.len()
            );
            let (time, picture) = &due[0];
            assert_eq!(*time, pts, "the picture keeps its access unit's time");
            assert_eq!((picture.width, picture.height), (640, 360));
            decoded += 1;
        }

        session.stop();
    }
}
