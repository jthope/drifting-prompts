//! One picture from a relaying session, decoded when asked: a snapshot, or a recording's
//! thumbnail.
//!
//! A relay decodes nothing, and decoding every camera all the time would cost the server what
//! relaying saves. But its [`Feed`](crate::Feed) keeps the pictures since the last keyframe for
//! late viewers, and those are exactly what decoding the newest picture takes. So a still is a
//! short graph of its own, built when asked and gone when done:
//!
//! ```text
//! backlog source ! h264parse ! size tap ! <decoder> ! videoconvert ! (BGRA) slot sink
//! ```
//!
//! with the same parser, decoders and sink as a live view ([`crate::pipeline`]), so a still is the
//! picture the desktop would have shown. It runs on a thread of its own with a runtime of its own,
//! as a session's graph does, and the caller waits for it while a keyframe interval of pictures
//! decodes on the CPU.

use std::{
    future::{Ready, ready},
    pin::Pin,
    sync::{Arc, atomic::AtomicBool},
    thread,
};

use g2g_core::{
    BitstreamAlignment, Caps, Colorimetry, ConfigureOutcome, Dim, ElementMetadata, Frame,
    FrameTiming, G2gError, MemoryDomain, OutputSink, PipelinePacket, Rate, RawVideoFormat,
    SystemSlice, VideoCodec,
    graph::Graph,
    runtime::{GraphNode, GraphNodeRef, LatencyProfile, SourceLoop, run_graph},
};
use g2g_plugins::{
    clock::WallClock, h264parse::H264Parse, h265parse::H265Parse, typefind::elementary_video_caps,
    videoconvert::VideoConvert,
};

use crate::{
    DecodedFrame, FrameSlot, Picture,
    error::{Result, failed},
    gate::Visibility,
    pipeline,
    sink::{DisplaySize, SizeTap, SlotSink},
};

/// Pictures a relay kept, to decode.
pub(crate) struct Backlog {
    /// The stream's codec.
    pub(crate) codec: VideoCodec,
    /// The picture size the stream declared, when it did.
    pub(crate) size: Option<(u32, u32)>,
    /// The pictures, from a keyframe on.
    pub(crate) pictures: Vec<Arc<Picture>>,
}

/// Decodes `backlog`, which starts at a keyframe, and returns the last picture it makes.
pub(crate) fn decode(backlog: Backlog) -> Result<DecodedFrame> {
    if !backlog.pictures.first().is_some_and(|first| first.keyframe) {
        return Err(failed!("no picture has arrived yet"));
    }
    thread::Builder::new()
        .name("ipcamera-still".into())
        .spawn(move || decode_here(backlog))?
        .join()
        .map_err(|_| failed!("decoding the picture crashed"))?
}

/// [`decode`], on this thread.
fn decode_here(backlog: Backlog) -> Result<DecodedFrame> {
    let frames = FrameSlot::default();
    let graph = build(backlog, frames.clone())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime
        .block_on(run_graph(
            graph,
            &WallClock::new(),
            LatencyProfile::Throughput,
        ))
        .map_err(|error| failed!("the picture could not be decoded: {error:?}"))?;
    frames
        .latest()
        .map(|frame| (*frame).clone())
        .ok_or_else(|| failed!("the camera's pictures decoded to nothing"))
}

fn build(backlog: Backlog, frames: FrameSlot) -> Result<Graph<GraphNode>> {
    let mut graph: Graph<GraphNode> = Graph::new();
    let link = |error| failed!("the decoder could not be linked: {error:?}");
    let codec = backlog.codec;

    let source = graph.add_source(GraphNodeRef::source(BacklogSource {
        caps: caps_of(codec, backlog.size),
        pictures: backlog.pictures,
    }));
    let parse = graph.add_transform(match codec {
        VideoCodec::H265 => GraphNodeRef::element(H265Parse::new()),
        _ => GraphNodeRef::element(H264Parse::new()),
    });
    let display = DisplaySize::default();
    let tap = graph.add_transform(GraphNodeRef::element(SizeTap::new(display.clone())));
    let decoder = graph.add_transform(pipeline::video_decoder(codec));
    let convert = graph.add_transform(GraphNodeRef::element(VideoConvert::new(
        RawVideoFormat::Bgra8,
    )));
    let sink = graph.add_sink(GraphNodeRef::element(SlotSink::new(
        Arc::new(Visibility::new(frames)),
        Arc::new(AtomicBool::new(false)),
        display,
    )));
    for (from, to) in [
        (source, parse),
        (parse, tap),
        (tap, decoder),
        (decoder, convert),
        (convert, sink),
    ] {
        graph.link(from, to).map_err(link)?;
    }
    Ok(graph)
}

/// What the camera's RTSP source would have said of the stream: its codec and, when the stream
/// declared it, its size. The parser reads the rest from the stream itself.
fn caps_of(codec: VideoCodec, size: Option<(u32, u32)>) -> Caps {
    match size {
        Some((width, height)) => Caps::CompressedVideo {
            codec,
            width: Dim::Fixed(width),
            height: Dim::Fixed(height),
            framerate: Rate::Range {
                min_q16: 1 << 16,
                max_q16: 240 << 16,
            },
            colorimetry: Colorimetry::UNKNOWN,
            alignment: BitstreamAlignment::Unspecified,
        },
        None => elementary_video_caps(codec),
    }
}

/// Plays the pictures a relay kept back into the graph, then ends the stream.
struct BacklogSource {
    caps: Caps,
    pictures: Vec<Arc<Picture>>,
}

impl SourceLoop for BacklogSource {
    type RunFuture<'a>
        = Pin<Box<dyn Future<Output = Result<u64, G2gError>> + 'a>>
    where
        Self: 'a;
    type CapsFuture<'a>
        = Ready<Result<Caps, G2gError>>
    where
        Self: 'a;

    fn intercept_caps<'a>(&'a mut self) -> Self::CapsFuture<'a> {
        ready(Ok(self.caps.clone()))
    }

    fn configure_pipeline(&mut self, _caps: &Caps) -> Result<ConfigureOutcome, G2gError> {
        Ok(ConfigureOutcome::Accepted)
    }

    fn run<'a>(&'a mut self, out: &'a mut dyn OutputSink) -> Self::RunFuture<'a> {
        Box::pin(async move {
            let mut pushed = 0;
            for (sequence, picture) in self.pictures.iter().enumerate() {
                let at = picture.timestamp_us.saturating_mul(1_000);
                let bytes = picture.bytes.clone().into_boxed_slice();
                let frame = Frame::new(
                    MemoryDomain::System(SystemSlice::from_boxed(bytes)),
                    FrameTiming {
                        pts_ns: at,
                        dts_ns: at,
                        keyframe: picture.keyframe,
                        ..FrameTiming::default()
                    },
                    sequence as u64,
                );
                out.push(PipelinePacket::DataFrame(frame)).await?;
                pushed += 1;
            }
            out.push(PipelinePacket::Eos).await?;
            Ok(pushed)
        })
    }

    fn metadata(&self) -> ElementMetadata {
        ElementMetadata::new(
            "IP Camera backlog",
            "Source/Video",
            "Plays the pictures a relay kept since the last keyframe, so one can be decoded.",
            "Drifting Prompts",
        )
    }
}
