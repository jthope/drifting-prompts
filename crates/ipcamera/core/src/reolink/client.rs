//! Sends [`super`]'s commands to a camera and keeps the login they need.
//!
//! The client logs in once and reuses the token until shortly before its lease runs out, then logs
//! out and in again: a camera allows only a few sessions at once. When the camera forgets a token
//! early, after a restart say, the client logs in again once and resends the request.
//!
//! A refused login is final. The client remembers it and answers every later call with it
//! without asking the camera again, since each failed login brings the account closer to being
//! locked. Only a new client, made after the user changed the password, logs in again.
//!
//! Calls block, for up to [`TIMEOUT`]: each camera's client belongs on a thread of its own, never on
//! the window's. HTTPS is the cameras' default, with a certificate each camera signs itself, which
//! no authority vouches for. The client accepts it without checking, as `reolink_aio` does.

use std::{
    io::Read,
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;
use ureq::{Agent, tls::TlsConfig};

use super::{
    FlvHead, ReolinkError, Replies, Result, ZoomPosition, flv_name, flv_names_to_ask, flv_request,
    get_ability, get_ai_state, get_dev_info, get_md_state, get_net_port, get_rtsp_url,
    get_white_led, get_zoom_focus, has_spotlight, login, logout, one_lens, ptz_ctrl, ptz_op,
    read_ai, read_device_info, read_flv_head, read_motion, read_ptz, read_replies, read_rtmp_port,
    read_spotlight, read_streams, read_token, read_zoom, reolink_channel, sends_flv, set_white_led,
    start_zoom,
};
use crate::{
    camera_features::{
        CameraActivity, CameraFeatures, CameraStream, DeviceInfo, FLV_PREFIX, Spotlight,
    },
    ptz::PtzVelocity,
};

/// How long a request may take before the camera counts as unreachable.
const TIMEOUT: Duration = Duration::from_secs(10);

/// How long Detect waits for an FLV stream's first bytes: one the camera has starts at once, and
/// one it hasn't is either refused or never answered.
const FLV_HEAD_WAIT: Duration = Duration::from_secs(3);

/// The most of an FLV stream Detect reads for its head, which is in the first few kilobytes.
const FLV_HEAD_BYTES: usize = 256 * 1024;

/// How long [`ReolinkClient::answers`] waits: it asks before anyone waits on a camera's answer,
/// and a camera that answers at all answers in well under this.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// How long before a token's lease runs out the client logs in again, as `reolink_aio` does.
const RENEW_BEFORE: Duration = Duration::from_secs(300);

/// A token and when to stop using it.
struct Token {
    name: String,
    renew_at: Instant,
}

/// A logged-in connection to a Reolink camera's HTTP API.
pub struct ReolinkClient {
    agent: Agent,
    /// The camera's web server: `https://192.0.2.10:443`.
    base: String,
    url: String,
    user: String,
    password: String,
    token: Option<Token>,
    /// Why the camera refused a login, which every later call returns without asking it again.
    refused: Option<ReolinkError>,
    /// Whether a PTZ move takes a speed, as [`features`](Self::features) found; until then it
    /// goes without one, which every camera takes.
    ptz_speed: bool,
}

impl ReolinkClient {
    /// Logs in to the API of the camera at `host` on `port`, over HTTPS when `https`, else plain
    /// HTTP. Makes exactly one login attempt.
    pub fn connect(host: &str, port: u16, https: bool, user: &str, password: &str) -> Result<Self> {
        let mut client = Self::new(host, port, https, user, password, TIMEOUT);
        client.login()?;
        Ok(client)
    }

    /// Whether a Reolink camera's API answers at `host` on `port`. It asks without logging in, and
    /// a Reolink camera answers "please login first", which no other camera says. Never a login,
    /// so never a failed one. Waits up to [`PROBE_TIMEOUT`].
    pub fn answers(host: &str, port: u16, https: bool) -> bool {
        let probe = Self::new(host, port, https, "", "", PROBE_TIMEOUT);
        matches!(
            probe.send(&[get_dev_info()], None),
            Err(ReolinkError::SessionExpired)
        )
    }

    /// A client that hasn't logged in.
    fn new(
        host: &str,
        port: u16,
        https: bool,
        user: &str,
        password: &str,
        timeout: Duration,
    ) -> Self {
        let scheme = if https { "https" } else { "http" };
        // An IPv6 address goes in brackets in a URL.
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        let base = format!("{scheme}://{host}:{port}");
        Self {
            agent: agent(timeout),
            url: format!("{base}/cgi-bin/api.cgi"),
            base,
            user: user.to_string(),
            password: password.to_string(),
            token: None,
            refused: None,
            ptz_speed: false,
        }
    }

    /// What the camera says it is.
    pub fn device_info(&mut self) -> Result<DeviceInfo> {
        let value = self.call_one(get_dev_info())?;
        Ok(read_device_info(&value))
    }

    /// What the camera can do on `channel`, counted from 1.
    pub fn features(&mut self, channel: u32) -> Result<CameraFeatures> {
        let channel = reolink_channel(channel);
        let [ability, motion, ai] = self.call_three([
            get_ability(&self.user),
            get_md_state(channel),
            get_ai_state(channel),
        ])?;
        let ability = ability?;
        let ptz = read_ptz(&ability, channel);
        self.ptz_speed = ptz.speed;
        Ok(CameraFeatures {
            motion: motion.is_ok(),
            // A camera with no AI refuses the command.
            detections: ai.map(|value| read_ai(&value).0).unwrap_or_default(),
            spotlight: has_spotlight(&ability, channel),
            pan_tilt: ptz.pan_tilt,
            zoom: ptz.zoom,
        })
    }

    /// What the camera sees on `channel`, counted from 1, right now, with its spotlight when
    /// `spotlight` (a camera without one may refuse to say).
    pub fn activity(&mut self, channel: u32, spotlight: bool) -> Result<CameraActivity> {
        let channel = reolink_channel(channel);
        let mut commands = vec![get_md_state(channel), get_ai_state(channel)];
        if spotlight {
            commands.push(get_white_led(channel));
        }
        let mut replies = self.call(commands)?.into_iter();
        let motion = replies.next().ok_or_else(missing_reply)??;
        let ai = replies.next().ok_or_else(missing_reply)?;
        let light = replies.next().and_then(|reply| reply.ok());
        Ok(CameraActivity {
            motion: read_motion(&motion),
            detected: ai.map(|value| read_ai(&value).1).unwrap_or_default(),
            spotlight: light.map(|value| read_spotlight(&value)),
        })
    }

    /// Starts moving the camera on `channel`, counted from 1, at `velocity`, or stops it when the
    /// velocity is still. A move lasts until a stop.
    pub fn ptz_move(&mut self, channel: u32, velocity: PtzVelocity) -> Result<()> {
        let channel = reolink_channel(channel);
        let Some((op, speed)) = ptz_op(velocity) else {
            return self.call_one(ptz_ctrl(channel, "Stop", None)).map(drop);
        };
        let with_speed = self.ptz_speed;
        match self.call_one(ptz_ctrl(channel, op, with_speed.then_some(speed))) {
            // What the camera says of itself doesn't always settle whether a move takes a speed,
            // a zoom's least of all: one refused is tried once the other way, and what the camera
            // took is how the next goes.
            Err(ReolinkError::Refused { .. }) => {
                self.call_one(ptz_ctrl(channel, op, (!with_speed).then_some(speed)))?;
                self.ptz_speed = !with_speed;
                Ok(())
            }
            sent => sent.map(drop),
        }
    }

    /// The streams of `channel`, counted from 1, as the camera lists their RTSP addresses; none
    /// from a camera without the command.
    pub fn streams(&mut self, channel: u32) -> Result<Vec<CameraStream>> {
        match self.call_one(get_rtsp_url(reolink_channel(channel))) {
            Ok(value) => Ok(read_streams(&value)),
            Err(ReolinkError::Refused { .. } | ReolinkError::Protocol(_)) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    /// The FLV streams of `channel`, counted from 1, that `listed`, the streams the camera listed
    /// over RTSP, doesn't have: each name Reolink's web server sends a stream under
    /// ([`super::FLV_STREAMS`]) opened for its first bytes, all at once, and kept where it sends
    /// H.264, under that name (`ext`), its path `flv:` and the name in the address
    /// (`flv:channel0_ext.bcs`), with what its head says of its picture. A second lens is listed
    /// under the first of its names that answers, though some cameras answer to two. None from a
    /// camera whose abilities don't include FLV.
    pub fn flv_streams(
        &mut self,
        channel: u32,
        listed: &[CameraStream],
    ) -> Result<Vec<CameraStream>> {
        let channel = reolink_channel(channel);
        let mut replies = self
            .call(vec![get_ability(&self.user), get_net_port()])?
            .into_iter();
        let ability = replies.next().ok_or_else(missing_reply)??;
        if !sends_flv(&ability, channel) {
            return Ok(Vec::new());
        }
        let rtmp_port = replies
            .next()
            .and_then(|reply| reply.ok())
            .map_or(super::RTMP_PORT, |net_port| read_rtmp_port(&net_port));
        let token = self.token()?;

        let wanted = flv_names_to_ask(listed);
        let this = &*self;
        let heads: Vec<Option<FlvHead>> = thread::scope(|scope| {
            let asked: Vec<_> = wanted
                .iter()
                .map(|stream| {
                    let request = flv_request(rtmp_port, &flv_name(channel, stream), &token);
                    scope.spawn(move || this.flv_head(&request))
                })
                .collect();
            asked
                .into_iter()
                .map(|head| head.join().unwrap_or(None))
                .collect()
        });
        let answered: Vec<(&str, FlvHead)> = wanted
            .into_iter()
            .zip(heads)
            .filter_map(|(stream, head)| Some((stream, head.filter(FlvHead::h264)?)))
            .collect();
        let kept = one_lens(answered.iter().map(|(stream, _)| *stream).collect());
        Ok(answered
            .into_iter()
            .filter(|(stream, _)| kept.contains(stream))
            .map(|(stream, head)| CameraStream {
                name: stream.to_string(),
                path: format!("{FLV_PREFIX}{}", flv_name(channel, stream)),
                detail: head.detail(),
            })
            .collect())
    }

    /// The address of the FLV stream `name` (`channel0_ext.bcs`), with this login's token in it
    /// rather than the password, for the media code to open. It is a login all the same: never
    /// print or log it. The camera takes the token only while this client is logged in, and goes
    /// on sending after it logs out, so open the stream before dropping the client.
    pub fn flv_url(&mut self, name: &str) -> Result<String> {
        let rtmp_port = match self.call_one(get_net_port()) {
            Ok(net_port) => read_rtmp_port(&net_port),
            Err(ReolinkError::Refused { .. } | ReolinkError::Protocol(_)) => super::RTMP_PORT,
            Err(error) => return Err(error),
        };
        let token = self.token()?;
        Ok(format!(
            "{}{}",
            self.base,
            flv_request(rtmp_port, name, &token)
        ))
    }

    /// What the FLV stream at `request` says of itself in its first bytes; `None` when the
    /// camera refuses it, hangs up, sends something else or says nothing within
    /// [`FLV_HEAD_WAIT`]. Reading stops there, which closes the stream.
    fn flv_head(&self, request: &str) -> Option<FlvHead> {
        let mut response = agent(FLV_HEAD_WAIT)
            .get(format!("{}{request}", self.base))
            .call()
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let mut body = response.body_mut().as_reader();
        let mut bytes = Vec::new();
        let mut chunk = [0; 16 * 1024];
        while bytes.len() < FLV_HEAD_BYTES {
            // The wait running out ends the read like the camera hanging up does.
            let read = body.read(&mut chunk).unwrap_or(0);
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            if let Some(head) = read_flv_head(&bytes) {
                return Some(head);
            }
        }
        read_flv_head(&bytes)
    }

    /// Where the zoom of `channel`, counted from 1, is and how far it goes; `None` for a camera
    /// that has no zoom position, or doesn't say its range.
    pub fn zoom_position(&mut self, channel: u32) -> Result<Option<ZoomPosition>> {
        match self.call_one(get_zoom_focus(reolink_channel(channel))) {
            Ok(value) => Ok(read_zoom(&value)),
            // A camera without the command has no zoom position.
            Err(ReolinkError::Refused { .. } | ReolinkError::Protocol(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Zooms `channel`, counted from 1, to `position`, in the camera's own numbers
    /// ([`zoom_position`](Self::zoom_position)). The lens travels there by itself.
    pub fn zoom_to(&mut self, channel: u32, position: i64) -> Result<()> {
        self.call_one(start_zoom(reolink_channel(channel), position))
            .map(drop)
    }

    /// Stops the camera on `channel`, counted from 1.
    pub fn ptz_stop(&mut self, channel: u32) -> Result<()> {
        self.call_one(ptz_ctrl(reolink_channel(channel), "Stop", None))
            .map(drop)
    }

    /// The spotlight on `channel`, counted from 1.
    pub fn spotlight(&mut self, channel: u32) -> Result<Spotlight> {
        let value = self.call_one(get_white_led(reolink_channel(channel)))?;
        Ok(read_spotlight(&value))
    }

    /// Switches the spotlight on `channel`, counted from 1, on or off at `spotlight`'s brightness.
    pub fn set_spotlight(&mut self, channel: u32, spotlight: Spotlight) -> Result<()> {
        self.call_one(set_white_led(reolink_channel(channel), spotlight))
            .map(drop)
    }

    /// Sends one command, and returns its reply.
    fn call_one(&mut self, command: Value) -> Result<Value> {
        self.call(vec![command])?.pop().ok_or_else(missing_reply)?
    }

    /// Sends three commands in one request, and returns their replies.
    fn call_three(&mut self, commands: [Value; 3]) -> Result<[Result<Value>; 3]> {
        self.call(commands.into())?
            .try_into()
            .map_err(|_: Replies| missing_reply())
    }

    /// The camera's own picture of what it sees now, a JPEG at its main stream's size: `Snap`,
    /// which answers with the picture itself rather than JSON. Logs in again once when the camera
    /// has forgotten the token, as every call does.
    pub fn snapshot(&mut self, channel: u32) -> Result<Vec<u8>> {
        let token = self.token()?;
        match self.snap(channel, &token) {
            Err(ReolinkError::SessionExpired) => {
                self.token = None;
                let token = self.token()?;
                self.snap(channel, &token)
            }
            picture => picture,
        }
    }

    /// One `Snap` request with `token`.
    fn snap(&self, channel: u32, token: &str) -> Result<Vec<u8>> {
        let asked = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        let mut response = self
            .agent
            .get(&self.url)
            .query("cmd", "Snap")
            .query("channel", channel.to_string())
            // A different value each time, as Reolink's guide has it, so nothing on the way
            // answers with a picture it kept.
            .query("rs", asked.to_string())
            .query("token", token)
            .call()
            .map_err(cannot_reach)?;
        let status = response.status().as_u16();
        let picture = response
            .headers()
            .get("content-type")
            .and_then(|kind| kind.to_str().ok())
            .is_some_and(|kind| kind.starts_with("image/"));
        if (200..300).contains(&status) && picture {
            return response.body_mut().read_to_vec().map_err(cannot_reach);
        }
        // Anything else is the API's error, in JSON or a short page.
        let text = response.body_mut().read_to_string().map_err(cannot_reach)?;
        Err(match read_replies(&text, 1) {
            Ok(mut replies) => replies
                .pop()
                .and_then(Result::err)
                .unwrap_or_else(|| ReolinkError::Protocol("The camera sent no picture.".into())),
            Err(error) => error,
        })
    }

    /// Sends `commands` in one request with the token, logging in first when there is none or it
    /// is about to run out, and once more when the camera has forgotten it.
    ///
    /// Some firmware answers several commands in one request with a single reply, when one of
    /// them is a command the model doesn't have. Those commands are then sent again one request
    /// each, so the ones the camera does have are still answered.
    fn call(&mut self, commands: Vec<Value>) -> Result<Replies> {
        let token = self.token()?;
        let replies = match self.send(&commands, Some(&token)) {
            Err(ReolinkError::SessionExpired) => {
                // The camera dropped the session: it is gone, so there is nothing to log out of.
                self.token = None;
                let token = self.token()?;
                self.send(&commands, Some(&token))
            }
            replies => replies,
        };
        match replies {
            Err(ReolinkError::Protocol(_)) if commands.len() > 1 => self.one_by_one(&commands),
            replies => replies,
        }
    }

    /// Sends each of `commands` in a request of its own. One the camera can't make sense of is
    /// that command's failure; anything about the session or the connection fails them all.
    fn one_by_one(&mut self, commands: &[Value]) -> Result<Replies> {
        let token = self.token()?;
        let mut replies = Replies::with_capacity(commands.len());
        for command in commands {
            let reply = match self.send(std::slice::from_ref(command), Some(&token)) {
                Ok(mut reply) => reply.pop().ok_or_else(missing_reply)?,
                Err(error @ ReolinkError::Protocol(_)) => Err(error),
                Err(error) => return Err(error),
            };
            replies.push(reply);
        }
        Ok(replies)
    }

    /// The token to send, after logging in again if it is about to run out.
    fn token(&mut self) -> Result<String> {
        if let Some(refused) = &self.refused {
            return Err(refused.clone());
        }
        if let Some(token) = &self.token {
            if Instant::now() < token.renew_at {
                return Ok(token.name.clone());
            }
            self.logout();
        }
        self.login()?;
        self.token
            .as_ref()
            .map(|token| token.name.clone())
            .ok_or_else(missing_reply)
    }

    /// Logs in once. A refusal is remembered, so the camera is never asked again.
    fn login(&mut self) -> Result<()> {
        let attempt = self
            .send(&[login(&self.user, &self.password)], None)
            .and_then(|mut replies| replies.pop().ok_or_else(missing_reply)?)
            .and_then(|value| read_token(&value));
        match attempt {
            Ok((name, lease)) => {
                // Half the lease at least, so a short one doesn't mean a login on every call.
                let renew_after = lease.saturating_sub(RENEW_BEFORE).max(lease / 2);
                self.token = Some(Token {
                    name,
                    renew_at: Instant::now() + renew_after,
                });
                Ok(())
            }
            Err(error) => {
                if matches!(error, ReolinkError::LoginRejected(_) | ReolinkError::Locked) {
                    self.refused = Some(error.clone());
                }
                Err(error)
            }
        }
    }

    /// Ends the session, if there is one. The camera ends it by itself when its lease runs out,
    /// so a failure here changes nothing.
    fn logout(&mut self) {
        if let Some(token) = self.token.take() {
            let _ = self.send(&[logout()], Some(&token.name));
        }
    }

    /// One request: `commands` as a JSON array, with the token in the query string.
    fn send(&self, commands: &[Value], token: Option<&str>) -> Result<Replies> {
        let mut request = self.agent.post(&self.url);
        // Naming a lone command in the query too is what the guide's examples do.
        if let [only] = commands
            && let Some(name) = only["cmd"].as_str()
        {
            request = request.query("cmd", name);
        }
        if let Some(token) = token {
            request = request.query("token", token);
        }
        let body = serde_json::to_string(commands)
            .map_err(|error| ReolinkError::Protocol(error.to_string()))?;

        let mut response = request
            .header("Content-Type", "application/json")
            .send(&body)
            .map_err(cannot_reach)?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().map_err(cannot_reach)?;

        if !(200..300).contains(&status) {
            return Err(super::session_problem(&text).unwrap_or_else(|| {
                ReolinkError::Protocol(format!(
                    "The camera's API answered with HTTP {status}. Check the API port and whether \
                     it is HTTPS."
                ))
            }));
        }
        read_replies(&text, commands.len())
    }
}

/// Ends the session, so the camera's few sessions don't run out while each waits for its lease.
/// It can wait for up to [`TIMEOUT`], so drop a client on its own thread.
impl Drop for ReolinkClient {
    fn drop(&mut self) {
        self.logout();
    }
}

/// An agent for the camera's web server that gives up after `timeout`, and accepts the certificate
/// the camera signs itself, which no authority vouches for.
fn agent(timeout: Duration) -> Agent {
    let config = Agent::config_builder()
        .tls_config(TlsConfig::builder().disable_verification(true).build())
        .timeout_global(Some(timeout))
        // The camera's error pages say why, which a status code alone doesn't.
        .http_status_as_error(false)
        .build();
    Agent::new_with_config(config)
}

/// A connection that failed, in ureq's words.
fn cannot_reach(error: ureq::Error) -> ReolinkError {
    ReolinkError::Unreachable(error.to_string())
}

/// A reply the camera should have sent and didn't.
fn missing_reply() -> ReolinkError {
    ReolinkError::Protocol("The camera's answer is missing a reply.".into())
}
