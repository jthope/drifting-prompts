//! Reolink's HTTP API, a JSON array of commands to `/cgi-bin/api.cgi`, as data; [`ReolinkClient`]
//! sends it. The password goes only in `Login`'s body, never in a URL, an FLV stream's included.
//! Reolink counts channels from 0, a camera's settings from 1.

mod client;

pub use client::ReolinkClient;

use std::{fmt, time::Duration};

use serde_json::{Value, json};

use crate::{
    camera_features::{CameraStream, Detection, DeviceInfo, Spotlight, rtsp_path},
    ptz::PtzVelocity,
};

/// Why a call to a Reolink camera failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReolinkError {
    /// The camera refused the login. Retrying could lock the account, so nothing does.
    LoginRejected(String),
    /// The camera has locked the account after too many failed logins.
    Locked,
    /// The camera no longer knows the token, even after one fresh login.
    SessionExpired,
    /// The camera answered a command with an error.
    Refused {
        /// The command, as Reolink names it: `SetWhiteLed`.
        command: String,
        /// The camera's reason.
        detail: String,
    },
    /// The camera could not be reached, or the connection broke.
    Unreachable(String),
    /// The camera answered with something that isn't the API.
    Protocol(String),
}

impl fmt::Display for ReolinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoginRejected(reason) => write!(
                f,
                "The camera refused the login ({reason}). Check the username and password."
            ),
            Self::Locked => f.write_str(
                "The camera has locked this account after too many failed logins. Wait for it to \
                 unlock, then check the password.",
            ),
            Self::SessionExpired => {
                f.write_str("The camera ended the session and didn't accept a new one.")
            }
            Self::Refused { command, detail } => {
                write!(f, "The camera refused {command}: {detail}.")
            }
            Self::Unreachable(reason) => write!(
                f,
                "The camera's API can't be reached ({reason}). Check the address and the API port."
            ),
            Self::Protocol(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for ReolinkError {}

type Result<T> = std::result::Result<T, ReolinkError>;

type Replies = Vec<Result<Value>>;

const AI_KEYS: [(&str, Detection); 5] = [
    ("people", Detection::Person),
    ("vehicle", Detection::Vehicle),
    ("dog_cat", Detection::Animal),
    ("face", Detection::Face),
    ("package", Detection::Package),
];

fn command(name: &str, param: Value) -> Value {
    json!({ "cmd": name, "action": 0, "param": param })
}

/// Carries the password: never print or log it.
fn login(user: &str, password: &str) -> Value {
    command(
        "Login",
        json!({ "User": { "userName": user, "password": password } }),
    )
}

/// A camera allows only a few sessions at once.
fn logout() -> Value {
    command("Logout", json!({}))
}

fn get_dev_info() -> Value {
    command("GetDevInfo", json!({}))
}

fn get_ability(user: &str) -> Value {
    command("GetAbility", json!({ "User": { "userName": user } }))
}

fn get_md_state(channel: u32) -> Value {
    command("GetMdState", json!({ "channel": channel }))
}

fn get_ai_state(channel: u32) -> Value {
    command("GetAiState", json!({ "channel": channel }))
}

/// `action` 1 asks for each setting's range too.
fn get_zoom_focus(channel: u32) -> Value {
    json!({ "cmd": "GetZoomFocus", "action": 1, "param": { "channel": channel } })
}

fn start_zoom(channel: u32, position: i64) -> Value {
    command(
        "StartZoomFocus",
        json!({ "ZoomFocus": { "channel": channel, "op": "ZoomPos", "pos": position } }),
    )
}

fn get_rtsp_url(channel: u32) -> Value {
    command("GetRtspUrl", json!({ "channel": channel }))
}

fn read_streams(value: &Value) -> Vec<CameraStream> {
    fn gather(name: &str, value: &Value, streams: &mut Vec<CameraStream>) {
        match value {
            Value::String(address) if address.trim_start().starts_with("rtsp://") => {
                if let Some(path) = rtsp_path(address) {
                    let stream = CameraStream {
                        name: name.to_string(),
                        path,
                        detail: String::new(),
                    };
                    CameraStream::merge(streams, vec![stream]);
                }
            }
            Value::Object(fields) => {
                for (name, value) in fields {
                    gather(name, value, streams);
                }
            }
            Value::Array(items) => {
                for item in items {
                    gather(name, item, streams);
                }
            }
            _ => {}
        }
    }
    let mut streams = Vec::new();
    gather("", value, &mut streams);
    streams
}

/// The FLV streams Reolink's guides name: no reply lists which a camera has.
const FLV_STREAMS: [&str; 7] = [
    "main",
    "sub",
    "ext",
    "autotrack_main",
    "autotrack_sub",
    "telephoto_main",
    "telephoto_sub",
];

const RTMP_PORT: u16 = 1935;

const FLV_H264: u8 = 7;

const FLV_AAC: u8 = 10;

fn get_net_port() -> Value {
    command("GetNetPort", json!({}))
}

fn sends_flv(ability: &Value, channel: u32) -> bool {
    ability_version(ability, channel, "httpFlv") > 0
}

fn read_rtmp_port(value: &Value) -> u16 {
    value["NetPort"]["rtmpPort"]
        .as_u64()
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port > 0)
        .unwrap_or(RTMP_PORT)
}

fn flv_name(channel: u32, stream: &str) -> String {
    format!("channel{channel}_{stream}.bcs")
}

/// A token in the address, never the password; `name` comes from saved settings, so it is escaped.
fn flv_request(rtmp_port: u16, name: &str, token: &str) -> String {
    let name: String = name
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect();
    format!("/flv?port={rtmp_port}&app=bcs&stream={name}&token={token}")
}

/// `sub` in `/h264Preview_01_sub`, `autotrack` in `/Preview_01_autotrack`.
fn rtsp_stream_name(path: &str) -> Option<&str> {
    let path = path.split('?').next()?;
    let (_, after) = path.rsplit_once("Preview_")?;
    Some(after.split_once('_')?.1)
}

fn lens_of(stream: &str) -> Option<&str> {
    let lens = stream.split('_').next()?;
    (!matches!(lens, "main" | "sub" | "ext")).then_some(lens)
}

/// Whether RTSP lists FLV stream `stream` already; a lens's main stream drops the `_main` over
/// RTSP (`/Preview_01_autotrack`).
fn listed_over_rtsp(listed: &[CameraStream], stream: &str) -> bool {
    let twin = match stream.strip_suffix("_main") {
        Some(lens) if !lens.is_empty() => lens,
        _ => stream,
    };
    listed.iter().any(|known| {
        rtsp_stream_name(&known.path).is_some_and(|name| name.eq_ignore_ascii_case(twin))
    })
}

fn flv_names_to_ask(listed: &[CameraStream]) -> Vec<&'static str> {
    let lens = listed
        .iter()
        .filter_map(|known| rtsp_stream_name(&known.path))
        .find_map(lens_of);
    FLV_STREAMS
        .into_iter()
        .filter(|stream| !listed_over_rtsp(listed, stream))
        .filter(|stream| {
            lens.is_none_or(|lens| lens_of(stream).is_none_or(|of| of.eq_ignore_ascii_case(lens)))
        })
        .collect()
}

/// One second lens at most: some cameras answer to another lens's names for theirs too.
fn one_lens(answered: Vec<&'static str>) -> Vec<&'static str> {
    let lens = answered.iter().find_map(|stream| lens_of(stream));
    answered
        .into_iter()
        .filter(|stream| lens_of(stream).is_none_or(|of| Some(of) == lens))
        .collect()
}

/// What the start of an FLV stream says of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlvHead {
    /// The picture's width and height.
    pub size: Option<(u32, u32)>,
    /// FLV's number for its video: 7 for H.264, 12 for the H.265 some Reolinks send.
    pub video: Option<u8>,
    /// FLV's number for its sound: 10 for AAC.
    pub sound: Option<u8>,
}

impl FlvHead {
    /// Whether its video is H.264, the only video this app takes out of FLV.
    pub fn h264(&self) -> bool {
        self.video == Some(FLV_H264)
    }

    /// Whether its sound is AAC.
    pub fn aac(&self) -> bool {
        self.sound == Some(FLV_AAC)
    }

    /// `896×672, H264`.
    pub fn detail(&self) -> String {
        let codec = if self.h264() { "H264" } else { "not H.264" };
        match self.size {
            Some((width, height)) => format!("{width}×{height}, {codec}"),
            None => codec.to_string(),
        }
    }
}

/// What the start of an FLV stream says once it has said what its video is; `None` for bytes that
/// aren't FLV, or too few yet. The header's flags go unread: Reolink's cameras leave them unset.
pub fn read_flv_head(bytes: &[u8]) -> Option<FlvHead> {
    const SCRIPT: u8 = 18;
    const VIDEO: u8 = 9;
    const SOUND: u8 = 8;
    if bytes.len() < 9 || !bytes.starts_with(b"FLV") {
        return None;
    }
    let offset = u32::from_be_bytes(bytes[5..9].try_into().ok()?) as usize;
    // Past the header and the first tag's previous-tag size.
    let mut at = offset.checked_add(4)?;
    let mut head = FlvHead::default();
    let mut said = false;
    let mut tagged_video = false;
    while let Some(header) = bytes.get(at..at + 11) {
        let size =
            (usize::from(header[1]) << 16) | (usize::from(header[2]) << 8) | usize::from(header[3]);
        let Some(body) = bytes.get(at + 11..at + 11 + size) else {
            break;
        };
        match header[0] & 0x1f {
            SCRIPT if body.windows(10).any(|word| word == b"onMetaData") => {
                let number = |key: &str| amf_number(body, key);
                if head.size.is_none()
                    && let (Some(width), Some(height)) = (number("width"), number("height"))
                {
                    head.size = Some((width as u32, height as u32));
                }
                if head.video.is_none() {
                    head.video = number("videocodecid").map(|codec| codec as u8);
                }
                if head.sound.is_none() {
                    head.sound = number("audiocodecid").map(|codec| codec as u8);
                }
                said |= head.video.is_some();
            }
            VIDEO if !tagged_video && !body.is_empty() => {
                // Enhanced FLV marks its own tags with the top bit, which is never H.264 here.
                head.video = Some(body[0] & 0x8f);
                tagged_video = true;
                said = true;
            }
            SOUND if !body.is_empty() && head.sound.is_none() => head.sound = Some(body[0] >> 4),
            _ => {}
        }
        at += 11 + size + 4;
    }
    said.then_some(head)
}

fn amf_number(body: &[u8], key: &str) -> Option<f64> {
    let mut name = u16::try_from(key.len()).ok()?.to_be_bytes().to_vec();
    name.extend_from_slice(key.as_bytes());
    name.push(0);
    let at = body.windows(name.len()).position(|word| word == name)? + name.len();
    let number = f64::from_be_bytes(body.get(at..at + 8)?.try_into().ok()?);
    (number.is_finite() && number >= 0.0).then_some(number)
}

/// Where a camera's zoom is, and how far it goes, in the camera's own numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoomPosition {
    /// Where it is now.
    pub at: i64,
    /// Zoomed all the way out.
    pub min: i64,
    /// Zoomed all the way in.
    pub max: i64,
}

impl ZoomPosition {
    /// Where a step of `part` of the range ends, in or out by `velocity`'s sign, within the range.
    pub fn stepped(self, velocity: f64, part: f64) -> i64 {
        let stride = ((self.max - self.min) as f64 * part).round().max(1.0) as i64;
        let to = if velocity > 0.0 {
            self.at + stride
        } else {
            self.at - stride
        };
        to.clamp(self.min, self.max)
    }
}

fn read_zoom(value: &Value) -> Option<ZoomPosition> {
    let range = &value["range"]["ZoomFocus"]["zoom"]["pos"];
    let zoom = ZoomPosition {
        at: value["ZoomFocus"]["zoom"]["pos"].as_i64()?,
        min: range["min"].as_i64()?,
        max: range["max"].as_i64()?,
    };
    (zoom.max > zoom.min).then_some(zoom)
}

fn get_white_led(channel: u32) -> Value {
    command("GetWhiteLed", json!({ "channel": channel }))
}

/// No `action`, unlike the other commands.
fn set_white_led(channel: u32, spotlight: Spotlight) -> Value {
    json!({
        "cmd": "SetWhiteLed",
        "param": { "WhiteLed": {
            "channel": channel,
            "state": u8::from(spotlight.on),
            "bright": spotlight.brightness.min(100),
        } },
    })
}

fn ptz_ctrl(channel: u32, op: &str, speed: Option<u8>) -> Value {
    let mut param = json!({ "channel": channel, "op": op });
    if let Some(speed) = speed {
        param["speed"] = json!(speed);
    }
    command("PtzCtrl", param)
}

/// Reolink names directions as Dahua does; only zoom differs.
fn ptz_op(velocity: PtzVelocity) -> Option<(&'static str, u8)> {
    let (op, fastest) = match crate::ptz::direction_code(velocity)? {
        "ZoomTele" => ("ZoomInc", velocity.zoom.abs()),
        "ZoomWide" => ("ZoomDec", velocity.zoom.abs()),
        direction => (direction, velocity.pan.abs().max(velocity.tilt.abs())),
    };
    Some((op, ((fastest * 64.0).round() as u8).clamp(1, 64)))
}

const fn reolink_channel(channel: u32) -> u32 {
    channel.saturating_sub(1)
}

/// A login failure, a lock or an expired session fails the whole request, whichever command it
/// came with.
fn read_replies(text: &str, sent: usize) -> Result<Replies> {
    let Ok(Value::Array(replies)) = serde_json::from_str::<Value>(text) else {
        // Some firmware answers a login problem with a short HTML page rather than JSON.
        return Err(session_problem(text).unwrap_or_else(|| {
            ReolinkError::Protocol(
                "The camera answered with something that isn't Reolink's API. Check the API port."
                    .into(),
            )
        }));
    };
    if replies.len() != sent {
        // One reply for them all can be the session's: "please login first".
        if let Some(problem) = replies
            .iter()
            .filter_map(|reply| reply["error"]["detail"].as_str())
            .find_map(session_problem)
        {
            return Err(problem);
        }
        return Err(ReolinkError::Protocol(format!(
            "The camera answered {} of {sent} commands.",
            replies.len()
        )));
    }

    let replies: Replies = replies.into_iter().map(read_reply).collect();
    if let Some(problem) = replies.iter().find_map(|reply| match reply {
        Err(
            error @ (ReolinkError::LoginRejected(_)
            | ReolinkError::Locked
            | ReolinkError::SessionExpired),
        ) => Some(error.clone()),
        _ => None,
    }) {
        return Err(problem);
    }
    Ok(replies)
}

fn read_reply(mut reply: Value) -> Result<Value> {
    let command = reply["cmd"].as_str().unwrap_or("a command").to_string();
    if reply["code"].as_i64() != Some(0) {
        let detail = reply["error"]["detail"]
            .as_str()
            .unwrap_or("no reason given");
        return Err(
            session_problem(detail).unwrap_or_else(|| ReolinkError::Refused {
                command,
                detail: detail.to_string(),
            }),
        );
    }

    let mut value = reply.get_mut("value").map(Value::take).unwrap_or_default();
    // Asked with `action` 1, a command also says each setting's range, beside its value.
    if let (Some(range), Some(fields)) = (reply.get_mut("range"), value.as_object_mut()) {
        fields.insert("range".into(), range.take());
    }
    // A setting's reply carries a status of its own, 200 when it took.
    match value["rspCode"].as_i64() {
        Some(code) if code != 200 => Err(ReolinkError::Refused {
            command,
            detail: format!("status {code}"),
        }),
        _ => Ok(value),
    }
}

/// The cameras say a session problem in words, not codes.
fn session_problem(detail: &str) -> Option<ReolinkError> {
    let said = detail.to_ascii_lowercase();
    if said.contains("locked") {
        Some(ReolinkError::Locked)
    } else if said.contains("please login first") {
        Some(ReolinkError::SessionExpired)
    } else if [
        "invalid user",
        "login failed",
        "password wrong",
        "wrong password",
    ]
    .iter()
    .any(|phrase| said.contains(phrase))
    {
        Some(ReolinkError::LoginRejected(detail.trim().to_string()))
    } else {
        None
    }
}

fn read_token(value: &Value) -> Result<(String, Duration)> {
    let token = &value["Token"];
    let name = token["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| ReolinkError::Protocol("The camera's login answer has no token.".into()))?;
    let lease = token["leaseTime"]
        .as_f64()
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .unwrap_or(3600.0);
    Ok((name.to_string(), Duration::from_secs_f64(lease)))
}

fn read_device_info(value: &Value) -> DeviceInfo {
    let info = &value["DevInfo"];
    let text = |field: &str| info[field].as_str().unwrap_or_default().trim().to_string();
    DeviceInfo {
        model: text("model"),
        firmware: text("firmVer"),
        hardware: text("hardVer"),
    }
}

/// The channel's version of ability `name`, else the camera's as a whole.
fn ability(value: &Value, channel: u32, name: &str) -> Option<i64> {
    let ability = &value["Ability"];
    ability["abilityChn"]
        .get(channel as usize)
        .and_then(|chn| chn[name]["ver"].as_i64())
        .or_else(|| ability[name]["ver"].as_i64())
}

fn ability_version(value: &Value, channel: u32, name: &str) -> i64 {
    ability(value, channel, name).unwrap_or(0)
}

fn has_spotlight(value: &Value, channel: u32) -> bool {
    ability_version(value, channel, "floodLight") > 0
        || ability_version(value, channel, "supportFLswitch") > 0
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PtzAbility {
    pan_tilt: bool,
    zoom: bool,
    speed: bool,
}

/// `ptzType`: 0 none, 1 zoom, 2 and 5 pan, tilt and zoom, 3 and 6 pan and tilt, 7 pan. Only 2 and 3
/// take a speed, unless `supportPtzSpeed` is 0; `supportZoom` adds a zoom to any type.
fn read_ptz(value: &Value, channel: u32) -> PtzAbility {
    let kind = ability_version(value, channel, "ptzType");
    PtzAbility {
        pan_tilt: matches!(kind, 2 | 3 | 5 | 6 | 7),
        zoom: matches!(kind, 1 | 2 | 5) || ability_version(value, channel, "supportZoom") > 0,
        speed: matches!(kind, 2 | 3) && ability(value, channel, "supportPtzSpeed").unwrap_or(1) > 0,
    }
}

fn read_motion(value: &Value) -> bool {
    value["state"].as_i64() == Some(1)
}

/// The detections the camera has, and the ones it sees now.
fn read_ai(value: &Value) -> (Vec<Detection>, Vec<Detection>) {
    let mut supported = Vec::new();
    let mut detected = Vec::new();
    for (key, detection) in AI_KEYS {
        let (has, sees) = match &value[key] {
            Value::Object(state) => (
                state.get("support").and_then(Value::as_i64) == Some(1),
                state.get("alarm_state").and_then(Value::as_i64) == Some(1),
            ),
            // Older firmware gives each detection's state as a bare number.
            Value::Number(state) => (true, state.as_i64() == Some(1)),
            _ => (false, false),
        };
        if has {
            supported.push(detection);
            if sees {
                detected.push(detection);
            }
        }
    }
    (supported, detected)
}

fn read_spotlight(value: &Value) -> Spotlight {
    let led = &value["WhiteLed"];
    Spotlight {
        on: led["state"].as_i64() == Some(1),
        brightness: led["bright"]
            .as_u64()
            .map_or(100, |bright| bright.min(100) as u8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_goes_only_in_the_login_body() {
        let body = login("admin", "s3cret");
        assert_eq!(body["cmd"], "Login");
        assert_eq!(body["param"]["User"]["userName"], "admin");
        assert_eq!(body["param"]["User"]["password"], "s3cret");
    }

    #[test]
    fn channels_count_from_one_in_the_settings_and_from_zero_at_reolink() {
        assert_eq!(reolink_channel(1), 0);
        assert_eq!(reolink_channel(3), 2);
        assert_eq!(
            reolink_channel(0),
            0,
            "a channel of 0 is refused before here"
        );
        assert_eq!(get_md_state(reolink_channel(1))["param"]["channel"], 0);
    }

    #[test]
    fn the_spotlight_is_switched_with_its_brightness_and_no_action() {
        let on = set_white_led(
            0,
            Spotlight {
                on: true,
                brightness: 180,
            },
        );
        assert_eq!(
            on,
            json!({ "cmd": "SetWhiteLed", "param": { "WhiteLed": { "channel": 0, "state": 1, "bright": 100 } } })
        );
    }

    #[test]
    fn replies_come_back_in_order_and_one_can_fail_alone() {
        let text = r#"[
            { "cmd": "GetMdState", "code": 0, "value": { "state": 1 } },
            { "cmd": "GetAiState", "code": 1, "error": { "rspCode": -9, "detail": "not support" } }
        ]"#;
        let replies = read_replies(text, 2).unwrap();
        assert!(read_motion(replies[0].as_ref().unwrap()));
        assert_eq!(
            replies[1],
            Err(ReolinkError::Refused {
                command: "GetAiState".into(),
                detail: "not support".into()
            })
        );
    }

    #[test]
    fn a_session_problem_fails_the_whole_request() {
        let expired = r#"[
            { "cmd": "GetMdState", "code": 0, "value": { "state": 0 } },
            { "cmd": "GetAiState", "code": 1, "error": { "rspCode": -6, "detail": "please login first" } }
        ]"#;
        assert_eq!(read_replies(expired, 2), Err(ReolinkError::SessionExpired));

        let rejected = r#"[{ "cmd": "Login", "code": 1, "error": { "rspCode": -7, "detail": "login failed" } }]"#;
        assert_eq!(
            read_replies(rejected, 1),
            Err(ReolinkError::LoginRejected("login failed".into()))
        );

        let locked = "<html><body>Login has been locked</body></html>";
        assert_eq!(read_replies(locked, 1), Err(ReolinkError::Locked));
    }

    #[test]
    fn a_setting_that_did_not_take_is_refused() {
        let text = r#"[{ "cmd": "SetWhiteLed", "code": 0, "value": { "rspCode": -4 } }]"#;
        assert_eq!(
            read_replies(text, 1).unwrap()[0],
            Err(ReolinkError::Refused {
                command: "SetWhiteLed".into(),
                detail: "status -4".into()
            })
        );
    }

    #[test]
    fn something_that_isnt_the_api_says_to_check_the_port() {
        let error = read_replies("<html>Welcome</html>", 1).unwrap_err();
        assert!(matches!(error, ReolinkError::Protocol(_)), "{error:?}");
        assert!(error.to_string().contains("API port"), "{error}");

        assert!(matches!(
            read_replies("[]", 1),
            Err(ReolinkError::Protocol(_))
        ));
    }

    #[test]
    fn a_token_lasts_as_long_as_the_camera_says() {
        let (name, lease) =
            read_token(&json!({ "Token": { "name": "abc123", "leaseTime": 3600 } })).unwrap();
        assert_eq!(name, "abc123");
        assert_eq!(lease, Duration::from_secs(3600));

        assert!(read_token(&json!({ "Token": {} })).is_err());
    }

    #[test]
    fn the_device_says_its_model_and_firmware() {
        let info = read_device_info(&json!({ "DevInfo": {
            "model": "RLC-811A", "firmVer": "v3.1.0.2368_23062700", "hardVer": "IPC_523SD10", "channelNum": 1
        } }));
        assert_eq!(info.model, "RLC-811A");
        assert_eq!(info.firmware, "v3.1.0.2368_23062700");
        assert_eq!(info.hardware, "IPC_523SD10");
    }

    #[test]
    fn a_spotlight_is_one_the_channel_or_the_camera_has() {
        let channel = json!({ "Ability": { "abilityChn": [{ "floodLight": { "ver": 1 } }] } });
        assert!(has_spotlight(&channel, 0));
        assert!(!has_spotlight(&channel, 1));

        let switch = json!({ "Ability": { "abilityChn": [{ "supportFLswitch": { "ver": 1 } }] } });
        assert!(has_spotlight(&switch, 0));

        let none = json!({ "Ability": { "abilityChn": [{ "floodLight": { "ver": 0 } }] } });
        assert!(!has_spotlight(&none, 0));
    }

    #[test]
    fn the_ai_says_what_it_has_and_what_it_sees() {
        let value = json!({
            "channel": 0,
            "people": { "alarm_state": 1, "support": 1 },
            "vehicle": { "alarm_state": 0, "support": 1 },
            "dog_cat": { "alarm_state": 1, "support": 0 },
        });
        let (supported, detected) = read_ai(&value);
        assert_eq!(supported, [Detection::Person, Detection::Vehicle]);
        assert_eq!(
            detected,
            [Detection::Person],
            "an unsupported one never counts"
        );
    }

    #[test]
    fn older_firmware_gives_bare_numbers() {
        let (supported, detected) = read_ai(&json!({ "people": 0, "vehicle": 1 }));
        assert_eq!(supported, [Detection::Person, Detection::Vehicle]);
        assert_eq!(detected, [Detection::Vehicle]);
    }

    #[test]
    fn a_ptz_move_is_named_as_reolink_names_it_with_a_speed_to_64() {
        assert_eq!(ptz_op(PtzVelocity::new(1.0, 0.0, 0.0)), Some(("Right", 64)));
        assert_eq!(
            ptz_op(PtzVelocity::new(-0.5, 0.5, 0.0)),
            Some(("LeftUp", 32))
        );
        assert_eq!(ptz_op(PtzVelocity::new(0.0, -0.001, 0.0)), None);
        assert_eq!(
            ptz_op(PtzVelocity::new(0.0, 0.0, 0.25)),
            Some(("ZoomInc", 16))
        );
        assert_eq!(
            ptz_op(PtzVelocity::new(0.3, 0.0, -1.0)),
            Some(("ZoomDec", 64))
        );
        assert_eq!(
            ptz_op(PtzVelocity::new(0.011, 0.0, 0.0)),
            Some(("Right", 1))
        );

        assert_eq!(
            ptz_ctrl(0, "Left", Some(32)),
            json!({ "cmd": "PtzCtrl", "action": 0, "param": { "channel": 0, "op": "Left", "speed": 32 } })
        );
        assert_eq!(
            ptz_ctrl(0, "Stop", None)["param"],
            json!({ "channel": 0, "op": "Stop" })
        );
    }

    #[test]
    fn the_ptz_type_says_what_the_camera_moves() {
        let of = |kind: i64| {
            read_ptz(
                &json!({ "Ability": { "abilityChn": [{ "ptzType": { "ver": kind } }] } }),
                0,
            )
        };
        assert_eq!(of(0), PtzAbility::default());
        assert_eq!(
            of(1),
            PtzAbility {
                zoom: true,
                ..PtzAbility::default()
            }
        );
        assert_eq!(
            of(2),
            PtzAbility {
                pan_tilt: true,
                zoom: true,
                speed: true
            }
        );
        assert_eq!(
            of(6),
            PtzAbility {
                pan_tilt: true,
                ..PtzAbility::default()
            }
        );

        let no_speed = json!({ "Ability": { "abilityChn": [{
            "ptzType": { "ver": 3 }, "supportPtzSpeed": { "ver": 0 }
        }] } });
        assert!(!read_ptz(&no_speed, 0).speed);

        let zooms_too = json!({ "Ability": { "abilityChn": [{
            "ptzType": { "ver": 3 }, "supportZoom": { "ver": 1 }
        }] } });
        assert!(
            read_ptz(&zooms_too, 0).zoom,
            "a pan and tilt camera with a zoom lens"
        );
    }

    #[test]
    fn the_streams_are_the_addresses_the_camera_gave_under_its_own_names() {
        let streams = read_streams(&json!({ "rtspUrl": {
            "channel": 0,
            "mainStream": "rtsp://192.0.2.10:554/first?x=1",
            "subStream": "rtsp://192.0.2.10:554/second",
            "again": "rtsp://192.0.2.10:554/SECOND",
            "note": "not an address",
            "bare": "rtsp://192.0.2.10:554",
        } }));
        let listed: Vec<(&str, &str)> = streams
            .iter()
            .map(|stream| (stream.name.as_str(), stream.path.as_str()))
            .collect();
        assert_eq!(
            listed.len(),
            2,
            "each address once, and nothing that isn't one: {listed:?}"
        );
        assert!(listed.contains(&("mainStream", "/first?x=1")));
        assert!(read_streams(&json!({})).is_empty());
    }

    #[test]
    fn a_zoom_step_is_a_part_of_the_range_and_stops_at_its_ends() {
        let reply = json!({
            "ZoomFocus": { "channel": 0, "focus": { "pos": 51 }, "zoom": { "pos": 1300 } },
            "range": { "ZoomFocus": { "zoom": { "pos": { "max": 6000, "min": 1000 } } } },
        });
        let zoom = read_zoom(&reply).unwrap();
        assert_eq!(
            zoom,
            ZoomPosition {
                at: 1300,
                min: 1000,
                max: 6000
            }
        );
        assert_eq!(zoom.stepped(1.0, 0.05), 1550);
        assert_eq!(zoom.stepped(-1.0, 0.05), 1050);
        assert_eq!(zoom.stepped(-1.0, 0.5), 1000, "no further out than it goes");
        assert_eq!(zoom.stepped(1.0, 2.0), 6000);

        let no_range = json!({ "ZoomFocus": { "zoom": { "pos": 3 } } });
        assert_eq!(
            read_zoom(&no_range),
            None,
            "a camera that doesn't say how far"
        );
        assert_eq!(
            start_zoom(0, 1550)["param"],
            json!({ "ZoomFocus": { "channel": 0, "op": "ZoomPos", "pos": 1550 } })
        );
    }

    #[test]
    fn the_spotlight_reads_as_on_or_off_with_its_brightness() {
        let on = read_spotlight(
            &json!({ "WhiteLed": { "channel": 0, "state": 1, "bright": 80, "mode": 0 } }),
        );
        assert_eq!(
            on,
            Spotlight {
                on: true,
                brightness: 80
            }
        );
        let off = read_spotlight(&json!({ "WhiteLed": { "state": 0 } }));
        assert!(!off.on);
    }

    fn flv_start(width: f64, height: f64, codec: f64, video: &[u8]) -> Vec<u8> {
        fn tag(kind: u8, body: &[u8]) -> Vec<u8> {
            let size = body.len() as u32;
            let mut tag = vec![
                kind,
                (size >> 16) as u8,
                (size >> 8) as u8,
                size as u8,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ];
            tag.extend_from_slice(body);
            tag.extend_from_slice(&(size + 11).to_be_bytes());
            tag
        }
        let mut script = vec![2, 0, 10];
        script.extend_from_slice(b"onMetaData");
        script.extend_from_slice(&[8, 0, 0, 0, 4]);
        for (key, value) in [
            ("width", width),
            ("height", height),
            ("videocodecid", codec),
            ("audiocodecid", 10.0),
        ] {
            script.extend_from_slice(&(key.len() as u16).to_be_bytes());
            script.extend_from_slice(key.as_bytes());
            script.push(0);
            script.extend_from_slice(&value.to_be_bytes());
        }
        script.extend_from_slice(&[0, 0, 9]);
        let mut bytes = b"FLV\x01\x00\x00\x00\x00\x09\x00\x00\x00\x00".to_vec();
        bytes.extend(tag(18, &script));
        bytes.extend(tag(9, video));
        bytes
    }

    #[test]
    fn an_flv_streams_head_says_its_size_and_codecs() {
        let start = flv_start(896.0, 672.0, 7.0, &[0x17, 0, 0, 0, 0, 1, 100]);
        let head = read_flv_head(&start).expect("a head");
        assert_eq!(head.size, Some((896, 672)));
        assert!(head.h264() && head.aac());
        assert_eq!(head.detail(), "896×672, H264");

        assert!(read_flv_head(&start[..start.len() - 6]).is_some());
        assert_eq!(read_flv_head(&start[..20]), None);
        assert_eq!(read_flv_head(b"<html>please login first</html>"), None);

        let hevc = read_flv_head(&flv_start(3840.0, 2160.0, 12.0, &[0x1c, 0, 0, 0, 0])).unwrap();
        assert!(
            !hevc.h264(),
            "Reolink's H.265 in FLV, which the app doesn't take"
        );
        assert_eq!(hevc.detail(), "3840×2160, not H.264");
    }

    #[test]
    fn an_flv_request_carries_a_token_and_never_a_password() {
        assert_eq!(
            flv_request(1935, &flv_name(0, "ext"), "tok1"),
            "/flv?port=1935&app=bcs&stream=channel0_ext.bcs&token=tok1"
        );
        assert_eq!(
            flv_request(1935, "channel0_ext.bcs&user=x", "tok1"),
            "/flv?port=1935&app=bcs&stream=channel0_ext.bcs%26user%3Dx&token=tok1",
            "a hand-edited name can't add to the request"
        );
        assert_eq!(
            read_rtmp_port(&json!({ "NetPort": { "rtmpPort": 1936 } })),
            1936
        );
        assert_eq!(read_rtmp_port(&json!({})), RTMP_PORT);
        let ability =
            json!({ "Ability": { "httpFlv": { "permit": 6, "ver": 1 }, "abilityChn": [{}] } });
        assert!(sends_flv(&ability, 0));
        assert!(!sends_flv(&json!({ "Ability": {} }), 0));
    }

    #[test]
    fn an_flv_stream_its_rtsp_streams_have_already_isnt_offered_twice() {
        let listed = |paths: &[&str]| -> Vec<CameraStream> {
            paths
                .iter()
                .map(|path| CameraStream {
                    path: path.to_string(),
                    ..CameraStream::default()
                })
                .collect()
        };
        let reolink = listed(&[
            "/Preview_01_main",
            "/h264Preview_01_sub",
            "/Preview_01_autotrack",
        ]);
        assert_eq!(
            flv_names_to_ask(&reolink),
            ["ext", "autotrack_sub"],
            "RTSP names the tracking lens, so another lens's names aren't asked"
        );
        assert_eq!(
            flv_names_to_ask(&listed(&["/Preview_01_main", "/Preview_01_sub"])),
            [
                "ext",
                "autotrack_main",
                "autotrack_sub",
                "telephoto_main",
                "telephoto_sub"
            ]
        );
        assert!(!listed_over_rtsp(
            &listed(&["/cam/realmonitor?channel=1&subtype=0"]),
            "main"
        ));
    }

    #[test]
    fn a_second_lens_that_answers_to_two_names_is_listed_once() {
        assert_eq!(
            one_lens(vec![
                "ext",
                "autotrack_sub",
                "telephoto_main",
                "telephoto_sub"
            ]),
            ["ext", "autotrack_sub"]
        );
        assert_eq!(
            one_lens(vec!["telephoto_main", "telephoto_sub"]),
            ["telephoto_main", "telephoto_sub"]
        );
        assert_eq!(one_lens(vec!["main", "ext"]), ["main", "ext"]);
    }
}
