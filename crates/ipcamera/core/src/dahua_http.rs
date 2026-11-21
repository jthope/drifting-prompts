//! Dahua's HTTP API, in its CGI and XML (`/dev-api/`) dialects, with a digest login. A refused
//! login is remembered and never retried, since failed logins lock the account; the `Lighting`
//! table is the infrared lamp's, never taken for a light. Calls block.

use std::{fmt, ops::Range, time::Duration};

use md5::{Digest, Md5};
use ureq::{Agent, tls::TlsConfig};

use crate::{
    camera_features::{DeviceInfo, Spotlight},
    ptz::DahuaCommand,
};

const TIMEOUT: Duration = Duration::from_secs(10);

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

const XML_HEAD: &str = r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?>"#;

/// Why a call to the API failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DahuaHttpError {
    /// The camera refused the login. Nothing retries it.
    LoginRejected(String),
    /// The camera could not be reached, or the connection broke.
    Unreachable(String),
    /// The camera answered the request with an error.
    Failed(String),
    /// The camera answered with something the client can't use.
    Protocol(String),
}

impl fmt::Display for DahuaHttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoginRejected(reason) => write!(
                f,
                "The camera refused the login to its HTTP API ({reason}). Check the username and \
                 password."
            ),
            Self::Unreachable(reason) => write!(
                f,
                "The camera's HTTP API can't be reached ({reason}). Check the API port and HTTPS."
            ),
            Self::Failed(reason) => write!(f, "The camera's HTTP API refused a request: {reason}."),
            Self::Protocol(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for DahuaHttpError {}

type Result<T> = std::result::Result<T, DahuaHttpError>;

/// Which of Dahua's HTTP APIs a camera speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    /// `GET /cgi-bin/<object>.cgi?action=<method>&…`, answered with `key=value` lines.
    Cgi,
    /// `POST /dev-api/<object>/<method>` with an XML body, answered in XML: Amcrest Link's.
    DevApi,
}

/// What a camera's PTZ does, as `ptz` says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PtzCaps {
    /// Whether it pans or tilts.
    pub pan_tilt: bool,
    /// Whether it zooms.
    pub zoom: bool,
    /// The channel number it answered to: most firmware counts from 0, some from 1.
    pub channel: u32,
}

/// How a camera's light is switched over the API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DahuaLight {
    /// White lights among `Lighting_V2`'s, by channel, profile and light index; every profile's
    /// switched together, since the profile in use changes through the day.
    Lighting(Vec<Vec<usize>>),
    /// `coaxialControlIO`'s white light, on this channel (from 1).
    Coaxial(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Challenge {
    realm: String,
    nonce: String,
    opaque: Option<String>,
    qop: bool,
}

enum Reply {
    Text(String),
    Challenge(Challenge),
}

/// A connection to a Dahua camera's HTTP API, with the login and dialect it has learned.
pub struct DahuaHttp {
    agent: Agent,
    base: String,
    user: String,
    password: String,
    dialect: Dialect,
    challenge: Option<Challenge>,
    count: u32,
    /// Returned by every later call without asking the camera again.
    refused: Option<DahuaHttpError>,
}

impl DahuaHttp {
    /// Logs in once and asks what the camera is: in the CGI, then the XML API if the CGI fails.
    pub fn connect(
        host: &str,
        port: u16,
        https: bool,
        user: &str,
        password: &str,
    ) -> Result<(Self, DeviceInfo)> {
        let mut client = Self::new(host, port, https, user, password, TIMEOUT);
        match client.device_info() {
            Ok(device) => Ok((client, device)),
            Err(DahuaHttpError::Failed(_) | DahuaHttpError::Protocol(_)) => {
                client.dialect = Dialect::DevApi;
                match client.device_info() {
                    Ok(device) => Ok((client, device)),
                    Err(DahuaHttpError::Failed(why) | DahuaHttpError::Protocol(why)) => {
                        Err(DahuaHttpError::Protocol(format!(
                            "Neither of Dahua's HTTP APIs answered on port {port}, though \
                             something did ({why})."
                        )))
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    /// Whether Dahua's HTTP API answers at `host` on `port`, in either dialect, without a login.
    pub fn answers(host: &str, port: u16, https: bool) -> bool {
        let probe = Self::new(host, port, https, "", "", PROBE_TIMEOUT);
        match probe.send(
            "GET",
            "/cgi-bin/magicBox.cgi?action=getDeviceType",
            None,
            None,
        ) {
            Ok(Reply::Challenge(_)) => return true,
            Ok(Reply::Text(text)) if find(&read_values(&text), "type").is_some() => return true,
            Err(DahuaHttpError::Unreachable(_)) => return false,
            _ => {}
        }
        match probe.send(
            "POST",
            "/dev-api/magicBox/getDeviceType",
            Some(&xml_body("")),
            None,
        ) {
            Ok(Reply::Challenge(_)) => true,
            Ok(Reply::Text(text)) => {
                read_xml_values(&text).is_ok_and(|values| find(&values, "type").is_some())
            }
            Err(_) => false,
        }
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
        let config = Agent::config_builder()
            .tls_config(TlsConfig::builder().disable_verification(true).build())
            .timeout_global(Some(timeout))
            // A 401 carries the login's challenge, and an error's body says why.
            .http_status_as_error(false)
            .build();
        Self {
            agent: Agent::new_with_config(config),
            base: format!("{scheme}://{host}:{port}"),
            user: user.to_string(),
            password: password.to_string(),
            dialect: Dialect::Cgi,
            challenge: None,
            count: 0,
            refused: None,
        }
    }

    /// Which dialect the camera speaks.
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// What the camera says it is.
    pub fn device_info(&mut self) -> Result<DeviceInfo> {
        let said = self.values("magicBox", "getDeviceType", "", "")?;
        let model = find(&said, "type")
            .ok_or_else(|| {
                DahuaHttpError::Protocol(
                    "The camera's HTTP API didn't say what it is, so it isn't Dahua's.".into(),
                )
            })?
            .to_string();
        let mut version = |method: &str| match self.values("magicBox", method, "", "") {
            Ok(said) => Ok(find(&said, "version").unwrap_or_default().to_string()),
            Err(DahuaHttpError::Failed(_) | DahuaHttpError::Protocol(_)) => Ok(String::new()),
            Err(error) => Err(error),
        };
        Ok(DeviceInfo {
            model,
            firmware: version("getSoftwareVersion")?,
            hardware: version("getHardwareVersion")?,
        })
    }

    /// What the PTZ on `channel` (from 1) does; `None` when the camera won't say on either count.
    pub fn ptz_caps(&mut self, channel: u32) -> Result<Option<PtzCaps>> {
        let channel = channel.max(1);
        for asked in [channel - 1, channel] {
            match self.values(
                "ptz",
                "getCurrentProtocolCaps",
                &format!("&channel={asked}"),
                &xml_number("channel", asked),
            ) {
                Ok(said) => {
                    // "Tile" is how the firmware spells tilt.
                    let has = |name: &str| {
                        said.iter().any(|(key, value)| {
                            last(key).eq_ignore_ascii_case(name)
                                && value.eq_ignore_ascii_case("true")
                        })
                    };
                    return Ok(Some(PtzCaps {
                        pan_tilt: has("Pan") || has("Tile"),
                        zoom: has("Zoom"),
                        channel: asked,
                    }));
                }
                Err(DahuaHttpError::Failed(_)) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(None)
    }

    /// Starts `command` on PTZ `channel` ([`PtzCaps::channel`]) until [`ptz_stop`](Self::ptz_stop).
    pub fn ptz_start(&mut self, channel: u32, command: DahuaCommand) -> Result<()> {
        self.ptz("start", channel, command.code, command.arg1, command.arg2)
    }

    /// Stops `command`.
    pub fn ptz_stop(&mut self, channel: u32, command: DahuaCommand) -> Result<()> {
        self.ptz("stop", channel, command.code, 0, 0)
    }

    fn ptz(&mut self, method: &str, channel: u32, code: &str, arg1: i32, arg2: i32) -> Result<()> {
        self.call(
            "ptz",
            method,
            &format!("&channel={channel}&code={code}&arg1={arg1}&arg2={arg2}&arg3=0"),
            &format!(
                "{}{}{}{}{}",
                xml_number("channel", channel),
                xml_text("code", code),
                xml_number("arg1", arg1),
                xml_number("arg2", arg2),
                xml_number("arg3", 0)
            ),
        )
        .map(drop)
    }

    /// The light on `channel` (from 1) the app can switch, if the camera has one.
    pub fn light(&mut self, channel: u32) -> Result<Option<DahuaLight>> {
        let channel = channel.max(1);
        let index = (channel - 1) as usize;
        let rows = match self.lighting() {
            Ok(text) => match self.dialect {
                Dialect::Cgi => cgi_white_lights(&read_values(&text), index),
                Dialect::DevApi => xml_white_lights(&text, index).unwrap_or_default(),
            },
            Err(DahuaHttpError::Failed(_) | DahuaHttpError::Protocol(_)) => Vec::new(),
            Err(error) => return Err(error),
        };
        if !rows.is_empty() {
            return Ok(Some(DahuaLight::Lighting(rows)));
        }
        match self.coaxial_status(channel) {
            Ok(said) if find(&said, "WhiteLight").is_some() => {
                Ok(Some(DahuaLight::Coaxial(channel)))
            }
            Ok(_) | Err(DahuaHttpError::Failed(_) | DahuaHttpError::Protocol(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Whether `light` is on (`Auto` is not), and how bright.
    pub fn light_state(&mut self, light: &DahuaLight) -> Result<Spotlight> {
        match light {
            DahuaLight::Lighting(rows) => {
                let text = self.lighting()?;
                let fields: Vec<(Option<String>, Option<u8>)> = match self.dialect {
                    Dialect::Cgi => {
                        let values = read_values(&text);
                        rows.iter()
                            .map(|row| cgi_row_fields(&values, row))
                            .collect()
                    }
                    Dialect::DevApi => xml_row_fields(&text, rows)?,
                };
                let on = fields.iter().any(|(mode, _)| {
                    mode.as_deref().is_some_and(|mode| {
                        mode.eq_ignore_ascii_case("Manual") || mode.eq_ignore_ascii_case("ForceOn")
                    })
                });
                let brightness = fields
                    .iter()
                    .find_map(|(_, brightness)| *brightness)
                    .unwrap_or(100);
                Ok(Spotlight { on, brightness })
            }
            DahuaLight::Coaxial(channel) => {
                let said = self.coaxial_status(*channel)?;
                let on = find(&said, "WhiteLight").ok_or_else(|| {
                    DahuaHttpError::Protocol(
                        "The camera didn't say whether its light is on.".into(),
                    )
                })?;
                Ok(Spotlight {
                    on: on.eq_ignore_ascii_case("On"),
                    brightness: 100,
                })
            }
        }
    }

    /// Switches `light` on or off.
    pub fn set_light(&mut self, light: &DahuaLight, on: bool) -> Result<()> {
        match light {
            DahuaLight::Lighting(rows) => {
                let mode = if on { "Manual" } else { "Off" };
                match self.dialect {
                    Dialect::Cgi => {
                        let settings: String = rows
                            .iter()
                            .map(|row| format!("&Lighting_V2{}.Mode={mode}", indexes(row)))
                            .collect();
                        self.get(&format!("configManager.cgi?action=setConfig{settings}"))
                            .map(drop)
                    }
                    // The XML API sets a table whole: the camera's own text, its modes changed.
                    Dialect::DevApi => {
                        let table = self.lighting()?;
                        let changed = xml_with_modes(&table, rows, mode)?;
                        self.request("POST", "/dev-api/configManager/setConfig", Some(&changed))
                            .map(drop)
                    }
                }
            }
            // Its output: 1 on, 2 off. Type 1 is the white light, 2 the siren.
            DahuaLight::Coaxial(channel) => {
                let io = if on { 1 } else { 2 };
                self.call(
                    "coaxialControlIO",
                    "control",
                    &format!("&channel={channel}&info[0].Type=1&info[0].IO={io}"),
                    &format!(
                        "{}<info><info>{}{}{}</info></info>",
                        xml_number("channel", channel - 1),
                        xml_number("Type", 1),
                        xml_number("IO", io),
                        // A manual trigger, not one linked to an alarm.
                        xml_number("TriggerMode", 2)
                    ),
                )
                .map(drop)
            }
        }
    }

    fn lighting(&mut self) -> Result<String> {
        self.call(
            "configManager",
            "getConfig",
            "&name=Lighting_V2",
            &xml_text("name", "Lighting_V2"),
        )
    }

    fn coaxial_status(&mut self, channel: u32) -> Result<Vec<(String, String)>> {
        let method = match self.dialect {
            Dialect::Cgi => "getStatus",
            Dialect::DevApi => "getstatus",
        };
        // The CGI counts this channel from 1 and the XML API from 0.
        self.values(
            "coaxialControlIO",
            method,
            &format!("&channel={channel}"),
            &xml_number("channel", channel - 1),
        )
    }

    fn call(&mut self, object: &str, method: &str, cgi: &str, xml: &str) -> Result<String> {
        match self.dialect {
            Dialect::Cgi => self.get(&format!("{object}.cgi?action={method}{cgi}")),
            Dialect::DevApi => self.post(&format!("{object}/{method}"), xml),
        }
    }

    fn values(
        &mut self,
        object: &str,
        method: &str,
        cgi: &str,
        xml: &str,
    ) -> Result<Vec<(String, String)>> {
        let text = self.call(object, method, cgi, xml)?;
        match self.dialect {
            Dialect::Cgi => Ok(read_values(&text)),
            Dialect::DevApi => read_xml_values(&text),
        }
    }

    /// One `GET /cgi-bin/<query>` with the login; the reply's body.
    pub fn get(&mut self, query: &str) -> Result<String> {
        self.request("GET", &format!("/cgi-bin/{query}"), None)
    }

    /// One `POST /dev-api/<path>` with `fields` in the XML root and the login; the reply's body.
    pub fn post(&mut self, path: &str, fields: &str) -> Result<String> {
        self.request("POST", &format!("/dev-api/{path}"), Some(&xml_body(fields)))
    }

    fn request(&mut self, method: &str, uri: &str, body: Option<&str>) -> Result<String> {
        if let Some(refused) = &self.refused {
            return Err(refused.clone());
        }

        // The camera says how to log in only to a request without a login.
        let fresh = self.challenge.is_none();
        if fresh {
            match self.send(method, uri, body, None)? {
                Reply::Text(text) => return Ok(text),
                Reply::Challenge(challenge) => self.learn(challenge),
            }
        }
        match self.send_logged_in(method, uri, body)? {
            Reply::Text(text) => Ok(text),
            // A nonce that worked before has run out: one more try with the new one.
            Reply::Challenge(challenge) if !fresh => {
                self.learn(challenge);
                match self.send_logged_in(method, uri, body)? {
                    Reply::Text(text) => Ok(text),
                    Reply::Challenge(_) => Err(self.refuse()),
                }
            }
            Reply::Challenge(_) => Err(self.refuse()),
        }
    }

    fn learn(&mut self, challenge: Challenge) {
        self.challenge = Some(challenge);
        self.count = 0;
    }

    fn refuse(&mut self) -> DahuaHttpError {
        let refused = DahuaHttpError::LoginRejected(
            "HTTP 401: the camera turned the username and password down".into(),
        );
        self.refused = Some(refused.clone());
        refused
    }

    fn send_logged_in(&mut self, method: &str, uri: &str, body: Option<&str>) -> Result<Reply> {
        let Some(challenge) = &self.challenge else {
            return Err(DahuaHttpError::Protocol(
                "The camera never said how to log in.".into(),
            ));
        };
        self.count += 1;
        let header = authorization(
            &self.user,
            &self.password,
            challenge,
            method,
            uri,
            self.count,
            &uuid::Uuid::new_v4().simple().to_string(),
        );
        self.send(method, uri, body, Some(&header))
    }

    fn send(
        &self,
        method: &str,
        uri: &str,
        body: Option<&str>,
        authorization: Option<&str>,
    ) -> Result<Reply> {
        let url = format!("{}{uri}", self.base);
        let sent = if method == "POST" {
            let mut request = self
                .agent
                .post(&url)
                .header("Content-Type", "application/xml; charset=utf-8");
            if let Some(authorization) = authorization {
                request = request.header("Authorization", authorization);
            }
            request.send(body.unwrap_or_default())
        } else {
            let mut request = self.agent.get(&url);
            if let Some(authorization) = authorization {
                request = request.header("Authorization", authorization);
            }
            request.call()
        };
        let mut response = sent.map_err(|error| DahuaHttpError::Unreachable(error.to_string()))?;
        let status = response.status().as_u16();
        if status == 401 {
            let asked: Vec<String> = response
                .headers()
                .get_all("www-authenticate")
                .iter()
                .filter_map(|value| value.to_str().ok())
                .map(str::to_string)
                .collect();
            return read_challenge(&asked).map(Reply::Challenge);
        }
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|error| DahuaHttpError::Unreachable(error.to_string()))?;
        let said = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !(200..300).contains(&status) {
            return Err(DahuaHttpError::Failed(if said.is_empty() {
                format!("HTTP {status}")
            } else {
                format!(
                    "HTTP {status}, {}",
                    said.chars().take(200).collect::<String>()
                )
            }));
        }
        // Some firmware says an error with a 200.
        if said.starts_with("Error") {
            return Err(DahuaHttpError::Failed(said));
        }
        Ok(Reply::Text(text))
    }
}

fn authorization(
    user: &str,
    password: &str,
    challenge: &Challenge,
    method: &str,
    uri: &str,
    count: u32,
    cnonce: &str,
) -> String {
    let known = md5_hex(&format!("{user}:{}:{password}", challenge.realm));
    let asked = md5_hex(&format!("{method}:{uri}"));
    let mut header = format!(
        r#"Digest username="{}", realm="{}", nonce="{}", uri="{}", algorithm=MD5"#,
        quoted(user),
        quoted(&challenge.realm),
        quoted(&challenge.nonce),
        quoted(uri)
    );
    if challenge.qop {
        let count = format!("{count:08x}");
        let response = md5_hex(&format!(
            "{known}:{}:{count}:{cnonce}:auth:{asked}",
            challenge.nonce
        ));
        header.push_str(&format!(
            r#", response="{response}", qop=auth, nc={count}, cnonce="{cnonce}""#
        ));
    } else {
        let response = md5_hex(&format!("{known}:{}:{asked}", challenge.nonce));
        header.push_str(&format!(r#", response="{response}""#));
    }
    if let Some(opaque) = &challenge.opaque {
        header.push_str(&format!(r#", opaque="{}""#, quoted(opaque)));
    }
    header
}

fn quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn md5_hex(text: &str) -> String {
    Md5::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_challenge(headers: &[String]) -> Result<Challenge> {
    let digests: Vec<_> = headers
        .iter()
        .filter_map(|header| {
            let (scheme, rest) = header.trim().split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("Digest")
                .then(|| read_parameters(rest))
        })
        .collect();
    if digests.is_empty() {
        return Err(DahuaHttpError::Protocol(if headers.is_empty() {
            "The camera's HTTP API refused the request without saying how to log in.".into()
        } else {
            format!(
                "The camera's HTTP API asks for a login the app doesn't send ({}): only a digest, \
                 which keeps the password off the network.",
                headers.join("; ")
            )
        }));
    }
    digests
        .into_iter()
        .find(|parameters| {
            parameter(parameters, "algorithm")
                .is_none_or(|algorithm| algorithm.eq_ignore_ascii_case("MD5"))
        })
        .and_then(|parameters| {
            Some(Challenge {
                realm: parameter(&parameters, "realm")?.to_string(),
                nonce: parameter(&parameters, "nonce")?.to_string(),
                opaque: parameter(&parameters, "opaque").map(str::to_string),
                qop: parameter(&parameters, "qop").is_some_and(|qop| {
                    qop.split(',')
                        .any(|option| option.trim().eq_ignore_ascii_case("auth"))
                }),
            })
        })
        .ok_or_else(|| {
            DahuaHttpError::Protocol(
                "The camera's HTTP API asks for a digest login the app doesn't speak (not MD5)."
                    .into(),
            )
        })
}

fn read_parameters(text: &str) -> Vec<(String, String)> {
    let mut parameters = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let Some((name, after)) = rest.split_once('=') else {
            break;
        };
        let name = name
            .trim()
            .trim_start_matches(',')
            .trim()
            .to_ascii_lowercase();
        let after = after.trim_start();
        let (value, left) = if let Some(inside) = after.strip_prefix('"') {
            let mut value = String::new();
            let mut chars = inside.char_indices();
            let mut end = inside.len();
            while let Some((at, c)) = chars.next() {
                match c {
                    '\\' => {
                        if let Some((_, escaped)) = chars.next() {
                            value.push(escaped);
                        }
                    }
                    '"' => {
                        end = at + 1;
                        break;
                    }
                    other => value.push(other),
                }
            }
            (value, &inside[end..])
        } else {
            let end = after.find(',').unwrap_or(after.len());
            (after[..end].trim().to_string(), &after[end..])
        };
        parameters.push((name, value));
        rest = left.trim_start().trim_start_matches(',').trim_start();
    }
    parameters
}

fn parameter<'a>(parameters: &'a [(String, String)], name: &str) -> Option<&'a str> {
    parameters
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn xml_body(fields: &str) -> String {
    format!("{XML_HEAD}<root>{fields}</root>")
}

fn xml_number(name: &str, value: impl fmt::Display) -> String {
    format!(r#"<{name} type="number">{value}</{name}>"#)
}

fn xml_text(name: &str, value: &str) -> String {
    format!(r#"<{name} type="string">{value}</{name}>"#)
}

/// A CGI reply's `key=value` lines, in order.
pub fn read_values(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

/// An XML reply's values, each named by its path below the root (`caps.Pan`).
pub fn read_xml_values(text: &str) -> Result<Vec<(String, String)>> {
    let document = parse_xml(text)?;
    let mut values = Vec::new();
    fn walk(node: roxmltree::Node<'_, '_>, path: &str, values: &mut Vec<(String, String)>) {
        for child in node.children().filter(roxmltree::Node::is_element) {
            let name = child.tag_name().name();
            let path = if path.is_empty() {
                name.to_string()
            } else {
                format!("{path}.{name}")
            };
            if child.children().any(|node| node.is_element()) {
                walk(child, &path, values);
            } else {
                values.push((path, child.text().unwrap_or_default().trim().to_string()));
            }
        }
    }
    walk(document.root_element(), "", &mut values);
    Ok(values)
}

fn parse_xml(text: &str) -> Result<roxmltree::Document<'_>> {
    roxmltree::Document::parse(xml_start(text)).map_err(|error| {
        DahuaHttpError::Protocol(format!(
            "The camera's HTTP API answered with something that isn't XML ({error})."
        ))
    })
}

/// Past a byte order mark and blank lines, which the parser refuses before the declaration.
fn xml_start(text: &str) -> &str {
    text.trim_start_matches('\u{feff}').trim_start()
}

fn last(key: &str) -> &str {
    key.rsplit('.').next().unwrap_or(key)
}

fn find<'a>(values: &'a [(String, String)], name: &str) -> Option<&'a str> {
    values
        .iter()
        .find(|(key, _)| last(key).eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn indexes(row: &[usize]) -> String {
    row.iter().map(|index| format!("[{index}]")).collect()
}

fn cgi_white_lights(values: &[(String, String)], channel: usize) -> Vec<Vec<usize>> {
    let row_of = |key: &str| {
        let mut rest = key.strip_prefix("table.Lighting_V2")?;
        let mut row = Vec::new();
        while let Some(inside) = rest.strip_prefix('[') {
            let end = inside.find(']')?;
            row.push(inside[..end].parse::<usize>().ok()?);
            rest = &inside[end + 1..];
        }
        (row.len() == 3).then_some(row)
    };
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for (key, value) in values {
        if key.ends_with(".LightType")
            && value.eq_ignore_ascii_case("WhiteLight")
            && let Some(row) = row_of(key)
            && row[0] == channel
            && !rows.contains(&row)
        {
            rows.push(row);
        }
    }
    // A row with no brightness is a stub that switches nothing.
    rows.retain(|row| {
        let bank = format!("table.Lighting_V2{}.", indexes(row));
        values.iter().any(|(key, _)| {
            key.strip_prefix(&bank).is_some_and(|field| {
                ["NearLight", "MiddleLight", "FarLight"]
                    .iter()
                    .any(|name| field.starts_with(name))
            })
        })
    });
    rows
}

fn cgi_row_fields(values: &[(String, String)], row: &[usize]) -> (Option<String>, Option<u8>) {
    let field = |name: &str| {
        let key = format!("table.Lighting_V2{}.{name}", indexes(row));
        values
            .iter()
            .find(|(each, _)| *each == key)
            .map(|(_, value)| value.clone())
    };
    let brightness = ["NearLight", "MiddleLight", "FarLight"]
        .into_iter()
        .find_map(|bank| field(&format!("{bank}[0].Light")))
        .and_then(|light| light.trim().parse().ok());
    (field("Mode"), brightness)
}

/// Every row of an XML `Lighting_V2` table (an element with a `LightType`), with its indexes.
fn xml_rows<'a, 'input>(
    document: &'a roxmltree::Document<'input>,
) -> Vec<(Vec<usize>, roxmltree::Node<'a, 'input>)> {
    fn walk<'a, 'input>(
        node: roxmltree::Node<'a, 'input>,
        path: &[usize],
        rows: &mut Vec<(Vec<usize>, roxmltree::Node<'a, 'input>)>,
    ) {
        for (index, child) in node
            .children()
            .filter(roxmltree::Node::is_element)
            .enumerate()
        {
            let mut here = path.to_vec();
            here.push(index);
            if child
                .children()
                .any(|field| field.has_tag_name("LightType"))
            {
                rows.push((here, child));
            } else {
                walk(child, &here, rows);
            }
        }
    }
    let mut rows = Vec::new();
    if let Some(table) = document
        .root_element()
        .children()
        .find(|node| node.has_tag_name("Lighting_V2"))
    {
        walk(table, &[], &mut rows);
    }
    rows
}

fn child<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children().find(|field| field.has_tag_name(name))
}

fn xml_white_lights(text: &str, channel: usize) -> Result<Vec<Vec<usize>>> {
    let document = parse_xml(text)?;
    Ok(xml_rows(&document)
        .into_iter()
        .filter(|(row, node)| {
            row.first() == Some(&channel)
                && child(*node, "LightType")
                    .and_then(|kind| kind.text())
                    .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("WhiteLight"))
                // A row with no brightness is a stub that switches nothing.
                && node.descendants().any(|field| field.has_tag_name("Light"))
        })
        .map(|(row, _)| row)
        .collect())
}

fn xml_row_fields(text: &str, rows: &[Vec<usize>]) -> Result<Vec<(Option<String>, Option<u8>)>> {
    let document = parse_xml(text)?;
    let all = xml_rows(&document);
    Ok(rows
        .iter()
        .filter_map(|row| all.iter().find(|(each, _)| each == row))
        .map(|(_, node)| {
            let mode = child(*node, "Mode")
                .and_then(|mode| mode.text())
                .map(|mode| mode.trim().to_string());
            let brightness = node
                .descendants()
                .find(|field| field.has_tag_name("Light"))
                .and_then(|light| light.text())
                .and_then(|light| light.trim().parse().ok());
            (mode, brightness)
        })
        .collect())
}

fn xml_with_modes(text: &str, rows: &[Vec<usize>], mode: &str) -> Result<String> {
    let start = xml_start(text);
    let document = parse_xml(start)?;
    let all = xml_rows(&document);
    let mut changes: Vec<(Range<usize>, String)> = Vec::new();
    for row in rows {
        let Some((_, node)) = all.iter().find(|(each, _)| each == row) else {
            return Err(DahuaHttpError::Protocol(
                "The camera's lighting table changed while it was being switched. Try again."
                    .into(),
            ));
        };
        match child(*node, "Mode") {
            Some(field) => match field.first_child().filter(|value| value.is_text()) {
                Some(value) => changes.push((value.range(), mode.to_string())),
                None => changes.push((field.range(), xml_text("Mode", mode))),
            },
            None => {
                let range = node.range();
                let end = start[range.clone()]
                    .rfind("</")
                    .map_or(range.end, |at| range.start + at);
                changes.push((end..end, xml_text("Mode", mode)));
            }
        }
    }
    let mut changed = start.to_string();
    changes.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, with) in changes {
        changed.replace_range(range, &with);
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_challenge_is_read_with_its_quotes_and_commas() {
        let challenge = read_challenge(&[
            r#"Basic realm="x""#.to_string(),
            r#"Digest realm="Login to 4L0, \"A\"", qop="auth,auth-int", nonce="1234", opaque="ab""#
                .to_string(),
        ])
        .unwrap();
        assert_eq!(challenge.realm, r#"Login to 4L0, "A""#);
        assert_eq!(challenge.nonce, "1234");
        assert_eq!(challenge.opaque.as_deref(), Some("ab"));
        assert!(challenge.qop);
    }

    #[test]
    fn only_a_digest_with_md5_is_answered() {
        assert!(matches!(
            read_challenge(&[r#"Basic realm="x""#.to_string()]),
            Err(DahuaHttpError::Protocol(_))
        ));
        assert!(
            read_challenge(&[r#"Digest realm="r", nonce="n", algorithm=SHA-256"#.to_string()])
                .is_err()
        );
        let unquoted =
            read_challenge(&["Digest realm=r, nonce=n, algorithm=MD5".to_string()]).unwrap();
        assert_eq!((unquoted.realm.as_str(), unquoted.qop), ("r", false));
    }

    /// RFC 2617's own example, section 3.5.
    #[test]
    fn the_answer_is_rfc_2617s() {
        let challenge = Challenge {
            realm: "testrealm@host.com".into(),
            nonce: "dcd98b7102dd2f0e8b11d0f600bfb0c093".into(),
            opaque: Some("5ccc069c403ebaf9f0171e9517f40e41".into()),
            qop: true,
        };
        let header = authorization(
            "Mufasa",
            "Circle Of Life",
            &challenge,
            "GET",
            "/dir/index.html",
            1,
            "0a4f113b",
        );
        assert!(
            header.contains(r#"response="6629fae49393a05397450978507c4ef1""#),
            "{header}"
        );
        assert!(header.contains("nc=00000001"));
        assert!(!header.contains("Circle Of Life"));
    }

    #[test]
    fn a_light_is_a_white_light_row_with_a_brightness() {
        let values = read_values(
            "table.Lighting_V2[0][0][0].LightType=InfraredLight\r\n\
             table.Lighting_V2[0][0][0].MiddleLight[0].Light=50\r\n\
             table.Lighting_V2[0][0][0].Mode=Auto\r\n\
             table.Lighting_V2[0][0][1].LightType=WhiteLight\r\n\
             table.Lighting_V2[0][0][1].Mode=Off\r\n\
             table.Lighting_V2[0][0][1].NearLight[0].Light=50\r\n\
             table.Lighting_V2[0][2][1].LightType=WhiteLight\r\n\
             table.Lighting_V2[0][2][1].Mode=Off\r\n\
             table.Lighting_V2[1][0][1].LightType=WhiteLight\r\n\
             table.Lighting_V2[1][0][1].NearLight[0].Light=50\r\n",
        );
        assert_eq!(cgi_white_lights(&values, 0), [vec![0, 0, 1]]);
        assert_eq!(cgi_white_lights(&values, 1), [vec![1, 0, 1]]);
        assert!(cgi_white_lights(&values, 2).is_empty());
        assert_eq!(
            cgi_row_fields(&values, &[0, 0, 1]),
            (Some("Off".into()), Some(50))
        );
    }

    const XML_TABLE: &str = r#"
<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<root>
  <Lighting_V2>
    <Lighting_V2>
      <Lighting_V2>
        <Lighting_V2>
          <LightType type="string">InfraredLight</LightType>
          <MiddleLight><MiddleLight><Light type="number">50</Light></MiddleLight></MiddleLight>
          <Mode type="string">Auto</Mode>
        </Lighting_V2>
        <Lighting_V2>
          <LightType type="string">WhiteLight</LightType>
          <Mode type="string">Off</Mode>
          <NearLight><NearLight><Light type="number">80</Light></NearLight></NearLight>
        </Lighting_V2>
      </Lighting_V2>
      <Lighting_V2>
        <Lighting_V2>
          <LightType type="string">WhiteLight</LightType>
          <Mode type="string"></Mode>
          <NearLight><NearLight><Light type="number">80</Light></NearLight></NearLight>
        </Lighting_V2>
        <Lighting_V2>
          <LightType type="string">WhiteLight</LightType>
        </Lighting_V2>
      </Lighting_V2>
    </Lighting_V2>
  </Lighting_V2>
</root>"#;

    #[test]
    fn an_xml_table_finds_its_white_lights_and_changes_only_their_modes() {
        let rows = xml_white_lights(XML_TABLE, 0).unwrap();
        assert_eq!(
            rows,
            [vec![0, 0, 1], vec![0, 1, 0]],
            "not the stub, not the lamp"
        );
        assert!(xml_white_lights(XML_TABLE, 1).unwrap().is_empty());
        assert_eq!(
            xml_row_fields(XML_TABLE, &rows).unwrap(),
            [(Some("Off".into()), Some(80)), (None, Some(80))],
            "an empty mode is none"
        );

        let on = xml_with_modes(XML_TABLE, &rows, "Manual").unwrap();
        assert_eq!(
            xml_row_fields(&on, &rows).unwrap(),
            [
                (Some("Manual".into()), Some(80)),
                (Some("Manual".into()), Some(80))
            ]
        );
        assert!(
            on.contains(r#"<Mode type="string">Auto</Mode>"#),
            "the lamp stays"
        );
        assert_eq!(
            on.matches("<Mode").count(),
            XML_TABLE.matches("<Mode").count(),
            "nothing else added"
        );
        assert!(on.starts_with("<?xml"), "the declaration first");
    }

    #[test]
    fn an_xml_reply_reads_as_names_and_values() {
        let values = read_xml_values(
            r#"<?xml version="1.0"?><root><caps><Pan type="bool">True</Pan><Zoom type="bool">False</Zoom></caps><status><whitelight type="string">on</whitelight></status></root>"#,
        )
        .unwrap();
        assert_eq!(find(&values, "pan"), Some("True"));
        assert_eq!(find(&values, "WhiteLight"), Some("on"));
        assert_eq!(values[0].0, "caps.Pan");
    }
}
