//! How large each of a camera's RTSP streams is, from its answer to DESCRIBE, for Detect.
//!
//! The first refused login ends the asking, so a wrong password costs one try.

use std::time::Duration;

use ipcamera_core::{Brand, CameraConfig, CameraStream, Stream, StreamPath, rtsp_url};
use retina::{
    client::{Session, SessionOptions},
    codec::ParametersRef,
};
use url::Url;

use crate::pipeline::{USER_AGENT, credentials};

const ASK_WAIT: Duration = Duration::from_secs(5);

enum Answer {
    Sized(String),
    /// The camera refused the login: ask it nothing more.
    Refused,
    Unsized,
}

/// The brand's main and sub only where the camera listed no RTSP stream: one that did lists them
/// under other paths.
fn unsized_paths(camera: &CameraConfig, listed: &[CameraStream]) -> Vec<(String, String)> {
    let is_rtsp = |path: &str| matches!(StreamPath::of(path), StreamPath::Rtsp(_));
    let brand_path = |stream| rtsp_url::path_for(camera.brand, camera.channel, stream);
    let known_brand = camera.brand != Brand::Other;
    let brand_name = |path: &str| {
        [Stream::Main, Stream::Sub]
            .into_iter()
            .find(|stream| known_brand && brand_path(*stream).eq_ignore_ascii_case(path))
            .map(|stream| stream.label().to_string())
            .unwrap_or_default()
    };
    let own = match camera.rtsp_path_override.trim() {
        "" => Some(brand_path(camera.stream)),
        path if is_rtsp(path) => Some(path.to_string()),
        _ => None,
    }
    .map(|path| {
        let name = brand_name(&path);
        (path, name)
    });
    let lists_rtsp = listed.iter().any(|stream| is_rtsp(&stream.path));
    let brand = (known_brand && !lists_rtsp)
        .then(|| {
            [Stream::Main, Stream::Sub]
                .map(|stream| (brand_path(stream), stream.label().to_string()))
        })
        .into_iter()
        .flatten();
    let unsized_listed = listed
        .iter()
        .filter(|stream| is_rtsp(&stream.path) && stream.size().is_none())
        .map(|stream| (stream.path.trim().to_string(), stream.name.clone()));

    let mut paths: Vec<(String, String)> = Vec::new();
    for (path, name) in own.into_iter().chain(brand).chain(unsized_listed) {
        let sized = listed.iter().any(|stream| {
            stream.path.trim().eq_ignore_ascii_case(&path) && stream.size().is_some()
        });
        if !sized
            && !paths
                .iter()
                .any(|(known, _)| known.eq_ignore_ascii_case(&path))
        {
            paths.push((path, name));
        }
    }
    paths
}

/// The streams `listed` doesn't size that the camera sizes; blocks, so never on an async runtime.
pub fn sized_streams(camera: &CameraConfig, listed: &[CameraStream]) -> Vec<CameraStream> {
    if camera.password.is_empty() {
        return Vec::new();
    }
    let paths = unsized_paths(camera, listed);
    if paths.is_empty() {
        return Vec::new();
    }
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Vec::new();
    };
    runtime.block_on(async {
        let mut sized = Vec::new();
        for (path, name) in paths {
            let mut at = camera.clone();
            at.rtsp_path_override = path.clone();
            let answer = tokio::time::timeout(ASK_WAIT, ask(&rtsp_url::build(&at), camera))
                .await
                .unwrap_or(Answer::Unsized);
            match answer {
                Answer::Sized(detail) => sized.push(CameraStream { name, path, detail }),
                Answer::Refused => break,
                Answer::Unsized => {}
            }
        }
        // A lone stream would give Auto nothing to pick between.
        if listed.is_empty() && sized.len() < 2 {
            sized.clear();
        }
        sized
    })
}

async fn ask(url: &str, camera: &CameraConfig) -> Answer {
    let Ok(parsed) = Url::parse(url) else {
        return Answer::Unsized;
    };
    let options = SessionOptions::default()
        .creds(credentials(camera))
        .user_agent(USER_AGENT.to_string());
    let session = match Session::describe(parsed, options).await {
        Ok(session) => session,
        Err(error) if error.status_code() == Some(401) => return Answer::Refused,
        Err(_) => return Answer::Unsized,
    };
    session
        .streams()
        .iter()
        .filter(|stream| stream.media() == "video")
        .find_map(|stream| {
            let codec = match stream.encoding_name() {
                "h264" => "H264",
                "h265" => "H265",
                _ => return None,
            };
            let Some(ParametersRef::Video(video)) = stream.parameters() else {
                return None;
            };
            let (width, height) = video.pixel_dimensions();
            (width > 0 && height > 0).then(|| format!("{width}×{height}, {codec}"))
        })
        .map_or(Answer::Unsized, Answer::Sized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(path: &str, detail: &str) -> CameraStream {
        CameraStream {
            name: String::new(),
            path: path.into(),
            detail: detail.into(),
        }
    }

    #[test]
    fn a_camera_that_listed_nothing_has_its_own_main_and_sub_sized() {
        let camera = CameraConfig::for_brand(Brand::Dahua);
        let path = |path: &str, name: &str| (path.to_string(), name.to_string());
        assert_eq!(
            unsized_paths(&camera, &[]),
            [
                path("/cam/realmonitor?channel=1&subtype=0", "main"),
                path("/cam/realmonitor?channel=1&subtype=1", "sub")
            ]
        );
        let mut other = CameraConfig::for_brand(Brand::Other);
        other.rtsp_path_override = "/live1".into();
        assert_eq!(unsized_paths(&other, &[]), [path("/live1", "")]);
    }

    #[test]
    fn what_a_camera_listed_with_a_size_is_not_asked_again() {
        let mut camera = CameraConfig::for_brand(Brand::Reolink);
        camera.rtsp_path_override = "/Preview_01_autotrack".into();
        let mut third = listed("/onvif/profile3", "");
        third.name = "Profile 3".into();
        let streams = [
            listed("/Preview_01_main", "3840×2160, H265"),
            listed("/Preview_01_autotrack", "1920×1080, H264"),
            third,
            listed("flv:channel0_autotrack_sub.bcs", ""),
        ];
        assert_eq!(
            unsized_paths(&camera, &streams),
            [("/onvif/profile3".to_string(), "Profile 3".to_string())]
        );

        camera.rtsp_path_override = "flv:channel0_ext.bcs".into();
        assert!(unsized_paths(&camera, &streams[..2]).is_empty());
    }

    #[test]
    fn a_camera_with_no_password_is_never_asked() {
        let camera = CameraConfig::for_brand(Brand::Dahua);
        assert!(camera.password.is_empty());
        assert!(sized_streams(&camera, &[]).is_empty());
    }
}
