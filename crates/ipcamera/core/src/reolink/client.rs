//! Sends [`super`]'s commands to a camera and keeps the login, renewing the token before its lease
//! runs out. A refused login is remembered and never retried, since failed logins lock the account.
//! Calls block; the camera's self-signed certificate is accepted unchecked.

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

const TIMEOUT: Duration = Duration::from_secs(10);

const FLV_HEAD_WAIT: Duration = Duration::from_secs(3);

const FLV_HEAD_BYTES: usize = 256 * 1024;

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

const RENEW_BEFORE: Duration = Duration::from_secs(300);

struct Token {
    name: String,
    renew_at: Instant,
}

/// A logged-in connection to a Reolink camera's HTTP API.
pub struct ReolinkClient {
    agent: Agent,
    base: String,
    url: String,
    user: String,
    password: String,
    token: Option<Token>,
    /// Returned by every later call without asking the camera again.
    refused: Option<ReolinkError>,
    /// Off until [`features`](Self::features) says so: every camera takes a move without a speed.
    ptz_speed: bool,
}

impl ReolinkClient {
    /// Logs in to the camera's API, with exactly one login attempt.
    pub fn connect(host: &str, port: u16, https: bool, user: &str, password: &str) -> Result<Self> {
        let mut client = Self::new(host, port, https, user, password, TIMEOUT);
        client.login()?;
        Ok(client)
    }

    /// Whether a Reolink's API answers at `host` on `port`: asked without a login, only a Reolink
    /// says "please login first".
    pub fn answers(host: &str, port: u16, https: bool) -> bool {
        let probe = Self::new(host, port, https, "", "", PROBE_TIMEOUT);
        matches!(
            probe.send(&[get_dev_info()], None),
            Err(ReolinkError::SessionExpired)
        )
    }

    fn new(
        host: &str,
        port: u16,
        https: bool,
        user: &str,
        password: &str,
        timeout: Duration,
    ) -> Self {
        let scheme = if https { "https" } else { "http" };
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
            detections: ai.map(|value| read_ai(&value).0).unwrap_or_default(),
            spotlight: has_spotlight(&ability, channel),
            pan_tilt: ptz.pan_tilt,
            zoom: ptz.zoom,
        })
    }

    /// What the camera sees on `channel`, counted from 1, now; its spotlight only when `spotlight`.
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

    /// Starts moving the camera on `channel`, counted from 1, until a stop; a still velocity stops.
    pub fn ptz_move(&mut self, channel: u32, velocity: PtzVelocity) -> Result<()> {
        let channel = reolink_channel(channel);
        let Some((op, speed)) = ptz_op(velocity) else {
            return self.call_one(ptz_ctrl(channel, "Stop", None)).map(drop);
        };
        let with_speed = self.ptz_speed;
        match self.call_one(ptz_ctrl(channel, op, with_speed.then_some(speed))) {
            // The abilities don't always settle whether a move takes a speed: a refused one is
            // tried once the other way, which then sticks.
            Err(ReolinkError::Refused { .. }) => {
                self.call_one(ptz_ctrl(channel, op, (!with_speed).then_some(speed)))?;
                self.ptz_speed = !with_speed;
                Ok(())
            }
            sent => sent.map(drop),
        }
    }

    /// The RTSP streams the camera lists for `channel`, counted from 1.
    pub fn streams(&mut self, channel: u32) -> Result<Vec<CameraStream>> {
        match self.call_one(get_rtsp_url(reolink_channel(channel))) {
            Ok(value) => Ok(read_streams(&value)),
            Err(ReolinkError::Refused { .. } | ReolinkError::Protocol(_)) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    /// The H.264 FLV streams of `channel`, counted from 1, that `listed` (over RTSP) lacks, each
    /// opened at once for its first bytes, since no reply lists them.
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

    /// The address of FLV stream `name`, with this login's token: never print or log it. The token
    /// works only while this client is logged in, so open the stream before dropping the client.
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

    /// Where the zoom of `channel`, counted from 1, is and how far it goes, when the camera says.
    pub fn zoom_position(&mut self, channel: u32) -> Result<Option<ZoomPosition>> {
        match self.call_one(get_zoom_focus(reolink_channel(channel))) {
            Ok(value) => Ok(read_zoom(&value)),
            Err(ReolinkError::Refused { .. } | ReolinkError::Protocol(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Zooms `channel`, counted from 1, to `position`, in the camera's own numbers.
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

    fn call_one(&mut self, command: Value) -> Result<Value> {
        self.call(vec![command])?.pop().ok_or_else(missing_reply)?
    }

    fn call_three(&mut self, commands: [Value; 3]) -> Result<[Result<Value>; 3]> {
        self.call(commands.into())?
            .try_into()
            .map_err(|_: Replies| missing_reply())
    }

    /// The camera's own JPEG of what it sees now, from `Snap`.
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

    fn snap(&self, channel: u32, token: &str) -> Result<Vec<u8>> {
        let asked = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        let mut response = self
            .agent
            .get(&self.url)
            .query("cmd", "Snap")
            .query("channel", channel.to_string())
            // A new value each time, so no cache answers with an old picture.
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
        let text = response.body_mut().read_to_string().map_err(cannot_reach)?;
        Err(match read_replies(&text, 1) {
            Ok(mut replies) => replies
                .pop()
                .and_then(Result::err)
                .unwrap_or_else(|| ReolinkError::Protocol("The camera sent no picture.".into())),
            Err(error) => error,
        })
    }

    /// Logs in again once when the camera has forgotten the token. Some firmware answers a request
    /// with one command it lacks with a single reply, so those are then sent one request each.
    fn call(&mut self, commands: Vec<Value>) -> Result<Replies> {
        let token = self.token()?;
        let replies = match self.send(&commands, Some(&token)) {
            Err(ReolinkError::SessionExpired) => {
                // The session is gone, so there is nothing to log out of.
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

    /// A refusal is remembered, so the camera is never asked again.
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

    fn logout(&mut self) {
        if let Some(token) = self.token.take() {
            let _ = self.send(&[logout()], Some(&token.name));
        }
    }

    fn send(&self, commands: &[Value], token: Option<&str>) -> Result<Replies> {
        let mut request = self.agent.post(&self.url);
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

/// Ends the session, so the camera's few don't run out. It can block for seconds.
impl Drop for ReolinkClient {
    fn drop(&mut self) {
        self.logout();
    }
}

fn agent(timeout: Duration) -> Agent {
    let config = Agent::config_builder()
        .tls_config(TlsConfig::builder().disable_verification(true).build())
        .timeout_global(Some(timeout))
        // The camera's error pages say why, which a status code alone doesn't.
        .http_status_as_error(false)
        .build();
    Agent::new_with_config(config)
}

fn cannot_reach(error: ureq::Error) -> ReolinkError {
    ReolinkError::Unreachable(error.to_string())
}

fn missing_reply() -> ReolinkError {
    ReolinkError::Protocol("The camera's answer is missing a reply.".into())
}
