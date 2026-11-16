//! What a camera sends, and the glass2glass graph that plays it.
//!
//! A session first asks the camera what it offers (an RTSP DESCRIBE, through retina), then builds
//! a graph for exactly that. As a glass2glass launch line, the graph is
//!
//! ```text
//! rtspsrcn ! h264parse ! record tap ! size tap ! decode gate ! <decoder> ! videoconvert ! (BGRA) slot sink
//!          ! aacparse ! record tap ! aacdec ! audioconvert ! audioresample ! volume ! <audio sink>
//! ```
//!
//! where the taps, the gate and the slot sink are this crate's own. The size tap ([`crate::sink`])
//! notes the picture size the stream declares, which the sink crops the decoded picture to. The
//! record taps ([`crate::record`]) pass everything on, copying the compressed stream into a file
//! while the camera records. The decode gate ([`crate::gate`]) hands the decoder nothing while no
//! tile shows the camera, and catches it up from the last keyframe when one does again.
//!
//! A relaying session ([`crate::relay`]) builds the short graph instead, with no decoder and no
//! sound device. It records as a live view does, so it keeps an AAC camera's sound for its
//! recordings and drops it after the record tap:
//!
//! ```text
//! rtspsrcn ! h264parse ! record tap ! feed sink
//!          ! aacparse  ! record tap ! discard
//! ```
//!
//! The decoders are this crate's own, in Rust, on every platform: [`crate::avc`] for H.264 and
//! [`crate::hevc`] for H.265, which gets `h265parse`. G.711 audio gets `alawdec` or `mulawdec`, and
//! there is no audio branch at all when the camera sends no sound. The audio sink is each
//! platform's own: WASAPI on Windows, ALSA on Linux.
//!
//! A Reolink's FLV stream ([`crate::flv`]) starts the same graphs another way: its source hands
//! the stream's bytes to an `flvdemux` for the video and another for the sound, which leave as
//! `rtspsrcn`'s two outputs do, so everything after them is the same.
//!
//! ```text
//! flvsrc ! flvdemux stream=h264 ! h264parse ! ...
//!        ! flvdemux stream=aac  ! aacparse  ! ...
//! ```
//!
//! The graph is built in code rather than parsed from a launch line, so the password goes straight
//! into the RTSP source and never sits in a string that could end up in a log.

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

/// What the camera says it is, in its RTSP requests.
pub(crate) const USER_AGENT: &str = "IPCamera";

/// The name of the `volume` element, so a running graph's gain can be changed.
pub(crate) const VOLUME: &str = "volume";

/// A stream that delivers nothing for this long counts as lost, so it reconnects instead of
/// showing the last picture forever.
const STALL_TIMEOUT_MS: u64 = 10_000;

/// The rate and layout every camera's sound is converted to before the sink.
const OUTPUT_RATE: u32 = 48_000;
const OUTPUT_CHANNELS: u8 = 2;

/// Where a session's stream comes from.
pub(crate) enum Source {
    /// An RTSP address, with no login in it: the RTSP source logs in itself.
    Rtsp(String),
    /// A Reolink's FLV stream, already answering.
    Flv(FlvAnswer),
}

/// The tracks a camera offers that this app plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Offer {
    pub(crate) video: VideoCodec,
    pub(crate) audio: Option<AudioCodec>,
}

/// The camera sound formats this app plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AudioCodec {
    Aac,
    /// G.711 A-law, RTP `PCMA`.
    Alaw,
    /// G.711 µ-law, RTP `PCMU`.
    Mulaw,
}

impl AudioCodec {
    /// The `audio-format` nick `rtspsrcn` takes.
    fn nick(self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Alaw => "alaw",
            Self::Mulaw => "mulaw",
        }
    }
}

/// The login `rtspsrcn` and retina send, if the camera has a user name.
pub(crate) fn credentials(camera: &CameraConfig) -> Option<Credentials> {
    let user = camera.user.trim();
    (!user.is_empty()).then(|| Credentials {
        username: user.to_string(),
        password: camera.password.clone(),
    })
}

/// Asks the camera at `url` what it sends.
///
/// This is the one request whose answer tells a rejected password apart from every other failure,
/// which glass2glass reports as the same connect error. A 401 becomes
/// [`MediaError::LoginRejected`].
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

/// The RTSP source for `camera` at `url`, with `outputs` tracks: the video, then the sound when
/// there are two.
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

/// Adds `source` to `graph`, taking its sound too when `audio` names it, and returns the pads its
/// video and its sound leave on: an RTSP source's own outputs, or for an FLV stream the `flvdemux`
/// behind each of its outputs.
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

/// The graph that plays `offer` from `source`: pictures into `visibility`'s slot while it is
/// shown, sound at `gain`, and both into `recorder` while it records.
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

/// The decoder for `codec`, this crate's own in Rust for either. A live view and a still
/// ([`crate::still`]) decode with the same one.
pub(crate) fn video_decoder(codec: VideoCodec) -> GraphNode {
    match codec {
        VideoCodec::H265 => GraphNodeRef::element(HevcDec::new()),
        _ => GraphNodeRef::element(AvcDec::new()),
    }
}

/// The graph that relays the video of `offer` from `source` to `feed`, compressed as the camera
/// sent it, and into `recorder` while it records, with the sound when `offer` has AAC. It decodes
/// nothing and needs no sound device, so it runs on a machine with no screen or speakers.
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

    /// The default playback device, through WASAPI.
    pub(super) fn audio_sink() -> Result<GraphNode> {
        Ok(GraphNodeRef::element(WasapiSink::new()))
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use g2g_core::runtime::{GraphNode, GraphNodeRef};
    use g2g_plugins::alsasink::AlsaSink;

    use crate::error::Result;

    /// The default ALSA device.
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
