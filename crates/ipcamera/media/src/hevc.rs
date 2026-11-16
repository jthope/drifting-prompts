//! An H.265 decoder element built on `rust_h265`, a decoder written in Rust.
//!
//! glass2glass's Media Foundation element creates the H.265 decoder by its fixed class id. The
//! Microsoft Store's HEVC Video Extensions do not register that id where a desktop app looks it
//! up, so the element fails even where the extension is installed. Decoding in Rust needs nothing
//! installed and works on every platform, at the cost of doing the work on the CPU.
//!
//! It follows glass2glass's own OpenH264 element: H.265 access units in, I420 pictures out,
//! announced by a caps change whenever the picture size changes. The pictures are already cropped
//! to the size the stream declares.

use std::{collections::BTreeSet, future::Future, pin::Pin};

use g2g_core::{
    AsyncElement, BitstreamAlignment, Caps, CapsConstraint, CapsSet, Colorimetry, ConfigureOutcome,
    Dim, ElementMetadata, Frame, FrameTiming, G2gError, Interlace, MemoryDomain, OutputSink,
    PipelinePacket, Rate, RawVideoFormat, SystemSlice, VideoCodec,
};
use rust_h265::{Decoder, PixelData};

/// Worker threads per decoder. Each one can hold a picture back while it decodes, so more of them
/// buy throughput with latency; two keep a 1080p camera well ahead of real time.
const THREADS: usize = 2;

/// One decoded picture, as I420 bytes.
struct Picture {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Packs a decoded picture as I420, or says why it cannot be shown.
fn pack(frame: rust_h265::Frame) -> Result<Picture, G2gError> {
    if frame.bit_depth != 8 || frame.chroma_format_idc != 1 {
        // 10-bit, 4:2:2 and 4:4:4 cameras exist, but none of the converters here take them.
        return Err(G2gError::CapsMismatch);
    }
    let (PixelData::U8(y), PixelData::U8(u), PixelData::U8(v)) = (frame.y, frame.u, frame.v) else {
        return Err(G2gError::CapsMismatch);
    };

    let format = RawVideoFormat::I420;
    let expected = |plane: usize| {
        let stride = format.plane_stride(plane, frame.width)?;
        let rows = format.plane_rows(plane, frame.height)?;
        usize::try_from(u64::from(stride) * u64::from(rows)).ok()
    };
    if [&y, &u, &v]
        .iter()
        .enumerate()
        .any(|(plane, samples)| expected(plane) != Some(samples.len()))
    {
        return Err(G2gError::CapsMismatch);
    }

    let mut pixels = y;
    pixels.extend_from_slice(&u);
    pixels.extend_from_slice(&v);
    Ok(Picture {
        width: frame.width,
        height: frame.height,
        pixels,
    })
}

fn new_decoder() -> Decoder {
    Decoder::new().with_display_order().with_threads(THREADS)
}

fn input_template() -> Caps {
    Caps::CompressedVideo {
        codec: VideoCodec::H265,
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

/// Decodes H.265 to I420.
pub(crate) struct HevcDec {
    decoder: Decoder,
    framerate: Rate,
    colorimetry: Colorimetry,
    /// The size last announced downstream.
    geometry: Option<(u32, u32)>,
    /// Pictures come out in display order, so each takes the earliest time still waiting.
    pending_pts: BTreeSet<u64>,
    last_timing: FrameTiming,
    sequence: u64,
    configured: bool,
}

impl HevcDec {
    pub(crate) fn new() -> Self {
        Self {
            decoder: new_decoder(),
            framerate: Rate::Any,
            colorimetry: Colorimetry::UNKNOWN,
            geometry: None,
            pending_pts: BTreeSet::new(),
            last_timing: FrameTiming::default(),
            sequence: 0,
            configured: false,
        }
    }

    /// Feeds one access unit in and takes whatever pictures are ready.
    ///
    /// A slice the decoder rejects is skipped rather than ending the stream: a camera recovers at
    /// its next key frame, as a player decoding through a lost packet does.
    fn decode(&mut self, access_unit: &[u8]) -> Result<Vec<Picture>, G2gError> {
        for nal in rust_h265::parse_annex_b(access_unit) {
            let _ = self.decoder.decode_nal(&nal);
        }
        while self.decoder.next_error().is_some() {}
        let mut pictures = Vec::new();
        while let Some(frame) = self.decoder.next_output() {
            pictures.push(pack(frame)?);
        }
        Ok(pictures)
    }

    fn drain(&mut self) -> Result<Vec<Picture>, G2gError> {
        let mut pictures = Vec::new();
        while let Some(frame) = self.decoder.flush() {
            pictures.push(pack(frame)?);
        }
        Ok(pictures)
    }

    async fn emit(
        &mut self,
        pictures: Vec<Picture>,
        timing: FrameTiming,
        out: &mut dyn OutputSink,
    ) -> Result<(), G2gError> {
        for picture in pictures {
            let pts_ns = self
                .pending_pts
                .pop_first()
                .unwrap_or(FrameTiming::PTS_NONE);
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

impl AsyncElement for HevcDec {
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
                codec: VideoCodec::H265,
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
            codec: VideoCodec::H265,
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
                    let access_unit = frame.domain.require_system_bytes("ipcamera hevc")?;
                    let timing = frame.timing;
                    if timing.pts_ns != FrameTiming::PTS_NONE {
                        self.pending_pts.insert(timing.pts_ns);
                    }
                    let pictures = self.decode(&access_unit)?;
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
                    let pictures = self.drain()?;
                    let timing = self.last_timing;
                    self.emit(pictures, timing, out).await?;
                }
                PipelinePacket::Flush => {
                    self.decoder = new_decoder();
                    self.pending_pts.clear();
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
            "rust_h265 video decoder",
            "Codec/Decoder/Video",
            "Decodes H.265 to I420 in Rust",
            "Drifting Prompts",
        )
    }
}
