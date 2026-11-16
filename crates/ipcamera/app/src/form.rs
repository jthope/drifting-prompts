//! The settings form and the camera's settings it edits, the brands it offers, and what Detect
//! fills in.

use std::cmp::Reverse;

use ipcamera_core::{
    Brand, CameraConfig, CameraFeatures, CameraStream, Detection, DeviceInfo, EventVideo, Lens,
    Stream, TileStream, rtsp_url,
};

// Shared with the web preview, so both refuse the same forms.
use crate::bridge::{
    BrandInfo, CameraForm, DetectedCamera, DetectionInfo, EventVideoSettings, FoundCamera,
    StreamInfo,
};
pub use crate::form_check::problems;

/// `video` as Settings → Automation and the camera form show it.
pub fn event_video_settings(video: &EventVideo) -> EventVideoSettings {
    EventVideoSettings {
        after_seconds: video.after_seconds,
        before_seconds: video.before_seconds,
        longest_minutes: video.longest_minutes,
        motion_continues: video.motion_continues,
        stream: video.stream.key().to_string(),
        kinds: video
            .kinds
            .iter()
            .map(|kind| kind.key().to_string())
            .collect(),
    }
}

/// The event video settings `settings` hold, within the app's ranges: a blank stream is the
/// camera's own, and a kind this build doesn't know is left out.
pub fn event_video_of(settings: &EventVideoSettings) -> EventVideo {
    EventVideo {
        after_seconds: settings.after_seconds,
        before_seconds: settings.before_seconds,
        longest_minutes: settings.longest_minutes,
        motion_continues: settings.motion_continues,
        stream: TileStream::from_key(&settings.stream).unwrap_or(TileStream::Own),
        kinds: settings
            .kinds
            .iter()
            .filter_map(|key| Detection::from_key(key))
            .collect(),
    }
    .within_limits()
}

/// The form filled in from `camera`. A camera that follows Settings → Automation's event video
/// settings shows the app's defaults for them, which the desktop replaces with the app's own.
pub fn from_config(camera: &CameraConfig) -> CameraForm {
    let mut form = CameraForm {
        name: camera.name.clone(),
        brand: camera.brand.key().to_string(),
        host: camera.host.clone(),
        user: camera.user.clone(),
        password: camera.password.clone(),
        rtsp_port: camera.rtsp_port.to_string(),
        channel: camera.channel.to_string(),
        sub_stream: u32::from(camera.stream.index()),
        rtsp_path: camera.rtsp_path_override.clone(),
        use_tcp: camera.use_tcp,
        auto_reconnect: camera.auto_reconnect,
        onvif_port: camera.onvif_port.to_string(),
        dvrip_port: camera.dvrip_port.to_string(),
        onvif_path: camera.onvif_device_path.clone(),
        api_port: camera.api_port.to_string(),
        api_https: camera.api_https,
        http_api: camera.http_api,
        onvif_events: camera.onvif_events,
        password_note: String::new(),
        streams: stream_infos(&camera.streams),
        onvif_refused: camera.onvif_refused,
        log_events: camera.log_events,
        own_event_video: camera.event_video.is_some(),
        event_video: event_video_settings(&camera.event_video.clone().unwrap_or_default()),
    };
    choose_stream(&mut form);
    form
}

/// Gives a form whose camera lists its streams one of them, where it names none yet: the one at
/// the path the brand, channel and quality make, which is what has played so far, else the first
/// the camera lists. A path typed by hand stays. The form of a camera that lists none is left
/// alone.
fn choose_stream(form: &mut CameraForm) {
    if form.streams.is_empty() || !form.rtsp_path.trim().is_empty() {
        return;
    }
    let built = rtsp_url::path_for(
        Brand::from_key(&form.brand).unwrap_or(Brand::Other),
        number(&form.channel, 1),
        Stream::from_index(u64::from(form.sub_stream)).unwrap_or_default(),
    );
    let chosen = form
        .streams
        .iter()
        .find(|stream| stream.path.eq_ignore_ascii_case(&built))
        .unwrap_or(&form.streams[0]);
    form.rtsp_path = chosen.path.clone();
}

/// What a list shows for a stream: its picture's `size`, its `codec` and its `name`, those that
/// are known: `1920×1080 · H.264 · Profile003_AutotrackStream`.
pub fn stream_title(size: &str, codec: &str, name: &str) -> String {
    [size, codec, name]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A stream's picture size as the lists write it, `1920×1080`; `""` when the camera didn't say.
pub fn size_text(stream: &CameraStream) -> String {
    stream
        .size()
        .map(|(width, height)| format!("{width}×{height}"))
        .unwrap_or_default()
}

/// Where a stream comes in the lists: by its `lens`, the main lens's first and then each other
/// lens's in the order `lenses` (each stream's, as listed) first has it, then the largest picture
/// first, those of an unknown `size` last. A key for `sort_by_key`, which keeps the camera's own
/// order between equals.
pub fn stream_order(
    lenses: &[Option<Lens>],
    lens: Option<Lens>,
    size: Option<(u32, u32)>,
) -> (usize, bool, Reverse<u64>) {
    let rank = lens.map_or(0, |lens| {
        let mut seen: Vec<Lens> = Vec::new();
        for each in lenses.iter().flatten() {
            if !seen.contains(each) {
                seen.push(*each);
            }
        }
        1 + seen
            .iter()
            .position(|each| *each == lens)
            .unwrap_or(seen.len())
    });
    let pixels = size.map(|(width, height)| u64::from(width) * u64::from(height));
    (rank, pixels.is_none(), Reverse(pixels.unwrap_or(0)))
}

/// The streams a camera listed, as the form shows them: in the lists' order ([`stream_order`]),
/// each with its title and lens.
pub fn stream_infos(streams: &[CameraStream]) -> Vec<StreamInfo> {
    let lenses: Vec<Option<Lens>> = streams.iter().map(CameraStream::lens).collect();
    let mut ordered: Vec<&CameraStream> = streams.iter().collect();
    ordered.sort_by_key(|stream| stream_order(&lenses, stream.lens(), stream.size()));
    ordered
        .into_iter()
        .map(|stream| {
            let name = match stream.name.trim() {
                "" => stream.path.trim(),
                name => name,
            };
            StreamInfo {
                name: stream.name.clone(),
                path: stream.path.clone(),
                detail: stream.detail.clone(),
                title: stream_title(
                    &size_text(stream),
                    &stream.codec().unwrap_or_default(),
                    name,
                ),
                lens: stream
                    .lens()
                    .map(|lens| lens.title().to_string())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

/// Every brand the form offers, in order.
pub fn brands() -> Vec<BrandInfo> {
    Brand::ALL
        .into_iter()
        .map(|brand| BrandInfo {
            key: brand.key().to_string(),
            title: brand.title().to_string(),
            rtsp_path: rtsp_url::path_for(brand, 1, Stream::Main),
        })
        .collect()
}

/// Everything the app can tell apart in a picture, in order.
pub fn detections() -> Vec<DetectionInfo> {
    Detection::ALL
        .into_iter()
        .map(|detection| DetectionInfo {
            key: detection.key().to_string(),
            title: detection.title().to_string(),
        })
        .collect()
}

/// `form` with brand `key`. A port or path still at the old brand's default takes the new one's;
/// anything the user changed stays. An unknown key leaves the form as it is.
pub fn with_brand(form: &CameraForm, key: &str) -> CameraForm {
    let Some(brand) = Brand::from_key(key) else {
        return form.clone();
    };
    let old = from_config(&CameraConfig::for_brand(
        Brand::from_key(&form.brand).unwrap_or_default(),
    ));
    let new = from_config(&CameraConfig::for_brand(brand));

    let mut changed = form.clone();
    changed.brand = new.brand.clone();
    for (field, was, becomes) in [
        (&mut changed.rtsp_port, &old.rtsp_port, &new.rtsp_port),
        (&mut changed.onvif_port, &old.onvif_port, &new.onvif_port),
        (&mut changed.dvrip_port, &old.dvrip_port, &new.dvrip_port),
        (&mut changed.onvif_path, &old.onvif_path, &new.onvif_path),
        (&mut changed.api_port, &old.api_port, &new.api_port),
    ] {
        if field.trim() == was.as_str() {
            field.clone_from(becomes);
        }
    }
    if form.api_port.trim() == old.api_port {
        changed.api_https = new.api_https;
    }
    changed
}

/// What Detect found, as `ipcamera_core::detect` and the server's `DetectReply` both say it.
pub struct Found<'a> {
    /// Who made the camera.
    pub brand: Brand,
    /// Where its HTTP API answered (its port, and whether it is HTTPS): a Reolink's or a Dahua's.
    pub api: Option<(u16, bool)>,
    /// For a Dahua: whether its PTZ and light go over that API rather than PTZ over DVRIP.
    pub http_api: bool,
    /// What it said it is, when it was asked with a login.
    pub device: Option<&'a DeviceInfo>,
    /// What it said it has, when it was asked with a login.
    pub features: Option<&'a CameraFeatures>,
    /// Whether it was asked with a password, to tell a Reolink's user to add one.
    pub asked_with_password: bool,
    /// Whether its ONVIF events report motion, or it has a light over ONVIF, which turns
    /// watching it over ONVIF on.
    pub onvif_events: bool,
    /// The port ONVIF answered on, which the form takes.
    pub onvif_port: Option<u16>,
    /// Why ONVIF told nothing of a camera found another way, said after the rest.
    pub onvif_problem: Option<&'a str>,
    /// The streams the camera listed, which the form then offers; a camera asked without a
    /// login lists none, and the form keeps those it had.
    pub streams: &'a [CameraStream],
    /// Whether its ONVIF refused the login, now or before: the form remembers it.
    pub onvif_refused: bool,
}

/// The cameras found on the network, as the form's network search lists them.
pub fn found_cameras(found: Vec<ipcamera_core::FoundCamera>) -> Vec<FoundCamera> {
    // A port as the form keeps its numbers, as text; none where the search didn't learn it.
    let text = |port: Option<u16>| port.map(|port| port.to_string()).unwrap_or_default();
    found
        .into_iter()
        .map(|camera| FoundCamera {
            host: camera.host,
            name: camera.name,
            model: camera.model,
            onvif_port: text(camera.onvif_port),
            onvif_path: camera.onvif_device_path.unwrap_or_default(),
            rtsp_port: text(camera.rtsp_port),
            details: camera.details,
        })
        .collect()
}

/// What Connect shows in a run that reaches no camera, for the reason `why`: the form as it is,
/// and nothing found. It answers rather than fails, so such a run can still add a camera.
pub fn nothing_to_ask(form: &CameraForm, why: &str) -> DetectedCamera {
    DetectedCamera {
        form: form.clone(),
        summary: why.to_string(),
        features: Vec::new(),
    }
}

/// What Detect shows for what it `found`, with `form` filled in from it.
pub fn detected(form: &CameraForm, found: Found<'_>) -> DetectedCamera {
    let Found {
        brand,
        api,
        http_api,
        device,
        features,
        asked_with_password,
        onvif_events,
        onvif_port,
        onvif_problem,
        streams,
        onvif_refused,
    } = found;
    let mut form = with_brand(form, brand.key());
    form.onvif_refused = onvif_refused;
    if !streams.is_empty() {
        form.streams = stream_infos(streams);
    }
    choose_stream(&mut form);
    if let Some((port, https)) = api {
        form.api_port = port.to_string();
        form.api_https = https;
    }
    form.http_api = brand == Brand::Dahua && http_api;
    if onvif_events {
        form.onvif_events = true;
    }
    if let Some(port) = onvif_port {
        form.onvif_port = port.to_string();
    }

    // ONVIF's model names its maker (`Dahua IPC-HDW2431T`), Reolink's doesn't (`RLC-811A`).
    let named = |model: &str| {
        let lower = model.to_ascii_lowercase();
        let maker_named = brand
            .title()
            .split(|c: char| !c.is_ascii_alphabetic())
            .filter(|word| !word.is_empty())
            .any(|word| lower.contains(&word.to_ascii_lowercase()));
        if maker_named || brand == Brand::Other {
            model.to_string()
        } else {
            format!("{} {model}", brand.title())
        }
    };
    let summary = match device.filter(|device| !device.model.is_empty()) {
        Some(device) if device.firmware.is_empty() => format!("{}.", named(&device.model)),
        Some(device) => format!("{}, firmware {}.", named(&device.model), device.firmware),
        None if brand == Brand::Reolink && !asked_with_password => {
            "A Reolink camera. Enter its password, then detect again for its model and what it \
             has."
                .to_string()
        }
        None if brand == Brand::Dahua && !asked_with_password => format!(
            "A Dahua camera: its {} answers, and PTZ goes over it. Enter its password, then \
             detect again for its model, its light and ONVIF events.",
            if http_api { "HTTP API" } else { "DVRIP port" }
        ),
        None if brand == Brand::Dahua => format!(
            "A Dahua camera: its {} answers, and PTZ goes over it.",
            if http_api { "HTTP API" } else { "DVRIP port" }
        ),
        None if !asked_with_password => {
            format!(
                "A camera whose ONVIF answers{}. Enter its password, then detect again for its \
                 model and what it has.",
                onvif_port
                    .map(|port| format!(" on port {port}"))
                    .unwrap_or_default()
            )
        }
        None => "A camera whose ONVIF answers.".to_string(),
    };
    let summary = match onvif_problem {
        // The camera has been reached another way, so this is a note and reads as one.
        Some(_) if onvif_refused => format!(
            "{summary} Its ONVIF service, which only adds motion events here, didn't take this \
             login and is left alone: many cameras keep a separate account for ONVIF."
        ),
        Some(problem) => {
            format!("{summary} Its ONVIF service, which is optional, didn't help: {problem}")
        }
        None => summary,
    };

    DetectedCamera {
        form,
        summary,
        features: features
            .map(|features| features.titles().into_iter().map(String::from).collect())
            .unwrap_or_default(),
    }
}

/// The form for a new camera: the usual ports and paths, and `admin` as the username, with no
/// name, address or password yet.
pub fn blank() -> CameraForm {
    CameraForm {
        // Dahua and Amcrest cameras ship with this username, so most new cameras keep it.
        user: "admin".into(),
        ..from_config(&CameraConfig::default())
    }
}

/// The form as a camera's settings, with the defaults for anything left blank or not a number.
pub fn to_config(form: &CameraForm) -> CameraConfig {
    // A brand this build doesn't know, from a newer app's form, is no brand in particular.
    let mut camera = CameraConfig::for_brand(Brand::from_key(&form.brand).unwrap_or(Brand::Other));
    camera.name = form.name.trim().to_string();
    camera.host = form.host.trim().to_string();
    camera.user = form.user.trim().to_string();
    // A password can begin or end with a space.
    camera.password = form.password.clone();
    camera.rtsp_port = number(&form.rtsp_port, camera.rtsp_port);
    camera.channel = number(&form.channel, camera.channel);
    camera.stream = Stream::from_index(u64::from(form.sub_stream)).unwrap_or_default();
    camera.rtsp_path_override = form.rtsp_path.trim().to_string();
    camera.use_tcp = form.use_tcp;
    camera.auto_reconnect = form.auto_reconnect;
    camera.onvif_port = number(&form.onvif_port, camera.onvif_port);
    camera.dvrip_port = number(&form.dvrip_port, camera.dvrip_port);
    camera.onvif_device_path = form.onvif_path.trim().to_string();
    camera.api_port = number(&form.api_port, camera.api_port);
    camera.api_https = form.api_https;
    camera.http_api = form.http_api;
    camera.onvif_events = form.onvif_events;
    camera.streams = form
        .streams
        .iter()
        .map(|stream| CameraStream {
            name: stream.name.clone(),
            path: stream.path.clone(),
            detail: stream.detail.clone(),
        })
        .collect();
    camera.onvif_refused = form.onvif_refused;
    camera.log_events = form.log_events;
    camera.event_video = form
        .own_event_video
        .then(|| event_video_of(&form.event_video));
    camera
}

fn number<T: std::str::FromStr>(text: &str, fallback: T) -> T {
    text.trim().parse().ok().unwrap_or(fallback)
}

#[cfg(test)]
mod tests;
