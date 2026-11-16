//! What the server and the web build say to each other beyond the library's own changes: its
//! status, the snapshots and recordings it saved, its folders, and the requests to take a
//! snapshot, record, move a camera, switch its spotlight and detect which camera is at an address;
//! and signing in to a server that has a password.
//!
//! Both ends read these types, as both read [`Change`](crate::change::Change), so the JSON can't
//! drift apart. In C# terms, they are the DTOs of the server's API. A URL in them is the server's
//! own path (`/files/snapshots/…`), which the web build puts after the server's address.
//!
//! What may be missing is an `Option`, as a nullable is in C#, and is left out of the JSON when it
//! is `None`: a thumbnail not made yet, a camera a file's name doesn't say.

use serde::{Deserialize, Serialize};

use crate::{
    camera_config::{Brand, CameraConfig},
    camera_features::{CameraFeatures, CameraStream, Detection, DeviceInfo},
    ids::CameraId,
};

/// `GET /api/status`: what the web build checks about once a second.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// The library's revision, which goes up with every change to the cameras, groups or views.
    pub revision: u64,
    /// A number that changes whenever a snapshot is added, deleted or rewritten; 0 with none.
    pub snapshots: u64,
    /// A number that changes whenever a recording or its thumbnail is added, deleted or written
    /// to; 0 with none.
    pub videos: u64,
    /// The cameras that have a stream, record, did, have a PTZ connection or are watched: one
    /// that does none of these is left out.
    pub cameras: Vec<CameraStatus>,
}

/// One camera's stream, recording and PTZ, as [`Status`] lists it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraStatus {
    /// Which camera.
    pub id: CameraId,
    /// Whether the server has a stream of it that hasn't given up: connecting, live or
    /// reconnecting.
    #[serde(default)]
    pub playing: bool,
    /// Why its stream gave up, until it is asked to connect again or its settings change; `None`
    /// when it hasn't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_problem: Option<String>,
    /// Whether it records, or waits for its stream to.
    pub recording: bool,
    /// Why its last recording stopped by itself, until the next one starts; `None` when none did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_problem: Option<String>,
    /// What PTZ last reported; `None` before its first move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ptz_status: Option<String>,
    /// What the server's watch of the camera says, while its stream runs; `None` for a camera it
    /// doesn't watch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch: Option<WatchStatus>,
}

/// What a camera's watch says now: what it sees and its spotlight (`CameraWatch` in `core`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchStatus {
    /// Whether something moves in its picture.
    pub motion: bool,
    /// What its AI sees now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detected: Vec<Detection>,
    /// Whether it has a spotlight the app can switch.
    pub spotlight: bool,
    /// Whether that spotlight is on.
    pub spotlight_on: bool,
    /// What the camera is and has, or why watching it failed, as a sentence.
    pub summary: String,
}

/// `POST /api/cameras/<id>/spotlight`: switches the camera's spotlight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotlightRequest {
    /// On, or off.
    pub on: bool,
}

/// `POST /api/detect`: which camera is at `setting`'s address.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DetectRequest {
    /// The saved camera the form edits, whose saved password the server logs in with when
    /// `setting` has none; `None` for a new camera.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraId>,
    /// The form's settings: the address, the login and the ports to ask on.
    pub setting: CameraConfig,
}

/// What `POST /api/detect` found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectReply {
    /// Who made the camera.
    pub brand: Brand,
    /// Where its HTTP API answered: a Reolink's, or a Dahua's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_port: Option<u16>,
    /// Whether that API is HTTPS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_https: Option<bool>,
    /// For a Dahua: whether its PTZ and light go over that API rather than PTZ over DVRIP.
    #[serde(default)]
    pub http_api: bool,
    /// What the camera says it is, when it was asked with a login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceInfo>,
    /// What it can do, when it was asked with a login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<CameraFeatures>,
    /// Whether it was asked with a password, which a Reolink needs to say more.
    pub asked_with_password: bool,
    /// Whether its ONVIF event service reports motion, or its PTZ service has a light, for a
    /// camera that isn't a Reolink.
    #[serde(default)]
    pub onvif_events: bool,
    /// The port ONVIF answered on, when it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onvif_port: Option<u16>,
    /// Why ONVIF told nothing of a camera found another way, as a sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onvif_problem: Option<String>,
    /// The streams the camera listed, when it was asked with a login.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub streams: Vec<CameraStream>,
    /// Whether its ONVIF refused the login, now or when it was asked before, where the login
    /// works another way: the form remembers it, and Detect leaves ONVIF alone from then on.
    #[serde(default)]
    pub onvif_refused: bool,
}

/// A saved snapshot, as `GET /api/snapshots` lists it, newest first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotEntry {
    /// The file's name, which every other request names it by.
    pub name: String,
    /// Where the server serves the picture.
    pub url: String,
    /// Where the server serves its thumbnail, which the gallery shows; `None` while it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    /// The camera that took it; `None` when its name doesn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraId>,
    /// What the camera saw as an event took it (`vehicle`, `person`), from its name; empty for
    /// one taken by hand.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// When it was taken, in milliseconds since 1970.
    pub taken_ms: u64,
    /// Its size on disk, in bytes.
    pub bytes: u64,
    /// Its size in pixels; `None` when its header can't be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<PixelSize>,
}

/// A picture's size, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelSize {
    /// Its width.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// A recording, as `GET /api/videos` lists it, newest first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoEntry {
    /// The file's name, which every other request names it by.
    pub name: String,
    /// Where the server serves the video, scrubbing included.
    pub url: String,
    /// Where the server serves its first picture; `None` while it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    /// The camera that made it; `None` when its name doesn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraId>,
    /// What the camera saw as an event's sighting started it, from its name; empty for one
    /// someone started.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// When it started, in milliseconds since 1970.
    pub started_ms: u64,
    /// How long it runs, in milliseconds: to now while it records, else to the file's last write.
    pub length_ms: u64,
    /// Its size on disk, in bytes.
    pub bytes: u64,
    /// Whether a camera is still writing it.
    pub recording: bool,
}

/// One of the server's folders, as `GET /api/storage` describes it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StorageEntry {
    /// Where it is on the server: an absolute path.
    pub path: String,
    /// Whether it is the default folder beside the server, not one its `settings.json` names.
    pub is_default: bool,
    /// The drive it is on, as Explorer names it (`D:`).
    pub drive: String,
    /// What is left on that drive, in bytes.
    pub free: u64,
    /// How much that drive holds, in bytes.
    pub total: u64,
    /// Why the drive's space couldn't be read, when it couldn't; then `free` and `total` are 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// `GET /api/storage`: the server's two folders.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Storage {
    /// Where snapshots go.
    pub snapshots: StorageEntry,
    /// Where recordings go.
    pub videos: StorageEntry,
}

/// `POST /api/snapshots`: which cameras to take a snapshot of.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotRequest {
    /// The cameras on screen; those that can't take one are left out, unless none can.
    pub cameras: Vec<CameraId>,
}

/// How many snapshots `POST /api/snapshots` saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotsTaken {
    /// How many.
    pub count: u32,
}

/// `POST /api/recording`: which cameras to record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecordRequest {
    /// The cameras on screen; those that can't record are left out, unless none can.
    pub cameras: Vec<CameraId>,
}

/// How many cameras started recording (`POST /api/recording`) or stopped (`DELETE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recorded {
    /// How many.
    pub count: u32,
}

/// `POST /api/cameras/<id>/ptz`: a move, which lasts only while the web build keeps sending it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PtzMove {
    /// -1 to 1: positive is right.
    pub pan: f64,
    /// -1 to 1: positive is up.
    pub tilt: f64,
    /// -1 to 1: positive is in.
    pub zoom: f64,
    /// 0 to 1.
    pub speed: f64,
}

/// `GET /api/session`: whether the server asks for a password, and whether this request came with
/// a session's key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Whether the server has a password, without which it answers nothing else.
    pub required: bool,
    /// Whether the request's key is a session's, or none is needed.
    pub signed_in: bool,
}

/// `POST /api/session`: signs in.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignIn {
    /// The server's password.
    pub password: String,
}

/// A session, as `POST /api/session` answers a right password.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedIn {
    /// The session's key: sent with every request after, as `Authorization: Bearer <key>`, or as
    /// `?key=<key>` where a browser makes the request itself (a player, a picture, a WebSocket).
    pub key: String,
}

/// `name` as one segment of a URL's path, as the server's URLs carry a file's name: every byte
/// but letters, digits and `-._~` escaped.
pub fn path_segment(name: &str) -> String {
    let mut segment = String::with_capacity(name.len());
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                segment.push(char::from(byte));
            }
            other => segment.push_str(&format!("%{other:02X}")),
        }
    }
    segment
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_goes_into_a_url_whole() {
        assert_eq!(path_segment("snapshot-5-a1.png"), "snapshot-5-a1.png");
        assert_eq!(path_segment("Holiday 1.PNG"), "Holiday%201.PNG");
        assert_eq!(path_segment("a/b"), "a%2Fb");
        assert_eq!(path_segment("é"), "%C3%A9");
    }

    #[test]
    fn a_status_reads_back_and_a_camera_needs_only_its_id_and_recording() {
        let status = Status {
            revision: 3,
            snapshots: 9,
            videos: 0,
            cameras: vec![CameraStatus {
                id: CameraId::from("porch"),
                playing: true,
                stream_problem: None,
                recording: true,
                record_problem: None,
                ptz_status: Some("PTZ: ready".into()),
                watch: Some(WatchStatus {
                    motion: true,
                    detected: vec![Detection::Person],
                    spotlight: true,
                    spotlight_on: false,
                    summary: "RLC-811A: Motion, Person, Spotlight.".into(),
                }),
            }],
        };
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(serde_json::from_str::<Status>(&json).unwrap(), status);

        let short: CameraStatus =
            serde_json::from_str(r#"{ "id": "yard", "recording": false }"#).unwrap();
        assert_eq!(short.ptz_status, None);
        assert_eq!(
            serde_json::to_string(&short).unwrap(),
            r#"{"id":"yard","playing":false,"recording":false}"#,
            "what is missing is left out"
        );
    }

    #[test]
    fn a_ptz_move_is_four_numbers() {
        let moved: PtzMove =
            serde_json::from_str(r#"{ "pan": 1, "tilt": 0, "zoom": -0.5, "speed": 0.5 }"#).unwrap();
        assert_eq!(moved.zoom, -0.5);
    }
}
