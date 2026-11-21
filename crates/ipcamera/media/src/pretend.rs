//! A pretend camera on this machine: a moving test pattern, encoded to H.264 by Media Foundation
//! and served over RTSP at `/stream`, with no login and no sound.

use std::{net::TcpListener, thread};

use g2g_core::{
    RawVideoFormat,
    graph::Graph,
    runtime::{GraphNode, GraphNodeRef, LatencyProfile, run_graph},
};
use g2g_plugins::{
    clock::WallClock, clocksync::ClockSyncTransform, h264parse::H264Parse, mfencode::MfEncode,
    rtspserversink::RtspServerSink, videoconvert::VideoConvert, videotestsrc::VideoTestSrc,
};
use ipcamera_core::CameraConfig;

use crate::error::{Result, failed};

/// What a pretend camera sends.
#[derive(Clone, Copy, Debug)]
pub struct Pattern {
    /// The picture's width, in pixels.
    pub width: u32,
    /// The picture's height, in pixels.
    pub height: u32,
    /// Pictures a second, paced in real time.
    pub fps: u32,
    /// How many pictures it sends before it stops.
    pub frames: u64,
}

/// Serves `pattern` over RTSP on `listener` until its last picture, on this thread.
pub fn serve(listener: TcpListener, pattern: Pattern) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let mut graph: Graph<GraphNode> = Graph::new();
        let source = graph.add_source(GraphNodeRef::source(VideoTestSrc::new(
            pattern.width,
            pattern.height,
            pattern.fps,
            pattern.frames,
        )));
        let pace = graph.add_transform(GraphNodeRef::element(ClockSyncTransform::new()));
        let convert = graph.add_transform(GraphNodeRef::element(VideoConvert::new(
            RawVideoFormat::Nv12,
        )));
        let encode = graph.add_transform(GraphNodeRef::element(MfEncode::new()));
        let parse = graph.add_transform(GraphNodeRef::element(H264Parse::new()));
        let serve = graph.add_sink(GraphNodeRef::element(
            RtspServerSink::from_listener(listener)
                .map_err(|error| failed!("the server could not start: {error:?}"))?,
        ));
        for (from, to) in [
            (source, pace),
            (pace, convert),
            (convert, encode),
            (encode, parse),
            (parse, serve),
        ] {
            graph
                .link(from, to)
                .map_err(|error| failed!("the graph could not be linked: {error:?}"))?;
        }
        run_graph(graph, &WallClock::new(), LatencyProfile::Live)
            .await
            .map_err(|error| failed!("the pretend camera stopped: {error:?}"))?;
        Ok(())
    })
}

/// Serves `pattern` on a free local port from its own thread; returns a camera that reaches it.
pub fn start(pattern: Pattern) -> Result<CameraConfig> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    thread::Builder::new()
        .name("ipcamera-pretend-camera".into())
        .spawn(move || {
            let _ = serve(listener, pattern);
        })?;
    Ok(setting(port))
}

/// The settings of a camera that reaches a pretend camera on `port`.
pub fn setting(port: u16) -> CameraConfig {
    CameraConfig {
        name: "Pretend camera".into(),
        host: "127.0.0.1".into(),
        rtsp_port: port,
        // A session refuses a blank username; the pretend camera checks none.
        user: "admin".into(),
        rtsp_path_override: "/stream".into(),
        auto_reconnect: false,
        ..CameraConfig::default()
    }
}
