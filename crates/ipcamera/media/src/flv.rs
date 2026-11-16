//! A Reolink's FLV stream: what a session opens it with, and the glass2glass source that plays it.
//!
//! Reolink's web server sends a camera's streams as FLV, some of them only that way (the one its
//! apps call Balanced, a tracking lens's sub stream). A session for one
//! ([`StreamPath::ReolinkFlv`](ipcamera_core::StreamPath::ReolinkFlv)) logs in to the camera's API
//! once for a token ([`log_in`], before the session's runtime, since the client blocks), GETs the
//! stream with it ([`open`]), reads the stream's head for whether it has sound, and drops the login,
//! which logs out: the camera goes on sending. A reconnect logs in again, because the camera hangs
//! up on a token whose session has ended.
//!
//! [`FlvSrc`] hands the same bytes to each of its outputs, one per track, and a glass2glass
//! `flvdemux` behind each takes its own track out: H.264 in Annex-B, AAC in ADTS, as `rtspsrcn`'s
//! two outputs carry them, so the rest of the graph is the same as for RTSP.
//!
//! The stream's address carries the token, which is a login too: no error here repeats it.

use std::{future::Future, pin::Pin, time::Duration};

use g2g_core::{
    ByteStreamEncoding, Caps, Frame, FrameTiming, G2gError, MemoryDomain, MultiOutputSink,
    MultiOutputSource, NetworkFailure, PipelinePacket, SystemSlice, VideoCodec,
    query::LatencyReport,
};
use ipcamera_core::{
    CameraConfig,
    reolink::{ReolinkClient, ReolinkError, read_flv_head},
};

use crate::{
    error::{MediaError, Result, failed},
    pipeline::{AudioCodec, Offer},
};

/// How long the camera may take to take the connection.
const CONNECT_WITHIN: Duration = Duration::from_secs(5);

/// The most of a stream read for its head before it counts as saying nothing of itself: the head
/// comes in the first few kilobytes.
const HEAD_BYTES: usize = 256 * 1024;

/// A stream that delivers nothing for this long counts as lost, as an RTSP one does
/// (`pipeline`'s stall timeout), so it reconnects instead of showing the last picture forever.
const STALL: Duration = Duration::from_secs(10);

/// What a session opens a Reolink's FLV stream with: the stream's address, a token in it, and the
/// client the token is from, which logs out when dropped.
pub(crate) struct FlvLogin {
    url: String,
    /// `None` only in tests, which have a stream and no camera to log in to.
    client: Option<ReolinkClient>,
}

/// Logs in to `camera`'s API once and makes the address of its FLV stream `name`
/// (`channel0_ext.bcs`) with the login's token. Blocks for up to the client's timeout, so call it
/// off any async runtime. A refused login is [`MediaError::LoginRejected`], which nothing
/// retries; a camera with no password saved isn't logged in to at all, since a blank login counts
/// against the account like a wrong one.
pub(crate) fn log_in(camera: &CameraConfig, name: &str) -> Result<FlvLogin> {
    if camera.password.is_empty() {
        return Err(MediaError::CannotPlay(
            "the camera's FLV stream needs its password, and none is saved".to_string(),
        ));
    }
    let refused = |error| match error {
        ReolinkError::LoginRejected(_) | ReolinkError::Locked => MediaError::LoginRejected,
        other => MediaError::Failed(other.to_string()),
    };
    let mut client = ReolinkClient::connect(
        camera.host.trim(),
        camera.api_port,
        camera.api_https,
        camera.user.trim(),
        &camera.password,
    )
    .map_err(refused)?;
    let url = client.flv_url(name).map_err(refused)?;
    Ok(FlvLogin {
        url,
        client: Some(client),
    })
}

/// A Reolink's FLV stream, answering: the response, and the bytes already read for its head.
pub(crate) struct FlvAnswer {
    response: reqwest::Response,
    head: Vec<u8>,
}

/// GETs the stream `login` names, reads its head, and logs out. The head becomes the offer: H.264
/// video, or a stream that can't be played, and AAC sound where it has any.
pub(crate) async fn open(login: FlvLogin) -> Result<(FlvAnswer, Offer)> {
    let FlvLogin { url, client } = login;
    let http = reqwest::Client::builder()
        // The certificate a camera signs itself, which `ReolinkClient` accepts too.
        .tls_danger_accept_invalid_certs(true)
        .connect_timeout(CONNECT_WITHIN)
        .build()
        .map_err(|error| failed!("{}", error.without_url()))?;
    let sent = http.get(&url).send().await;
    // The stream has started, or won't: the login can end, and the camera goes on sending. It
    // waits for the camera's answer, which on this session's own runtime holds up nothing else.
    drop(client);
    let mut response = sent.map_err(|error| {
        failed!(
            "the camera's FLV stream can't be reached: {}",
            error.without_url()
        )
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(failed!(
            "the camera answered the request for its FLV stream with HTTP {}",
            status.as_u16()
        ));
    }

    let mut head = Vec::new();
    let said = loop {
        if let Some(said) = read_flv_head(&head) {
            break said;
        }
        if head.len() > HEAD_BYTES {
            return Err(MediaError::CannotPlay(
                "the camera's FLV stream doesn't say what it sends".to_string(),
            ));
        }
        match response.chunk().await {
            Ok(Some(bytes)) => head.extend_from_slice(&bytes),
            Ok(None) => return Err(failed!("the camera ended its FLV stream before it began")),
            Err(error) => {
                return Err(failed!(
                    "the camera's FLV stream broke off: {}",
                    error.without_url()
                ));
            }
        }
    };
    if !said.h264() {
        return Err(MediaError::CannotPlay(
            "the camera sends this FLV stream as something other than H.264, the only video the \
             app takes out of FLV: pick one of its RTSP streams"
                .to_string(),
        ));
    }
    let offer = Offer {
        video: VideoCodec::H264,
        audio: said.aac().then_some(AudioCodec::Aac),
    };
    Ok((FlvAnswer { response, head }, offer))
}

/// A Reolink's FLV stream as a glass2glass source: the same bytes on each of its outputs, one per
/// track, for an `flvdemux` behind each.
pub(crate) struct FlvSrc {
    answer: Option<FlvAnswer>,
    outputs: usize,
}

impl FlvSrc {
    /// The source for `answer`, with `outputs` outputs: 1 for the video alone, 2 with the sound.
    pub(crate) fn new(answer: FlvAnswer, outputs: usize) -> Self {
        Self {
            answer: Some(answer),
            outputs,
        }
    }
}

/// `bytes` of the stream as a frame of a byte stream, which carries no timing: `flvdemux` reads
/// the tags' own.
fn chunk_frame(bytes: Vec<u8>, sequence: u64) -> Frame {
    Frame {
        domain: MemoryDomain::System(SystemSlice::from_boxed(bytes.into_boxed_slice())),
        timing: FrameTiming {
            arrival_ns: g2g_core::metrics::monotonic_ns(),
            ..FrameTiming::default()
        },
        sequence,
        meta: Default::default(),
    }
}

impl MultiOutputSource for FlvSrc {
    type RunFuture<'a>
        = Pin<Box<dyn Future<Output = Result<u64, G2gError>> + 'a>>
    where
        Self: 'a;

    fn output_count(&self) -> usize {
        self.outputs
    }

    fn output_caps(&self, output: usize) -> Result<Caps, G2gError> {
        if output < self.outputs {
            Ok(Caps::ByteStream {
                encoding: ByteStreamEncoding::Flv,
            })
        } else {
            Err(G2gError::CapsMismatch)
        }
    }

    // Live, as `rtspsrcn` is: a sink anchors presentation on the pipeline's start, so a slow start
    // plays late and catches up rather than becoming the stream's standing delay.
    fn latency(&self) -> LatencyReport {
        LatencyReport::live(0, None)
    }

    fn run<'a>(&'a mut self, out: &'a mut dyn MultiOutputSink) -> Self::RunFuture<'a> {
        Box::pin(async move {
            let FlvAnswer { mut response, head } =
                self.answer.take().ok_or(G2gError::NotConfigured)?;
            let mut sent = 0;
            let mut next = Some(head);
            loop {
                if let Some(bytes) = next.take().filter(|bytes| !bytes.is_empty()) {
                    for port in 1..self.outputs {
                        out.push_to(
                            port,
                            PipelinePacket::DataFrame(chunk_frame(bytes.clone(), sent)),
                        )
                        .await?;
                    }
                    out.push_to(0, PipelinePacket::DataFrame(chunk_frame(bytes, sent)))
                        .await?;
                    sent += 1;
                }
                next = match tokio::time::timeout(STALL, response.chunk()).await {
                    Err(_) => return Err(G2gError::Timeout),
                    Ok(Err(_)) => return Err(G2gError::Network(NetworkFailure::Stream)),
                    Ok(Ok(None)) => break,
                    Ok(Ok(Some(bytes))) => Some(bytes.to_vec()),
                };
            }
            for port in 0..self.outputs {
                out.push_to(port, PipelinePacket::Eos).await?;
            }
            Ok(sent)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, atomic::AtomicBool},
        thread,
    };

    use ipcamera_core::Brand;

    use super::*;
    use crate::{
        gate::Visibility,
        pipeline::{self, Source},
        record::{RecordControl, Recorder},
    };

    /// The start of an FLV stream as a Reolink sends it: the header with its flags unset, an
    /// `onMetaData` script, and the first video tag, of video codec `codec` (7 is H.264).
    fn flv_start(codec: u8) -> Vec<u8> {
        fn tag(kind: u8, body: &[u8]) -> Vec<u8> {
            let size = body.len() as u32;
            let mut tag = vec![kind];
            tag.extend_from_slice(&size.to_be_bytes()[1..]);
            tag.extend_from_slice(&[0; 7]);
            tag.extend_from_slice(body);
            tag.extend_from_slice(&(size + 11).to_be_bytes());
            tag
        }
        let mut script = vec![2, 0, 10];
        script.extend_from_slice(b"onMetaData");
        script.extend_from_slice(&[8, 0, 0, 0, 4]);
        for (key, value) in [
            ("width", 896.0),
            ("height", 672.0),
            ("videocodecid", f64::from(codec)),
            ("audiocodecid", 10.0),
        ] {
            script.extend_from_slice(&(key.len() as u16).to_be_bytes());
            script.extend_from_slice(key.as_bytes());
            script.push(0);
            script.extend_from_slice(&f64::to_be_bytes(value));
        }
        script.extend_from_slice(&[0, 0, 9]);
        let mut bytes = b"FLV\x01\x00\x00\x00\x00\x09\x00\x00\x00\x00".to_vec();
        bytes.extend(tag(18, &script));
        bytes.extend(tag(9, &[0x10 | codec, 0, 0, 0, 0]));
        bytes
    }

    /// A web server on a loopback port that answers one request with `body` as an FLV stream,
    /// and the address of a stream on it.
    fn serve_once(body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the bound address").port();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0; 1024];
                let _ = stream.read(&mut request);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: video/x-flv\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(&body);
            }
        });
        format!("http://127.0.0.1:{port}/flv?port=1935&app=bcs&stream=channel0_ext.bcs&token=t")
    }

    fn opened(body: Vec<u8>) -> Result<(FlvAnswer, Offer)> {
        let login = FlvLogin {
            url: serve_once(body),
            client: None,
        };
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(open(login))
    }

    #[test]
    fn an_flv_streams_head_is_its_offer_and_it_builds_a_graph_with_its_sound() {
        let (answer, offer) = opened(flv_start(7)).expect("the stream opens");
        assert_eq!(
            offer,
            Offer {
                video: VideoCodec::H264,
                audio: Some(AudioCodec::Aac),
            }
        );
        let graph = pipeline::build(
            Source::Flv(answer),
            &CameraConfig::for_brand(Brand::Reolink),
            &offer,
            Arc::new(Visibility::default()),
            Arc::new(AtomicBool::new(false)),
            1.0,
            Recorder::new(RecordControl::default()),
        )
        .expect("the graph should build");
        // The RTSP graph with sound's 15, and an `flvdemux` for each of the video and the sound.
        assert_eq!(graph.node_count(), 17);
    }

    #[test]
    fn an_flv_stream_of_h265_cant_be_played() {
        assert!(matches!(
            opened(flv_start(12)),
            Err(MediaError::CannotPlay(_))
        ));
        assert!(matches!(
            opened(b"<html>not found</html>".to_vec()),
            Err(MediaError::Failed(_) | MediaError::CannotPlay(_))
        ));
    }

    #[test]
    fn a_camera_with_no_password_is_never_logged_in_to_for_its_flv_stream() {
        let camera = CameraConfig {
            host: "192.0.2.1".into(),
            user: "admin".into(),
            ..CameraConfig::for_brand(Brand::Reolink)
        };
        assert!(matches!(
            log_in(&camera, "channel0_ext.bcs"),
            Err(MediaError::CannotPlay(_))
        ));
    }

    #[test]
    fn its_source_hands_each_track_the_same_bytes() {
        let source = FlvSrc {
            answer: None,
            outputs: 2,
        };
        assert_eq!(source.output_count(), 2);
        assert_eq!(
            source.output_caps(1).unwrap(),
            Caps::ByteStream {
                encoding: ByteStreamEncoding::Flv
            }
        );
        assert!(source.output_caps(2).is_err());
    }
}
