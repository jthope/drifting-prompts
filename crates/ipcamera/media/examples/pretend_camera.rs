//! A pretend camera on this machine, for trying the app without a real one: a moving test pattern,
//! encoded to H.264 by Media Foundation and served over RTSP by glass2glass, with no login and no
//! sound. Unlike `IPCAMERA_SOURCE=pattern`, it reaches the app as a camera stream does, so
//! everything a stream feeds works on it, recording and the server included.
//!
//! ```text
//! cargo run -p ipcamera-media --features pretend-camera --example pretend_camera -- [port] [minutes]
//! ```
//!
//! It listens on `127.0.0.1:8554` for 20 minutes unless told otherwise. Add a camera in the app
//! with the address `127.0.0.1`, that RTSP port and the RTSP path `/stream`, then press Connect.

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::net::TcpListener;

    use ipcamera_media::pretend::{self, Pattern};

    const WIDTH: u32 = 1280;
    const HEIGHT: u32 = 720;
    const FPS: u32 = 25;

    let mut arguments = std::env::args().skip(1);
    let port: u16 = arguments.next().map_or(Ok(8554), |port| port.parse())?;
    let minutes: u64 = arguments.next().map_or(Ok(20), |minutes| minutes.parse())?;

    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!(
        "rtsp://127.0.0.1:{port}/stream, {WIDTH}x{HEIGHT} H.264 at {FPS} fps, for {minutes} min"
    );
    pretend::serve(
        listener,
        Pattern {
            width: WIDTH,
            height: HEIGHT,
            fps: FPS,
            frames: minutes * 60 * u64::from(FPS),
        },
    )?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The pretend camera encodes with Media Foundation, so it runs on Windows only.");
}
