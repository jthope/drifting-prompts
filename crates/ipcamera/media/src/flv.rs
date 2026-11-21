//! A Reolink's FLV stream: the login for its token, and the glass2glass source that plays it.
//!
//! A reconnect logs in again: the camera hangs up on a token whose login has ended. The address
//! carries the token, a login too, so no error repeats it.

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

const CONNECT_WITHIN: Duration = Duration::from_secs(5);

const HEAD_BYTES: usize = 256 * 1024;

/// Without it a stalled stream would show its last picture forever.
const STALL: Duration = Duration::from_secs(10);

/// The stream's address with a token in it; the client logs out when dropped.
pub(crate) struct FlvLogin {
    url: String,
    /// `None` only in tests, which have a stream and no camera to log in to.
    client: Option<ReolinkClient>,
}

/// Blocks, so never on an async runtime. A camera with no password is never logged in to, since a
/// blank login counts against the account.
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

pub(crate) struct FlvAnswer {
    response: reqwest::Response,
    head: Vec<u8>,
}

pub(crate) async fn open(login: FlvLogin) -> Result<(FlvAnswer, Offer)> {
    let FlvLogin { url, client } = login;
    let http = reqwest::Client::builder()
        // Cameras sign their own certificates.
        .tls_danger_accept_invalid_certs(true)
        .connect_timeout(CONNECT_WITHIN)
        .build()
        .map_err(|error| failed!("{}", error.without_url()))?;
    let sent = http.get(&url).send().await;
    // Once the stream has started the login can end; the camera goes on sending.
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

/// The same bytes on each output, one per track, for an `flvdemux` behind each.
pub(crate) struct FlvSrc {
    answer: Option<FlvAnswer>,
    outputs: usize,
}

impl FlvSrc {
    pub(crate) fn new(answer: FlvAnswer, outputs: usize) -> Self {
        Self {
            answer: Some(answer),
            outputs,
        }
    }
}

/// No timing: `flvdemux` reads the tags' own.
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

    // Live, so a slow start catches up rather than becoming a standing delay.
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
        // RTSP's 15 with sound, plus an `flvdemux` per track.
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
