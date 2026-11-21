//! What a camera sends, from its answer to DESCRIBE, and the glass2glass graph that plays it.
//!
//! The graph is built in code, not parsed from a launch line, so the password never sits in a
//! string that could end up in a log.

use std::sync::{Arc, Mutex, atomic::AtomicBool};

use g2g_core::{
    AudioFormat, MultiOutputSource, RawVideoFormat, VideoCodec,
    graph::{Graph, PadId},
    property::PropValue,
    runtime::{GraphNode, GraphNodeRef},
};
use g2g_plugins::{
    aacparse::AacParse,
    audioconvert::AudioConvert,
    audioresample::AudioResample,
    flvdemux::{FlvDemux, FlvStream},
    g711::{AlawDec, MulawDec},
    h264parse::H264Parse,
    h265parse::H265Parse,
    rtspsrcn::{AUDIO_PORT, RtspSrcN, VIDEO_PORT},
    symphoniadec::AacDec,
    videoconvert::VideoConvert,
    volume::Volume,
};
use ipcamera_core::CameraConfig;
use retina::client::{Credentials, Session, SessionOptions};
use url::Url;

use crate::{
    avc::AvcDec,
    error::{MediaError, Result, failed},
    flv::{FlvAnswer, FlvSrc},
    gate::{DecodeGate, Visibility},
    hevc::HevcDec,
    record::{RecordTap, Recorder, Track},
    relay::{Discard, Feed, FeedSink},
    sink::{DisplaySize, SizeTap, SlotSink},
};

pub(crate) const USER_AGENT: &str = "IPCamera";

pub(crate) const VOLUME: &str = "volume";

/// Without it a stalled stream would show its last picture forever.
const STALL_TIMEOUT_MS: u64 = 10_000;

const OUTPUT_RATE: u32 = 48_000;
const OUTPUT_CHANNELS: u8 = 2;

pub(crate) enum Source {
    /// With no login in it: the RTSP source logs in itself.
    Rtsp(String),
    Flv(FlvAnswer),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Offer {
    pub(crate) video: VideoCodec,
    pub(crate) audio: Option<AudioCodec>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AudioCodec {
    Aac,
    Alaw,
    Mulaw,
}

impl AudioCodec {
    fn nick(self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Alaw => "alaw",
            Self::Mulaw => "mulaw",
        }
    }
}

pub(crate) fn credentials(camera: &CameraConfig) -> Option<Credentials> {
    let user = camera.user.trim();
    (!user.is_empty()).then(|| Credentials {
        username: user.to_string(),
        password: camera.password.clone(),
    })
}

/// The one request that tells a rejected login apart: glass2glass reports every failure alike.
pub(crate) async fn describe(url: &str, camera: &CameraConfig) -> Result<Offer> {
    let parsed =
        Url::parse(url).map_err(|error| failed!("the camera address is not valid: {error}"))?;
    let options = SessionOptions::default()
        .creds(credentials(camera))
        .user_agent(USER_AGENT.to_string());
    let session = Session::describe(parsed, options).await.map_err(|error| {
        if error.status_code() == Some(401) {
            MediaError::LoginRejected
        } else {
            failed!("{error}")
        }
    })?;

    let streams = session.streams();
    let video = streams
        .iter()
        .filter(|stream| stream.media() == "video")
        .find_map(|stream| match stream.encoding_name() {
            "h264" => Some(VideoCodec::H264),
            "h265" => Some(VideoCodec::H265),
            _ => None,
        })
        .ok_or_else(|| {
            MediaError::CannotPlay("the camera sends no H.264 or H.265 video".to_string())
        })?;
    let audio = streams
        .iter()
        .filter(|stream| stream.media() == "audio")
        .find_map(|stream| match stream.encoding_name() {
            "mpeg4-generic" => Some(AudioCodec::Aac),
            "pcma" => Some(AudioCodec::Alaw),
            "pcmu" => Some(AudioCodec::Mulaw),
            _ => None,
        });
    Ok(Offer { video, audio })
}

fn rtsp_source(url: &str, camera: &CameraConfig, outputs: u8) -> Result<RtspSrcN> {
    let mut source = RtspSrcN::new(url)
        .with_user_agent(USER_AGENT)
        .with_outputs(usize::from(outputs));
    if let Some(Credentials { username, password }) = credentials(camera) {
        source = source.with_credentials(username, password);
    }
    let transport = if camera.use_tcp { "tcp" } else { "udp" };
    let properties = [
        (
            "protocols",
            PropValue::Flags(Vec::from([transport.to_string()])),
        ),
        ("stall-timeout", PropValue::Uint(STALL_TIMEOUT_MS)),
    ];
    for (name, value) in properties {
        source
            .set_property(name, value)
            .map_err(|error| failed!("the RTSP source refused `{name}`: {error:?}"))?;
    }
    Ok(source)
}

fn add_source(
    graph: &mut Graph<GraphNode>,
    source: Source,
    camera: &CameraConfig,
    audio: Option<AudioCodec>,
) -> Result<(PadId, Option<PadId>)> {
    let link = |error| failed!("the pipeline could not be linked: {error:?}");
    let outputs: u8 = if audio.is_some() { 2 } else { 1 };
    match source {
        Source::Rtsp(url) => {
            let mut source = rtsp_source(&url, camera, outputs)?;
            if let Some(audio) = audio {
                source
                    .set_property("audio-format", PropValue::Str(audio.nick().to_string()))
                    .map_err(|error| {
                        failed!("the RTSP source refused the audio format: {error:?}")
                    })?;
            }
            let source = graph.add_fanout_src(GraphNodeRef::fanout_source(source), outputs);
            Ok((
                source.output(VIDEO_PORT as u8),
                audio.map(|_| source.output(AUDIO_PORT as u8)),
            ))
        }
        Source::Flv(answer) => {
            let source = graph.add_fanout_src(
                GraphNodeRef::fanout_source(FlvSrc::new(answer, usize::from(outputs))),
                outputs,
            );
            let video = graph.add_transform(GraphNodeRef::element(
                FlvDemux::new().with_stream(FlvStream::H264),
            ));
            graph.link(source.output(0), video).map_err(link)?;
            // FLV carries no other sound this app plays.
            let sound = match audio {
                Some(AudioCodec::Aac) => {
                    let sound = graph.add_transform(GraphNodeRef::element(
                        FlvDemux::new().with_stream(FlvStream::Aac),
                    ));
                    graph.link(source.output(1), sound).map_err(link)?;
                    Some(sound.into())
                }
                _ => None,
            };
            Ok((video.into(), sound))
        }
    }
}

pub(crate) fn build(
    source: Source,
    camera: &CameraConfig,
    offer: &Offer,
    visibility: Arc<Visibility>,
    live: Arc<AtomicBool>,
    gain: f64,
    recorder: Arc<Mutex<Recorder>>,
) -> Result<Graph<GraphNode>> {
    let mut graph: Graph<GraphNode> = Graph::new();
    let link = |error| failed!("the pipeline could not be linked: {error:?}");

    let (video_out, audio_out) = add_source(&mut graph, source, camera, offer.audio)?;

    let parse = graph.add_transform(match offer.video {
        VideoCodec::H265 => GraphNodeRef::element(H265Parse::new()),
        _ => GraphNodeRef::element(H264Parse::new()),
    });
    let record = graph.add_transform(GraphNodeRef::element(RecordTap::new(
        Track::Video,
        recorder.clone(),
    )));
    let display = DisplaySize::default();
    let tap = graph.add_transform(GraphNodeRef::element(SizeTap::new(display.clone())));
    let gate = graph.add_transform(GraphNodeRef::element(DecodeGate::new(
        visibility.clone(),
        live.clone(),
    )));
    let decode = graph.add_transform(video_decoder(offer.video));
    let convert = graph.add_transform(GraphNodeRef::element(VideoConvert::new(
        RawVideoFormat::Bgra8,
    )));
    let sink = graph.add_sink(GraphNodeRef::element(SlotSink::new(
        visibility, live, display,
    )));
    graph.link(video_out, parse).map_err(link)?;
    graph.link(parse, record).map_err(link)?;
    graph.link(record, tap).map_err(link)?;
    graph.link(tap, gate).map_err(link)?;
    graph.link(gate, decode).map_err(link)?;
    graph.link(decode, convert).map_err(link)?;
    graph.link(convert, sink).map_err(link)?;

    let (Some(audio), Some(audio_out)) = (offer.audio, audio_out) else {
        return Ok(graph);
    };
    let decoded = match audio {
        AudioCodec::Aac => {
            let parse = graph.add_transform(GraphNodeRef::element(AacParse::new()));
            let record = graph.add_transform(GraphNodeRef::element(RecordTap::new(
                Track::Sound,
                recorder,
            )));
            let decode = graph.add_transform(GraphNodeRef::element(AacDec::new()));
            graph.link(audio_out, parse).map_err(link)?;
            graph.link(parse, record).map_err(link)?;
            graph.link(record, decode).map_err(link)?;
            decode
        }
        // An MP4 has no place for G.711, so this sound is not recorded.
        AudioCodec::Alaw | AudioCodec::Mulaw => {
            let decode = graph.add_transform(if audio == AudioCodec::Alaw {
                GraphNodeRef::element(AlawDec::new())
            } else {
                GraphNodeRef::element(MulawDec::new())
            });
            graph.link(audio_out, decode).map_err(link)?;
            decode
        }
    };
    let convert = graph.add_transform(GraphNodeRef::element(AudioConvert::new(
        AudioFormat::PcmS16Le,
        OUTPUT_CHANNELS,
    )));
    let resample = graph.add_transform(GraphNodeRef::element(AudioResample::new(OUTPUT_RATE)));
    let volume = graph.add_transform(GraphNodeRef::element(Volume::new().with_volume(gain)));
    graph.set_node_name(volume, VOLUME.to_string());
    let speaker = graph.add_sink(platform::audio_sink()?);
    graph.link(decoded, convert).map_err(link)?;
    graph.link(convert, resample).map_err(link)?;
    graph.link(resample, volume).map_err(link)?;
    graph.link(volume, speaker).map_err(link)?;
    Ok(graph)
}

pub(crate) fn video_decoder(codec: VideoCodec) -> GraphNode {
    match codec {
        VideoCodec::H265 => GraphNodeRef::element(HevcDec::new()),
        _ => GraphNodeRef::element(AvcDec::new()),
    }
}

/// Decodes nothing and needs no sound device.
pub(crate) fn build_relay(
    source: Source,
    camera: &CameraConfig,
    offer: &Offer,
    feed: Feed,
    live: Arc<AtomicBool>,
    recorder: Arc<Mutex<Recorder>>,
) -> Result<Graph<GraphNode>> {
    let mut graph: Graph<GraphNode> = Graph::new();
    let link = |error| failed!("the pipeline could not be linked: {error:?}");

    // Only AAC goes into a recording, so only AAC is worth taking.
    let sound = offer.audio.filter(|audio| *audio == AudioCodec::Aac);
    let (video_out, audio_out) = add_source(&mut graph, source, camera, sound)?;
    let parse = graph.add_transform(match offer.video {
        VideoCodec::H265 => GraphNodeRef::element(H265Parse::new()),
        _ => GraphNodeRef::element(H264Parse::new()),
    });
    let record = graph.add_transform(GraphNodeRef::element(RecordTap::new(
        Track::Video,
        recorder.clone(),
    )));
    let sink = graph.add_sink(GraphNodeRef::element(FeedSink::new(feed, live)));
    graph.link(video_out, parse).map_err(link)?;
    graph.link(parse, record).map_err(link)?;
    graph.link(record, sink).map_err(link)?;

    if let Some(audio_out) = audio_out {
        let parse = graph.add_transform(GraphNodeRef::element(AacParse::new()));
        let record = graph.add_transform(GraphNodeRef::element(RecordTap::new(
            Track::Sound,
            recorder,
        )));
        let discard = graph.add_sink(GraphNodeRef::element(Discard));
        graph.link(audio_out, parse).map_err(link)?;
        graph.link(parse, record).map_err(link)?;
        graph.link(record, discard).map_err(link)?;
    }
    Ok(graph)
}

#[cfg(windows)]
mod platform {
    use g2g_core::runtime::{GraphNode, GraphNodeRef};
    use g2g_plugins::wasapisink::WasapiSink;

    use crate::error::Result;

    pub(super) fn audio_sink() -> Result<GraphNode> {
        Ok(GraphNodeRef::element(WasapiSink::new()))
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use g2g_core::runtime::{GraphNode, GraphNodeRef};
    use g2g_plugins::alsasink::AlsaSink;

    use crate::error::Result;

    pub(super) fn audio_sink() -> Result<GraphNode> {
        Ok(GraphNodeRef::element(AlsaSink::new()))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use g2g_core::runtime::GraphNode;

    use crate::error::{Result, failed};

    pub(super) fn audio_sink() -> Result<GraphNode> {
        Err(failed!(
            "camera sound is not supported on this platform yet"
        ))
    }
}
