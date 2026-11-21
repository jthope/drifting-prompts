//! Builds a camera's RTSP URL. Credentials never go in it: the RTSP source logs the URL.

use crate::camera_config::{Brand, CameraConfig, Stream};

/// Dahua/Amcrest live path.
pub fn default_path(channel: u32, stream: Stream) -> String {
    format!(
        "/cam/realmonitor?channel={channel}&subtype={}",
        stream.index()
    )
}

/// Reolink's live path, for either codec; some firmware needs `/h264Preview_01_main` instead.
pub fn reolink_path(channel: u32, stream: Stream) -> String {
    let stream = match stream {
        Stream::Main => "main",
        Stream::Sub => "sub",
    };
    format!("/Preview_{channel:02}_{stream}")
}

/// The path a blank override stands for, by brand; any other brand gets Dahua's.
pub fn path_for(brand: Brand, channel: u32, stream: Stream) -> String {
    match brand {
        Brand::Reolink => reolink_path(channel, stream),
        Brand::Dahua | Brand::Other => default_path(channel, stream),
    }
}

/// The stream URL, without credentials.
pub fn build(camera: &CameraConfig) -> String {
    let mut path = if camera.rtsp_path_override.trim().is_empty() {
        path_for(camera.brand, camera.channel, camera.stream)
    } else {
        camera.rtsp_path_override.trim().to_string()
    };

    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    while path.len() > 1 && path.ends_with('/') {
        path.pop();
    }
    if path == "/" {
        path.clear();
    }

    format!("rtsp://{}:{}{}", camera.host, camera.rtsp_port, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> CameraConfig {
        CameraConfig::default()
    }

    #[test]
    fn builds_the_default_dahua_path() {
        let mut p = camera();
        p.host = "192.0.2.10".into();
        p.channel = 1;
        p.stream = Stream::Main;
        assert_eq!(
            build(&p),
            "rtsp://192.0.2.10:554/cam/realmonitor?channel=1&subtype=0"
        );
    }

    #[test]
    fn a_reolink_camera_gets_reolinks_path() {
        let mut p = camera();
        p.brand = Brand::Reolink;
        p.host = "192.0.2.11".into();
        assert_eq!(build(&p), "rtsp://192.0.2.11:554/Preview_01_main");
        p.stream = Stream::Sub;
        p.channel = 2;
        assert!(build(&p).ends_with("/Preview_02_sub"));
        p.rtsp_path_override = "/h264Preview_01_main".into();
        assert!(
            build(&p).ends_with("/h264Preview_01_main"),
            "an override wins"
        );
    }

    #[test]
    fn switches_to_the_sub_stream() {
        let mut p = camera();
        p.stream = Stream::Sub;
        assert!(build(&p).ends_with("subtype=1"));
    }

    #[test]
    fn an_override_without_a_leading_slash_gets_one() {
        let mut p = camera();
        p.rtsp_path_override = "live".into();
        assert!(build(&p).ends_with(":554/live"));
    }

    #[test]
    fn a_bare_slash_override_produces_no_path() {
        let mut p = camera();
        p.rtsp_path_override = "/".into();
        assert_eq!(build(&p), format!("rtsp://{}:{}", p.host, p.rtsp_port));
    }
}
