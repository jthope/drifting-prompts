//! A pretend camera on this machine: a moving H.264 test pattern over RTSP at `/stream`, with no
//! login and no sound.

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
