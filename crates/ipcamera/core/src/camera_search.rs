//! Finding the cameras on this machine's networks, for the camera form's network search, whatever
//! they speak. Nothing here logs in to anything.
//!
//! Two searches run side by side, about three seconds together:
//!
//! - **ONVIF's own** ([`onvif::discover`](crate::onvif::discover)): a camera with ONVIF on says
//!   where its ONVIF service is, its name and its model.
//! - **For video** ([`video_devices`]): many cameras have no ONVIF, or have it off, and still
//!   send video over RTSP. Each address of this machine's private networks is tried on RTSP's
//!   port, and one that takes the connection is asked which requests it takes (`OPTIONS`, which
//!   needs no login and names no stream). A device that answers in RTSP is listed by its address,
//!   with what its RTSP server calls itself: it says nothing else without a login.
//!
//! A camera both find is one entry, ONVIF's, with its RTSP port. Only RTSP's usual port is tried:
//! a camera moved to another port, like one on another network, is added by its address.

use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    thread,
    time::Duration,
};

use crate::{camera_features::FoundCamera, onvif};

/// RTSP's port, which cameras ship their video on.
const RTSP_PORT: u16 = 554;

/// How long an address may take to accept a connection on RTSP's port: one on the same network
/// answers within a few milliseconds, and nothing at an address never does.
const CONNECT_WAIT: Duration = Duration::from_millis(800);

/// How long a device that took the connection may take to answer.
const ANSWER_WAIT: Duration = Duration::from_millis(1200);

/// The stack of the thread that tries one address: it holds a small buffer and nothing else.
const TRY_STACK: usize = 64 * 1024;

/// The cameras on this machine's networks: those ONVIF's discovery finds, and every other device
/// that answers in RTSP, by address. No camera is logged in to. Blocks for about three seconds,
/// so call it off the window's thread and off any async runtime's own threads.
pub fn find_cameras() -> Vec<FoundCamera> {
    let (over_onvif, with_video) = thread::scope(|scope| {
        let video = scope.spawn(video_devices);
        (onvif::discover(), video.join().unwrap_or_default())
    });
    merged(over_onvif, with_video)
}

/// `over_onvif` and `with_video` as one list, by address: a camera in both is ONVIF's entry with
/// its RTSP port, and a device only RTSP found comes after those ONVIF did.
fn merged(mut over_onvif: Vec<FoundCamera>, with_video: Vec<FoundCamera>) -> Vec<FoundCamera> {
    let mut only_video = Vec::new();
    for device in with_video {
        match over_onvif
            .iter_mut()
            .find(|found| found.host == device.host)
        {
            Some(found) => found.rtsp_port = device.rtsp_port,
            None => only_video.push(device),
        }
    }
    over_onvif.extend(only_video);
    over_onvif
}

/// Every device on this machine's private networks that answers in RTSP on RTSP's port, by
/// address. Each address is tried at once, on a thread of its own, so the whole search takes
/// what one address may.
fn video_devices() -> Vec<FoundCamera> {
    let addresses: Vec<Ipv4Addr> = onvif::private_networks()
        .into_iter()
        .flat_map(onvif::neighbors)
        .collect();
    let mut found: Vec<FoundCamera> = thread::scope(|scope| {
        let tries: Vec<_> = addresses
            .iter()
            .filter_map(|address| {
                thread::Builder::new()
                    .stack_size(TRY_STACK)
                    .spawn_scoped(scope, move || video_device(*address))
                    .ok()
            })
            .collect();
        tries
            .into_iter()
            .filter_map(|tried| tried.join().ok().flatten())
            .collect()
    });
    found.sort_by_key(|device| device.host.parse::<Ipv4Addr>().ok());
    found
}

/// The device at `address`, when it answers in RTSP: asked `OPTIONS` for the server itself
/// (`*`), with no login and no stream named.
fn video_device(address: Ipv4Addr) -> Option<FoundCamera> {
    let mut stream =
        TcpStream::connect_timeout(&SocketAddr::from((address, RTSP_PORT)), CONNECT_WAIT).ok()?;
    stream.set_read_timeout(Some(ANSWER_WAIT)).ok()?;
    stream.set_write_timeout(Some(ANSWER_WAIT)).ok()?;
    let request = format!(
        "OPTIONS rtsp://{address}:{RTSP_PORT}/ RTSP/1.0\r\nCSeq: 1\r\nUser-Agent: IPCamera\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).ok()?;
    let mut reply = [0; 1024];
    let read = stream.read(&mut reply).ok()?;
    let server = rtsp_server(&String::from_utf8_lossy(&reply[..read]))?;
    Some(FoundCamera {
        host: address.to_string(),
        rtsp_port: Some(RTSP_PORT),
        details: vec![match server.as_str() {
            "" => "Video over RTSP".to_string(),
            server => format!("Video over RTSP: {server}"),
        }],
        ..FoundCamera::default()
    })
}

/// What the RTSP server that sent `reply` calls itself, empty when it doesn't say; `None` for a
/// reply that isn't RTSP's. Any RTSP answer counts, one asking for a login included: it is a
/// device that speaks RTSP either way.
fn rtsp_server(reply: &str) -> Option<String> {
    let mut lines = reply.lines();
    if !lines.next()?.starts_with("RTSP/1.") {
        return None;
    }
    let server = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("server"))
        .map(|(_, value)| value.trim().to_string());
    Some(server.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_is_one_that_answers_in_rtsp_whatever_it_answers() {
        assert_eq!(
            rtsp_server(
                "RTSP/1.0 200 OK\r\nCSeq: 1\r\nServer: Rtsp Server/3.0\r\nPublic: OPTIONS, DESCRIBE\r\n\r\n"
            ),
            Some("Rtsp Server/3.0".into())
        );
        assert_eq!(
            rtsp_server("RTSP/1.0 401 Unauthorized\r\nCSeq: 1\r\n\r\n"),
            Some(String::new()),
            "it wants a login for this too, and is a camera all the same"
        );
        assert_eq!(rtsp_server("HTTP/1.1 400 Bad Request\r\n\r\n"), None);
        assert_eq!(rtsp_server(""), None);
    }

    #[test]
    fn a_camera_both_searches_find_is_listed_once_with_its_video_port() {
        let onvif = |host: &str| FoundCamera {
            host: host.into(),
            onvif_port: Some(80),
            onvif_device_path: Some("/onvif/device_service".into()),
            model: "IPC-123".into(),
            ..FoundCamera::default()
        };
        let video = |host: &str| FoundCamera {
            host: host.into(),
            rtsp_port: Some(554),
            details: vec!["Video over RTSP".into()],
            ..FoundCamera::default()
        };

        let found = merged(
            vec![onvif("10.0.0.5"), onvif("10.0.0.9")],
            vec![video("10.0.0.3"), video("10.0.0.5")],
        );

        let hosts: Vec<&str> = found.iter().map(|camera| camera.host.as_str()).collect();
        assert_eq!(hosts, ["10.0.0.5", "10.0.0.9", "10.0.0.3"]);
        assert_eq!(found[0].rtsp_port, Some(554));
        assert_eq!(found[0].model, "IPC-123", "what ONVIF said of it stays");
        assert!(found[0].details.is_empty());
        assert_eq!(found[1].rtsp_port, None);
        assert_eq!(found[2].onvif_port, None, "video alone");
    }
}
