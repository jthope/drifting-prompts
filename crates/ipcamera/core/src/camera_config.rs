//! One camera's settings: who made it, how to reach it, its stream and its control ports. Its
//! [`Brand`] decides the default RTSP path and the protocols of its controls.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    camera_features::{FLV_PREFIX, StreamPath},
    camera_view::TileStream,
    ids::CameraId,
};

/// One camera. Every field has a default, so a hand-written entry only needs what differs.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraConfig {
    /// What groups and views call this camera by; empty until the camera store gives it one.
    pub id: CameraId,
    /// The name the user gave it.
    pub name: String,
    /// Who made it.
    pub brand: Brand,
    /// A host name or an IP address.
    pub host: String,
    /// The account it logs in with.
    pub user: String,
    /// That account's password, in the clear; never printed (see the `Debug` impl).
    pub password: String,
    /// The RTSP port.
    pub rtsp_port: u16,
    /// The camera's channel, from 1.
    pub channel: u32,
    /// The main stream or the sub stream, saved as `sub_stream`, 0 or 1.
    #[serde(rename = "sub_stream")]
    pub stream: Stream,
    /// The RTSP path in place of the brand's (`/` for no path), or a Reolink's `flv:` stream.
    pub rtsp_path_override: String,
    /// Whether RTSP carries the stream over TCP rather than UDP.
    pub use_tcp: bool,
    /// Whether a lost stream reconnects by itself.
    pub auto_reconnect: bool,
    /// The ONVIF service's port.
    pub onvif_port: u16,
    /// The ONVIF device service's path.
    pub onvif_device_path: String,
    /// The DVRIP port, for a Dahua's PTZ unless `http_api` is on.
    pub dvrip_port: u16,
    /// The port of the camera's own HTTP API: a Reolink's, or a Dahua's with `http_api` on.
    pub api_port: u16,
    /// Whether that API is HTTPS rather than plain HTTP.
    pub api_https: bool,
    /// For a Dahua: whether its PTZ and light go over its HTTP API rather than DVRIP.
    pub http_api: bool,
    /// Whether the app watches the camera's ONVIF events, for a camera that isn't a Reolink. Off by
    /// default: a failed ONVIF login counts against the account like any other.
    pub onvif_events: bool,
    /// The streams the camera listed when Detect last asked it with a login.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub streams: Vec<crate::CameraStream>,
    /// Whether ONVIF refused a login that works another way; Detect then leaves ONVIF alone, since
    /// every refused login counts against the account, until the address, login or port changes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub onvif_refused: bool,
    /// Whether the app logs what this camera sees as events.
    pub log_events: bool,
    /// This camera's own event video settings; `None` follows Settings → Automation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_video: Option<crate::EventVideo>,
}

/// No name, address or login, so a default is never mistaken for a camera that is set up.
impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            id: CameraId::default(),
            name: String::new(),
            brand: Brand::Dahua,
            host: String::new(),
            user: String::new(),
            password: String::new(),
            rtsp_port: 554,
            channel: 1,
            stream: Stream::Main,
            rtsp_path_override: String::new(),
            use_tcp: true,
            auto_reconnect: true,
            onvif_port: 80,
            onvif_device_path: "/onvif/device_service".into(),
            dvrip_port: 37777,
            api_port: 80,
            api_https: false,
            http_api: false,
            onvif_events: false,
            streams: Vec::new(),
            onvif_refused: false,
            log_events: true,
            event_video: None,
        }
    }
}

impl CameraConfig {
    /// This camera's event video settings, else `app`'s, within the app's ranges either way.
    pub fn event_video_or(&self, app: &crate::EventVideo) -> crate::EventVideo {
        self.event_video
            .clone()
            .unwrap_or_else(|| app.clone())
            .within_limits()
    }

    /// A new camera of `brand`, with that brand's usual ports.
    pub fn for_brand(brand: Brand) -> Self {
        let mut camera = Self {
            brand,
            ..Self::default()
        };
        if brand == Brand::Reolink {
            camera.onvif_port = 8000;
            camera.api_port = 443;
            camera.api_https = true;
        }
        camera
    }
}

/// Written by hand so the password never reaches a log or a failed test's output.
impl fmt::Debug for CameraConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let password = if self.password.is_empty() {
            ""
        } else {
            "<hidden>"
        };
        f.debug_struct("CameraConfig")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("brand", &self.brand)
            .field("host", &self.host)
            .field("user", &self.user)
            .field("password", &password)
            .field("rtsp_port", &self.rtsp_port)
            .field("channel", &self.channel)
            .field("stream", &self.stream)
            .field("rtsp_path_override", &self.rtsp_path_override)
            .field("use_tcp", &self.use_tcp)
            .field("auto_reconnect", &self.auto_reconnect)
            .field("onvif_port", &self.onvif_port)
            .field("onvif_device_path", &self.onvif_device_path)
            .field("dvrip_port", &self.dvrip_port)
            .field("api_port", &self.api_port)
            .field("api_https", &self.api_https)
            .field("http_api", &self.http_api)
            .field("onvif_events", &self.onvif_events)
            .field("streams", &self.streams)
            .field("onvif_refused", &self.onvif_refused)
            .field("log_events", &self.log_events)
            .field("event_video", &self.event_video)
            .finish()
    }
}

/// Who made a camera. An unknown brand reads as [`Other`](Self::Other), so the app never speaks
/// the wrong protocol to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Brand {
    /// Dahua, and cameras built on its firmware such as Amcrest's.
    #[default]
    Dahua,
    /// Reolink.
    Reolink,
    /// Any other camera.
    Other,
}

impl Brand {
    /// Every brand, in order.
    pub const ALL: [Brand; 3] = [Brand::Dahua, Brand::Reolink, Brand::Other];

    /// The name saved in `cameras.json`.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Dahua => "dahua",
            Self::Reolink => "reolink",
            Self::Other => "other",
        }
    }

    /// The brand saved as `key`, if there is one.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|brand| brand.key() == key.trim())
    }

    /// Its display name.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Dahua => "Dahua / Amcrest",
            Self::Reolink => "Reolink",
            Self::Other => "Other",
        }
    }
}

impl Serialize for Brand {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.key())
    }
}

impl<'de> Deserialize<'de> for Brand {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Ok(Self::from_key(&key).unwrap_or(Self::Other))
    }
}

/// Which of a camera's two streams. Saved as 0 or 1; any other number reads as the main stream
/// rather than making the file unreadable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Stream {
    /// The camera's full picture.
    #[default]
    Main,
    /// A smaller picture that costs less to decode.
    Sub,
}

impl Stream {
    /// Dahua's number for it: 0 or 1.
    pub const fn index(self) -> u8 {
        match self {
            Self::Main => 0,
            Self::Sub => 1,
        }
    }

    /// The stream Dahua numbers `index`, if there is one.
    pub const fn from_index(index: u64) -> Option<Self> {
        match index {
            0 => Some(Self::Main),
            1 => Some(Self::Sub),
            _ => None,
        }
    }

    /// `main` or `sub`.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Sub => "sub",
        }
    }
}

impl Serialize for Stream {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.index())
    }
}

impl<'de> Deserialize<'de> for Stream {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let index = u64::deserialize(deserializer)?;
        Ok(Self::from_index(index).unwrap_or_default())
    }
}

/// Why a camera can't be saved or played.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidCamera {
    /// The camera has no name.
    NoName,
    /// The camera has no address.
    NoAddress,
    /// The camera has no username.
    NoUser,
    /// A port is 0.
    BadPort,
    /// The channel is 0.
    BadChannel,
    /// A Reolink's FLV stream, on a camera of another brand.
    FlvNotReolink,
}

impl InvalidCamera {
    /// A sentence that says what to enter.
    pub const fn message(self) -> &'static str {
        match self {
            Self::NoName => "Give the camera a name.",
            Self::NoAddress => "Enter the camera's address.",
            Self::NoUser => "Enter a username.",
            Self::BadPort => "Use a port from 1 to 65535.",
            Self::BadChannel => "Use a channel of 1 or more.",
            Self::FlvNotReolink => {
                "Only a Reolink sends an flv: stream. Set the brand to Reolink, or pick another                  stream."
            }
        }
    }
}

impl fmt::Display for InvalidCamera {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for InvalidCamera {}

impl CameraConfig {
    /// Checks that this camera can be saved, returning the first problem found.
    pub fn validate(&self) -> Result<(), InvalidCamera> {
        if self.name.trim().is_empty() {
            return Err(InvalidCamera::NoName);
        }
        self.validate_stream()?;
        if [self.onvif_port, self.dvrip_port, self.api_port].contains(&0) {
            return Err(InvalidCamera::BadPort);
        }
        Ok(())
    }

    /// Checks only what connecting to the stream needs: a hand-edited camera may lack the rest.
    pub fn validate_stream(&self) -> Result<(), InvalidCamera> {
        let flv = matches!(self.stream_path(), StreamPath::ReolinkFlv(_));
        if self.host.trim().is_empty() {
            Err(InvalidCamera::NoAddress)
        } else if self.user.trim().is_empty() {
            Err(InvalidCamera::NoUser)
        } else if self.rtsp_port == 0 || (flv && self.api_port == 0) {
            Err(InvalidCamera::BadPort)
        } else if self.channel < 1 {
            Err(InvalidCamera::BadChannel)
        } else if flv && self.brand != Brand::Reolink {
            Err(InvalidCamera::FlvNotReolink)
        } else {
            Ok(())
        }
    }

    /// An RTSP path, or one of a Reolink's FLV streams.
    pub fn stream_path(&self) -> StreamPath<'_> {
        StreamPath::of(&self.rtsp_path_override)
    }

    /// Where this camera's stream is, with no login in it, for telling two streams apart.
    pub fn stream_address(&self) -> String {
        match self.stream_path() {
            StreamPath::Rtsp(_) => crate::rtsp_url::build(self),
            StreamPath::ReolinkFlv(name) => {
                format!(
                    "{FLV_PREFIX}//{}:{}/{name}",
                    self.host.trim(),
                    self.api_port
                )
            }
        }
    }

    /// Overrides the address and login with `IPCAMERA_HOST`, `_USER` and `_PASSWORD` when set.
    pub fn apply_env(&mut self) {
        if let Some(host) = non_empty_env("IPCAMERA_HOST") {
            self.host = host;
        }
        if let Some(user) = non_empty_env("IPCAMERA_USER") {
            self.user = user;
        }
        if let Some(password) = non_empty_env("IPCAMERA_PASSWORD") {
            self.password = password;
        }
    }

    /// Who it logs in as and where: `admin@192.0.2.10:554 · ch1 main`, or the overriding path.
    pub fn stream_label(&self) -> String {
        let stream = match self.rtsp_path_override.trim() {
            "" => format!("ch{} {}", self.channel, self.stream.label()),
            path => path.to_string(),
        };
        let port = match self.stream_path() {
            StreamPath::Rtsp(_) => self.rtsp_port,
            StreamPath::ReolinkFlv(_) => self.api_port,
        };
        format!("{}@{}:{port}  ·  {stream}", self.user, self.host)
    }

    /// These settings with `stream` in place of the stream they name; `None` changes nothing.
    pub fn with_stream(&self, stream: Option<&TileStream>) -> CameraConfig {
        let mut camera = self.clone();
        match stream {
            None | Some(TileStream::Own) => {}
            Some(TileStream::Brand(stream)) => {
                camera.stream = *stream;
                // The brand's path, not the camera's own override.
                camera.rtsp_path_override.clear();
            }
            Some(TileStream::Path(path)) => camera.rtsp_path_override = path.clone(),
        }
        camera
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_usual_ports() {
        let camera = CameraConfig::default();
        assert_eq!(camera.rtsp_port, 554);
        assert_eq!(camera.channel, 1);
        assert_eq!(camera.stream, Stream::Main);
        assert!(camera.use_tcp);
        assert!(camera.auto_reconnect);
        assert_eq!(camera.onvif_port, 80);
        assert_eq!(camera.onvif_device_path, "/onvif/device_service");
        assert_eq!(camera.dvrip_port, 37777);
        assert_eq!((camera.api_port, camera.api_https), (80, false));
        assert!(
            !camera.http_api,
            "PTZ over DVRIP, as before there was a choice"
        );
        assert_eq!(camera.brand, Brand::Dahua);
        assert!(
            camera.log_events,
            "every camera's events are logged until it is told not to"
        );
        let quiet: CameraConfig = serde_json::from_str(r#"{ "log_events": false }"#).unwrap();
        assert!(!quiet.log_events);
        let reolink = CameraConfig::for_brand(Brand::Reolink);
        assert!(reolink.log_events);
        assert_eq!((reolink.api_port, reolink.api_https), (443, true));
        assert_eq!(reolink.onvif_port, 8000);
        assert!(camera.id.is_empty(), "only the camera store hands out ids");
    }

    #[test]
    fn a_camera_saved_before_brands_is_dahuas_and_an_unknown_brand_is_other() {
        let old: CameraConfig = serde_json::from_str(r#"{ "name": "Porch" }"#).unwrap();
        assert_eq!(old.brand, Brand::Dahua);

        let reolink: CameraConfig = serde_json::from_str(r#"{ "brand": "reolink" }"#).unwrap();
        assert_eq!(reolink.brand, Brand::Reolink);
        let saved = serde_json::to_string(&reolink).unwrap();
        assert!(saved.contains(r#""brand":"reolink""#), "{saved}");

        let newer: CameraConfig = serde_json::from_str(r#"{ "brand": "hikvision" }"#).unwrap();
        assert_eq!(newer.brand, Brand::Other, "never the wrong protocol");
    }

    #[test]
    fn defaults_name_no_camera() {
        let camera = CameraConfig::default();
        assert!(camera.name.is_empty());
        assert!(camera.host.is_empty());
        assert!(camera.user.is_empty());
        assert!(camera.password.is_empty());
    }

    #[test]
    fn a_hand_written_entry_only_needs_what_differs() {
        let camera: CameraConfig =
            serde_json::from_str(r#"{ "name": "Porch", "host": "10.0.0.7" }"#).unwrap();

        assert_eq!(camera.name, "Porch");
        assert_eq!(camera.host, "10.0.0.7");
        assert_eq!(camera.rtsp_port, 554);
        assert_eq!(camera.dvrip_port, 37777);
        assert!(camera.id.is_empty());
    }

    #[test]
    fn a_camera_without_what_it_needs_says_why() {
        let porch = CameraConfig {
            name: "Porch".into(),
            host: "10.0.0.7".into(),
            user: "admin".into(),
            ..CameraConfig::default()
        };
        assert_eq!(porch.validate(), Ok(()), "a password is not needed to save");

        let cases = [
            (
                CameraConfig {
                    name: " ".into(),
                    ..porch.clone()
                },
                InvalidCamera::NoName,
            ),
            (
                CameraConfig {
                    host: String::new(),
                    ..porch.clone()
                },
                InvalidCamera::NoAddress,
            ),
            (
                CameraConfig {
                    user: String::new(),
                    ..porch.clone()
                },
                InvalidCamera::NoUser,
            ),
            (
                CameraConfig {
                    dvrip_port: 0,
                    ..porch.clone()
                },
                InvalidCamera::BadPort,
            ),
            (
                CameraConfig {
                    channel: 0,
                    ..porch.clone()
                },
                InvalidCamera::BadChannel,
            ),
        ];
        for (camera, problem) in cases {
            assert_eq!(camera.validate(), Err(problem), "{camera:?}");
        }
    }

    #[test]
    fn a_camera_with_no_name_can_still_play() {
        let unnamed = CameraConfig {
            host: "10.0.0.7".into(),
            user: "admin".into(),
            onvif_port: 0,
            ..CameraConfig::default()
        };
        assert_eq!(unnamed.validate(), Err(InvalidCamera::NoName));
        assert_eq!(unnamed.validate_stream(), Ok(()));
    }

    #[test]
    fn a_stream_is_saved_as_dahuas_number_and_an_unknown_one_is_the_main_stream() {
        let sub: CameraConfig = serde_json::from_str(r#"{ "sub_stream": 1 }"#).unwrap();
        assert_eq!(sub.stream, Stream::Sub);
        let saved = serde_json::to_string(&sub).unwrap();
        assert!(saved.contains(r#""sub_stream":1"#), "{saved}");

        let odd: CameraConfig = serde_json::from_str(r#"{ "sub_stream": 7 }"#).unwrap();
        assert_eq!(odd.stream, Stream::Main);
    }

    #[test]
    fn the_password_never_shows_in_debug_output() {
        let camera = CameraConfig {
            password: "s3cret".into(),
            ..CameraConfig::default()
        };

        let shown = format!("{camera:?}");

        assert!(!shown.contains("s3cret"), "{shown}");
        assert!(shown.contains("<hidden>"), "{shown}");
    }

    #[test]
    fn a_tiles_stream_changes_the_stream_and_nothing_else() {
        let camera = CameraConfig {
            brand: Brand::Reolink,
            host: "192.0.2.11".into(),
            user: "admin".into(),
            password: "s3cret".into(),
            channel: 2,
            rtsp_path_override: "/h264Preview_02_main".into(),
            ..CameraConfig::default()
        };

        assert_eq!(camera.with_stream(None), camera);

        let sub = camera.with_stream(Some(&TileStream::Brand(Stream::Sub)));
        assert_eq!(
            crate::rtsp_url::build(&sub),
            "rtsp://192.0.2.11:554/Preview_02_sub",
            "the brand's path for the camera's channel, not the camera's own override"
        );

        let listed = camera.with_stream(Some(&TileStream::Path("/h264Preview_02_sub".into())));
        assert_eq!(
            crate::rtsp_url::build(&listed),
            "rtsp://192.0.2.11:554/h264Preview_02_sub"
        );
        assert_eq!(
            (listed.host, listed.user, listed.password, listed.id),
            (camera.host, camera.user, camera.password, camera.id)
        );
    }

    #[test]
    fn an_flv_path_is_a_reolinks_stream_on_its_api_port() {
        let mut camera = CameraConfig {
            name: "Door".into(),
            host: "192.0.2.12".into(),
            user: "admin".into(),
            rtsp_path_override: " flv:channel0_ext.bcs ".into(),
            ..CameraConfig::for_brand(Brand::Reolink)
        };
        assert_eq!(
            camera.stream_path(),
            StreamPath::ReolinkFlv("channel0_ext.bcs")
        );
        assert_eq!(camera.validate(), Ok(()));
        assert_eq!(
            camera.stream_label(),
            "admin@192.0.2.12:443  ·  flv:channel0_ext.bcs"
        );
        assert_eq!(
            camera.stream_address(),
            "flv://192.0.2.12:443/channel0_ext.bcs"
        );

        camera.brand = Brand::Dahua;
        assert_eq!(camera.validate(), Err(InvalidCamera::FlvNotReolink));

        camera.rtsp_path_override = "/Preview_01_main".into();
        assert_eq!(camera.stream_path(), StreamPath::Rtsp("/Preview_01_main"));
        assert_eq!(
            camera.stream_address(),
            "rtsp://192.0.2.12:554/Preview_01_main"
        );
    }

    #[test]
    fn a_tile_can_pick_an_flv_stream_the_camera_listed() {
        let camera = CameraConfig {
            host: "192.0.2.12".into(),
            rtsp_path_override: "/Preview_01_main".into(),
            ..CameraConfig::for_brand(Brand::Reolink)
        };
        let ext = camera.with_stream(Some(&TileStream::Path("flv:channel0_ext.bcs".into())));
        assert_eq!(
            ext.stream_path(),
            StreamPath::ReolinkFlv("channel0_ext.bcs")
        );
        assert_ne!(ext.stream_address(), camera.stream_address());
    }

    #[test]
    fn the_label_names_the_path_when_one_overrides_the_channel() {
        let mut camera = CameraConfig {
            host: "192.0.2.10".into(),
            user: "admin".into(),
            ..CameraConfig::default()
        };
        assert_eq!(camera.stream_label(), "admin@192.0.2.10:554  ·  ch1 main");
        camera.rtsp_path_override = "/live?stream=1".into();
        assert_eq!(
            camera.stream_label(),
            "admin@192.0.2.10:554  ·  /live?stream=1"
        );
    }
}
