//! What the app asks a camera over ONVIF, which `oxvif` speaks: what it is, its streams, PTZ, light
//! and events, and finding cameras. Calls block on a runtime of their own, so keep a client off
//! async runtimes' threads; a refused login is remembered and never sent again.

use std::{
    collections::HashMap,
    fmt,
    net::Ipv4Addr,
    time::{Duration, Instant},
};

use oxvif::{
    soap::SoapError,
    transport::TransportError,
    types::{NotificationMessage, PtzSpaceRange},
};
use tokio::runtime::Runtime;

use crate::{
    camera_features::{
        CameraActivity, CameraFeatures, CameraStream, Detection, DeviceInfo, FoundCamera, rtsp_path,
    },
    ptz::PtzVelocity,
};

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

const PULL_WAIT: &str = "PT1S";

/// Renewed well before it runs out.
const SUBSCRIPTION: &str = "PT60S";
const RENEW_EVERY: Duration = Duration::from_secs(30);

const ROUNDS: usize = 2;
const ROUND_WAIT: Duration = Duration::from_millis(1500);
const ROUND_PAUSE: Duration = Duration::from_millis(300);

const SWEEP_WAIT: Duration = Duration::from_secs(2);
/// This machine's own few networks, not every virtual one.
const SWEEP_NETWORKS: usize = 4;

/// Why a call to a camera's ONVIF services failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OnvifError {
    /// The camera refused the login. Nothing retries it.
    LoginRejected(String),
    /// The camera could not be reached, or the connection broke.
    Unreachable(String),
    /// The camera answered with a SOAP fault.
    Fault(String),
    /// The camera answered with something that isn't ONVIF.
    Protocol(String),
}

impl fmt::Display for OnvifError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoginRejected(reason) => write!(
                f,
                "The camera refused the ONVIF login ({reason}). Check the username and password, \
                 and that ONVIF is on with this account."
            ),
            Self::Unreachable(reason) => write!(
                f,
                "The camera's ONVIF service can't be reached ({reason}). Check the ONVIF port and \
                 that ONVIF is on."
            ),
            Self::Fault(reason) => write!(f, "The camera refused an ONVIF request: {reason}."),
            Self::Protocol(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for OnvifError {}

type Result<T> = std::result::Result<T, OnvifError>;

type Asked<T> = std::result::Result<T, oxvif::OnvifError>;

/// Where a camera's services are, as paths on its address.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Services {
    /// The event service.
    pub events: Option<String>,
    /// The media service.
    pub media: Option<String>,
    /// The PTZ service.
    pub ptz: Option<String>,
}

/// What a camera's PTZ service says it does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PtzNode {
    /// Whether it pans and tilts at a speed.
    pub pan_tilt: bool,
    /// Whether it zooms at a speed.
    pub zoom: bool,
    /// The auxiliary commands that switch its light on and off, never an infrared lamp.
    pub light: Option<(String, String)>,
}

/// A logged-in connection to a camera's ONVIF services.
pub struct OnvifClient {
    /// Before the runtime, so it is dropped first.
    client: oxvif::OnvifClient,
    runtime: Runtime,
    base: String,
    /// After which nothing is sent.
    refused: Option<OnvifError>,
}

impl OnvifClient {
    /// Whether an ONVIF device service answers at `host`, `port` and `path`, without a login.
    pub fn answers(host: &str, port: u16, path: &str) -> bool {
        let Ok(runtime) = runtime() else {
            return false;
        };
        let probe = oxvif::OnvifClient::new(format!("{}{}", base_of(host, port), with_slash(path)));
        runtime.block_on(async {
            let asked = tokio::time::timeout(PROBE_TIMEOUT, probe.get_system_date_and_time()).await;
            matches!(asked, Ok(Ok(_)))
        })
    }

    /// Asks the camera's time for the login's digest, then logs in once by asking what it is.
    pub fn connect(
        host: &str,
        port: u16,
        path: &str,
        user: &str,
        password: &str,
    ) -> Result<(Self, DeviceInfo)> {
        let runtime = runtime()?;
        let base = base_of(host, port);
        let url = format!("{base}{}", with_slash(path));
        // Without a login. A camera that isn't there is not asked again.
        let asked = runtime.block_on(oxvif::OnvifClient::new(&url).get_system_date_and_time());
        let clock_offset = match asked {
            Ok(time) => time.utc_offset_secs(),
            Err(error @ oxvif::OnvifError::Transport(TransportError::Http(_))) => {
                return Err(told(error));
            }
            Err(_) => 0,
        };
        let mut client = Self {
            client: oxvif::OnvifClient::new(url)
                .with_credentials(user, password)
                .with_utc_offset(clock_offset),
            runtime,
            base,
            refused: None,
        };
        let device = client.device_info()?;
        Ok((client, device))
    }

    /// What the camera says it is.
    pub fn device_info(&mut self) -> Result<DeviceInfo> {
        let info = self.ask(async |client| client.get_device_info().await)?;
        Ok(DeviceInfo {
            model: [info.manufacturer.trim(), info.model.trim()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
            firmware: info.firmware_version.trim().to_string(),
            hardware: info.hardware_id.trim().to_string(),
        })
    }

    /// Where the camera's event, media and PTZ services are.
    pub fn services(&mut self) -> Result<Services> {
        let capabilities = self.ask(async |client| client.get_capabilities().await)?;
        let path = |address: Option<String>| {
            address
                .as_deref()
                .map(str::trim)
                .filter(|address| !address.is_empty())
                .map(path_of)
        };
        Ok(Services {
            events: path(capabilities.events.url),
            media: path(capabilities.media.url),
            ptz: path(capabilities.ptz.url),
        })
    }

    /// The token of the first media profile with a PTZ configuration, else the first.
    pub fn profile_token(&mut self, media_path: &str) -> Result<String> {
        let url = self.url(media_path);
        let profiles = self.ask(async |client| client.get_profiles(&url).await)?;
        profiles
            .iter()
            .find(|profile| profile.ptz_config_token.is_some())
            .or_else(|| profiles.first())
            .map(|profile| profile.token.clone())
            .ok_or_else(|| OnvifError::Fault("the camera has no media profile".into()))
    }

    /// Each profile's picture by token, from the second media service: the first can't name H.265.
    fn pictures(&mut self) -> HashMap<String, String> {
        let Ok(services) = self.ask(async |client| client.get_services().await) else {
            return HashMap::new();
        };
        let Some(url) = services
            .iter()
            .find(|service| service.is_media2() && !service.url.trim().is_empty())
            .map(|service| self.url(&path_of(service.url.trim())))
        else {
            return HashMap::new();
        };
        let (Ok(profiles), Ok(encoders)) = (
            self.ask(async |client| client.get_profiles_media2(&url).await),
            self.ask(async |client| client.get_video_encoder_configurations_media2(&url).await),
        ) else {
            return HashMap::new();
        };
        profiles
            .into_iter()
            .filter_map(|profile| {
                let encoder = encoders
                    .iter()
                    .find(|encoder| Some(&encoder.token) == profile.video_encoder_token.as_ref())?;
                let picture = picture_of(
                    encoder.resolution.width,
                    encoder.resolution.height,
                    encoder.encoding.as_str(),
                );
                Some((profile.token, picture))
            })
            .collect()
    }

    /// The streams the camera's media service lists, each with an RTSP address.
    pub fn streams(&mut self, media_path: &str) -> Result<Vec<CameraStream>> {
        let url = self.url(media_path);
        let profiles = self.ask(async |client| client.get_profiles(&url).await)?;
        let encoders = self
            .ask(async |client| client.get_video_encoder_configurations(&url).await)
            .unwrap_or_default();
        let mut pictures = self.pictures();

        let mut streams = Vec::new();
        for profile in profiles {
            let detail = pictures.remove(&profile.token).unwrap_or_else(|| {
                encoders
                    .iter()
                    .find(|encoder| Some(&encoder.token) == profile.video_encoder_token.as_ref())
                    .map(|encoder| {
                        picture_of(
                            encoder.resolution.width,
                            encoder.resolution.height,
                            encoder.encoding.as_str(),
                        )
                    })
                    .unwrap_or_default()
            });
            let asked = self.ask(async |client| client.get_stream_uri(&url, &profile.token).await);
            let address = match asked {
                Ok(address) => address,
                // The rest would fail the same way.
                Err(error @ (OnvifError::LoginRejected(_) | OnvifError::Unreachable(_))) => {
                    return Err(error);
                }
                Err(_) => continue,
            };
            if let Some(path) = rtsp_path(&address.uri) {
                let name = match profile.name.trim() {
                    "" => profile.token,
                    name => name.to_string(),
                };
                CameraStream::merge(&mut streams, vec![CameraStream { name, path, detail }]);
            }
        }
        Ok(streams)
    }

    /// What the camera's PTZ service does.
    pub fn ptz_node(&mut self, ptz_path: &str) -> Result<PtzNode> {
        let url = self.url(ptz_path);
        let nodes = self.ask(async |client| client.ptz_get_nodes(&url).await)?;
        let commands: Vec<String> = nodes
            .iter()
            .flat_map(|node| &node.aux_commands)
            .map(|command| command.trim().to_string())
            .filter(|command| !command.is_empty())
            .collect();
        Ok(PtzNode {
            pan_tilt: nodes.iter().any(|node| at_a_speed(&node.pan_tilt_spaces)),
            zoom: nodes.iter().any(|node| at_a_speed(&node.zoom_spaces)),
            light: light_commands(&commands),
        })
    }

    /// Starts moving at `velocity` until [`stop`](Self::stop); zoom wins over pan and tilt.
    pub fn continuous_move(
        &mut self,
        ptz_path: &str,
        profile: &str,
        velocity: PtzVelocity,
    ) -> Result<()> {
        let url = self.url(ptz_path);
        let part = |speed: f64| speed.clamp(-1.0, 1.0) as f32;
        let (pan, tilt, zoom) = if velocity.zoom.abs() > 0.01 {
            (0.0, 0.0, part(velocity.zoom))
        } else {
            (part(velocity.pan), part(velocity.tilt), 0.0)
        };
        self.ask(async |client| {
            client
                .ptz_continuous_move(&url, profile, pan, tilt, zoom)
                .await
        })
    }

    /// Stops a move, pan, tilt and zoom alike.
    pub fn stop(&mut self, ptz_path: &str, profile: &str) -> Result<()> {
        let url = self.url(ptz_path);
        self.ask(async |client| client.ptz_stop(&url, profile).await)
    }

    /// Sends an auxiliary command (`Light|On`).
    pub fn auxiliary(&mut self, ptz_path: &str, profile: &str, command: &str) -> Result<()> {
        let url = self.url(ptz_path);
        self.ask(async |client| {
            let sent = client
                .ptz_send_auxiliary_command(&url, profile, command)
                .await;
            match sent {
                // The reply's text is optional: a camera that leaves it out took the command.
                Ok(_) | Err(oxvif::OnvifError::Soap(SoapError::MissingField(_))) => Ok(()),
                Err(error) => Err(error),
            }
        })
    }

    /// Whether the camera reports motion, and what it tells apart, from the topics it can send.
    pub fn features(&mut self, events_path: &str) -> Result<CameraFeatures> {
        let url = self.url(events_path);
        let properties = self.ask(async |client| client.get_event_properties(&url).await)?;
        Ok(features_of(&properties.topics))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, with_slash(path))
    }

    /// One call, blocking; a refused login is remembered, so nothing is sent after it.
    fn ask<T>(&mut self, call: impl AsyncFnOnce(&oxvif::OnvifClient) -> Asked<T>) -> Result<T> {
        if let Some(refused) = &self.refused {
            return Err(refused.clone());
        }
        let answered = self.runtime.block_on(call(&self.client)).map_err(told);
        if let Err(error @ OnvifError::LoginRejected(_)) = &answered {
            self.refused = Some(error.clone());
        }
        answered
    }
}

/// A camera's events, from a pull-point subscription, with the last state of each topic: most
/// cameras send a state only when it changes. Dropping it ends the subscription.
pub struct OnvifEvents {
    client: OnvifClient,
    subscription: String,
    renew_at: Instant,
    states: HashMap<String, (Kind, bool)>,
}

impl OnvifEvents {
    /// Subscribes to the camera's events; when it can't, the client comes back with why.
    pub fn subscribe(
        mut client: OnvifClient,
        events_path: &str,
    ) -> std::result::Result<Self, Box<(OnvifError, OnvifClient)>> {
        let url = client.url(events_path);
        let made = client.ask(async |client| {
            client
                .create_pull_point_subscription(&url, None, Some(SUBSCRIPTION))
                .await
        });
        match made {
            Ok(made) => Ok(Self {
                subscription: client.url(&path_of(made.reference_url.trim())),
                client,
                renew_at: Instant::now() + RENEW_EVERY,
                states: HashMap::new(),
            }),
            Err(error) => Err(Box::new((error, client))),
        }
    }

    /// The client the events come through.
    pub fn client(&mut self) -> &mut OnvifClient {
        &mut self.client
    }

    /// Waits up to a second for events, and returns what the camera sees after them.
    pub fn activity(&mut self) -> Result<CameraActivity> {
        let url = &self.subscription;
        if Instant::now() >= self.renew_at {
            self.client
                .ask(async |client| client.renew_subscription(url, SUBSCRIPTION).await)?;
            self.renew_at = Instant::now() + RENEW_EVERY;
        }
        let pulled = self
            .client
            .ask(async |client| client.pull_messages(url, PULL_WAIT, 64).await)?;
        for (topic, kind, on) in pulled.iter().filter_map(state_of) {
            self.states.insert(topic, (kind, on));
        }
        Ok(activity_of(self.states.values().copied()))
    }
}

impl Drop for OnvifEvents {
    /// Ends the subscription, so the camera's few don't run out. It can block for seconds.
    fn drop(&mut self) {
        let url = &self.subscription;
        let _ = self
            .client
            .ask(async |client| client.unsubscribe(url).await);
    }
}

/// The ONVIF cameras on this machine's networks, by address, over WS-Discovery with no login.
/// Blocks for about three seconds.
pub fn discover() -> Vec<FoundCamera> {
    let Ok(runtime) = runtime() else {
        return Vec::new();
    };
    let answered: Vec<_> = runtime.block_on(async {
        let (heard, swept) = tokio::join!(
            oxvif::discovery::probe_rounds(ROUNDS, ROUND_WAIT, ROUND_PAUSE),
            sweep()
        );
        heard.into_iter().chain(swept).collect()
    });
    let mut found: Vec<FoundCamera> = answered.iter().filter_map(found_of).collect();
    found.sort_by(|a, b| by_address(&a.host).cmp(&by_address(&b.host)));
    found.dedup_by(|a, b| a.host == b.host);
    found
}

/// Asks each private address by itself: Wi-Fi often drops a question sent to everyone.
async fn sweep() -> Vec<oxvif::discovery::DiscoveredDevice> {
    let mut asked = tokio::task::JoinSet::new();
    for network in private_networks() {
        for address in neighbors(network) {
            asked.spawn(oxvif::discovery::probe_unicast(address.into(), SWEEP_WAIT));
        }
    }
    let mut answered = Vec::new();
    while let Some(joined) = asked.join_next().await {
        answered.extend(joined.unwrap_or_default());
    }
    answered
}

pub(crate) fn private_networks() -> Vec<Ipv4Addr> {
    oxvif::discovery::discovery_interfaces()
        .unwrap_or_default()
        .into_iter()
        .map(|interface| interface.address)
        .filter(Ipv4Addr::is_private)
        .take(SWEEP_NETWORKS)
        .collect()
}

pub(crate) fn neighbors(own: Ipv4Addr) -> impl Iterator<Item = Ipv4Addr> {
    let [a, b, c, mine] = own.octets();
    (1..=254)
        .filter(move |last| *last != mine)
        .map(move |last| Ipv4Addr::new(a, b, c, last))
}

fn found_of(device: &oxvif::discovery::DiscoveredDevice) -> Option<FoundCamera> {
    let (host, onvif_port, onvif_device_path) = device
        .xaddrs
        .iter()
        .filter_map(|address| service_of(address))
        // IPv4 first: an IPv6 address is often the link's own.
        .min_by_key(|(host, ..)| host.contains(':'))?;
    Some(FoundCamera {
        host,
        onvif_port: Some(onvif_port),
        onvif_device_path: Some(onvif_device_path),
        rtsp_port: None,
        name: scope_of(&device.scopes, "name"),
        model: scope_of(&device.scopes, "hardware"),
        details: details_of(&device.scopes),
    })
}

fn service_of(address: &str) -> Option<(String, u16, String)> {
    let rest = address.trim().strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.strip_prefix('[') {
        Some(bracketed) => {
            let (host, after) = bracketed.split_once(']')?;
            (host, after.strip_prefix(':'))
        }
        None => match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        },
    };
    let port = match port {
        Some(port) => port.parse().ok()?,
        None => 80,
    };
    let nowhere = host.is_empty() || host == "0.0.0.0" || host == "::";
    (!nowhere).then(|| (host.to_string(), port, path.to_string()))
}

fn scope_of(scopes: &[String], kind: &str) -> String {
    let prefix = format!("onvif://www.onvif.org/{kind}/");
    scopes
        .iter()
        .find_map(|scope| {
            let head = scope.get(..prefix.len())?;
            head.eq_ignore_ascii_case(&prefix)
                .then(|| unescaped(&scope[prefix.len()..]))
        })
        .map(|said| said.trim().to_string())
        .unwrap_or_default()
}

/// What a maker adds to a device's `scopes` (a serial number, a MAC address), as `Kind: value`.
fn details_of(scopes: &[String]) -> Vec<String> {
    const PREFIX: &str = "onvif://www.onvif.org/";
    let mut details: Vec<String> = Vec::new();
    for scope in scopes {
        let Some(said) = scope
            .get(..PREFIX.len())
            .filter(|head| head.eq_ignore_ascii_case(PREFIX))
            .map(|_| &scope[PREFIX.len()..])
        else {
            continue;
        };
        let Some((kind, value)) = said.split_once('/') else {
            continue;
        };
        let value = unescaped(value);
        let value = value.trim().trim_matches('/');
        let skipped = [
            "name",
            "hardware",
            "type",
            "profile",
            "location",
            "extension",
        ]
        .iter()
        .any(|own| kind.eq_ignore_ascii_case(own));
        if skipped || kind.is_empty() || value.is_empty() {
            continue;
        }
        let mut label = kind.to_string();
        label[..1].make_ascii_uppercase();
        let detail = format!("{label}: {value}");
        if !details.contains(&detail) {
            details.push(detail);
        }
    }
    details
}

fn unescaped(text: &str) -> String {
    percent_encoding::percent_decode_str(text)
        .decode_utf8_lossy()
        .into_owned()
}

/// IPv4 addresses by their numbers, so `.9` comes before `.10`, then any other by its text.
fn by_address(host: &str) -> (bool, [u8; 4], &str) {
    match host.parse::<Ipv4Addr>() {
        Ok(address) => (false, address.octets(), host),
        Err(_) => (true, [0; 4], host),
    }
}

fn runtime() -> Result<Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| OnvifError::Protocol(format!("ONVIF couldn't be started: {error}.")))
}

/// Tells a refused login apart from every other failure, since it alone is never tried again:
/// `oxvif` has no error of its own for it.
fn told(error: oxvif::OnvifError) -> OnvifError {
    const NOT_ONVIF: &str =
        "The camera answered with something that isn't ONVIF. Check the ONVIF port and path.";
    match error {
        oxvif::OnvifError::Transport(TransportError::HttpStatus { status: 401, .. }) => {
            OnvifError::LoginRejected("HTTP 401".into())
        }
        oxvif::OnvifError::Transport(TransportError::HttpStatus { status, .. }) => {
            OnvifError::Protocol(format!(
                "The camera answered ONVIF with HTTP {status}. Check the ONVIF port and path."
            ))
        }
        oxvif::OnvifError::Transport(TransportError::Http(error)) => {
            OnvifError::Unreachable(error.without_url().to_string())
        }
        oxvif::OnvifError::Soap(SoapError::Fault {
            code,
            reason,
            subcode,
            detail,
        }) => {
            let said = [
                Some(code.as_str()),
                subcode.as_deref(),
                Some(reason.as_str()),
                detail.as_deref(),
            ];
            let refused = said.into_iter().flatten().any(refuses_login);
            let reason = match reason.trim() {
                "" => "no reason given".to_string(),
                reason => reason.to_string(),
            };
            if refused {
                OnvifError::LoginRejected(reason)
            } else {
                OnvifError::Fault(reason)
            }
        }
        oxvif::OnvifError::Soap(_) => OnvifError::Protocol(NOT_ONVIF.into()),
        oxvif::OnvifError::InvalidArgument(reason) => OnvifError::Protocol(reason),
    }
}

fn refuses_login(said: &str) -> bool {
    let said = said.to_ascii_lowercase();
    [
        "notauthorized",
        "not authorized",
        "unauthorized",
        "failedauthentication",
    ]
    .iter()
    .any(|word| said.contains(word))
}

/// Whether a PTZ node has a velocity space, which a continuous move needs.
fn at_a_speed(spaces: &[PtzSpaceRange]) -> bool {
    spaces
        .iter()
        .any(|space| space.uri.to_ascii_lowercase().contains("velocity"))
}

fn picture_of(width: u32, height: u32, encoding: &str) -> String {
    let size = (width > 0 && height > 0).then(|| format!("{width}\u{d7}{height}"));
    let encoding = Some(encoding.trim().to_string()).filter(|encoding| !encoding.is_empty());
    let parts: Vec<String> = size.into_iter().chain(encoding).collect();
    parts.join(", ")
}

/// The commands that switch a light on and off, never an infrared lamp's.
fn light_commands(commands: &[String]) -> Option<(String, String)> {
    let light = commands.iter().find(|command| {
        let name = command.to_ascii_lowercase();
        (name.contains("light") || name.contains("lamp"))
            && !["irlamp", "irlight", "infrared", "ir_lamp", "ir-lamp"]
                .iter()
                .any(|infrared| name.contains(infrared))
    })?;
    let base = light.split('|').next()?.trim();
    Some((format!("{base}|On"), format!("{base}|Off")))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Motion,
    Detected(Detection),
}

/// What a topic is about, by the words in its parts. Some match only a part's start, so `Face` is
/// not found in `Interface`, nor `Pet` in `Competition`.
fn kind_of(topic: &str) -> Option<Kind> {
    let topic = topic.to_ascii_lowercase();
    let parts: Vec<&str> = topic.split(['/', ':']).collect();
    let has = |words: &[&str]| {
        parts
            .iter()
            .any(|part| words.iter().any(|word| part.contains(word)))
    };
    let starts = |words: &[&str]| {
        parts
            .iter()
            .any(|part| words.iter().any(|word| part.starts_with(word)))
    };
    if has(&["people", "human", "person"]) {
        Some(Kind::Detected(Detection::Person))
    } else if has(&["vehicle"]) {
        Some(Kind::Detected(Detection::Vehicle))
    } else if has(&["dogcat", "animal"]) || starts(&["pet"]) {
        Some(Kind::Detected(Detection::Animal))
    } else if starts(&["face"]) {
        Some(Kind::Detected(Detection::Face))
    } else if has(&["package", "parcel"]) {
        Some(Kind::Detected(Detection::Package))
    } else if has(&["motion"]) {
        Some(Kind::Motion)
    } else {
        None
    }
}

fn activity_of(states: impl Iterator<Item = (Kind, bool)>) -> CameraActivity {
    let mut activity = CameraActivity::default();
    for (kind, _) in states.filter(|(_, on)| *on) {
        match kind {
            Kind::Motion => activity.motion = true,
            Kind::Detected(detection) if !activity.detected.contains(&detection) => {
                activity.detected.push(detection);
            }
            Kind::Detected(_) => {}
        }
    }
    activity.detected.sort();
    activity
}

/// A notification's topic with its source, so a recorder's channels each keep their own state, what
/// it is about, and its state.
fn state_of(message: &NotificationMessage) -> Option<(String, Kind, bool)> {
    let kind = kind_of(&message.topic)?;
    let truth = |value: &String| match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    };
    let on = ["IsMotion", "State"]
        .into_iter()
        .find_map(|name| truth(message.data.get(name)?))
        .or_else(|| {
            let mut items: Vec<_> = message.data.iter().collect();
            items.sort();
            items.into_iter().find_map(|(_, value)| truth(value))
        })?;
    let mut source: Vec<String> = message
        .source
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    source.sort();
    Some((format!("{}|{}", message.topic, source.join(",")), kind, on))
}

fn features_of(topics: &[String]) -> CameraFeatures {
    let mut features = CameraFeatures::default();
    for topic in topics {
        match kind_of(topic) {
            Some(Kind::Motion) => features.motion = true,
            Some(Kind::Detected(detection)) if !features.detections.contains(&detection) => {
                features.detections.push(detection);
            }
            _ => {}
        }
    }
    features.detections.sort();
    features
}

/// Only the path of an address the camera gives: many give one they don't answer on.
fn path_of(address: &str) -> String {
    let after_scheme = address.split_once("://").map_or(address, |(_, rest)| rest);
    match after_scheme.find('/') {
        Some(slash) => after_scheme[slash..].to_string(),
        None => "/".to_string(),
    }
}

fn base_of(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("http://[{host}]:{port}")
    } else {
        format!("http://{host}:{port}")
    }
}

fn with_slash(path: &str) -> String {
    let path = path.trim();
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_the_camera_gives_is_kept_for_its_path() {
        assert_eq!(
            path_of("http://0.0.0.0:8000/onvif/event_service"),
            "/onvif/event_service"
        );
        assert_eq!(
            path_of("http://10.0.0.5/onvif/Subscription?Idx=3"),
            "/onvif/Subscription?Idx=3"
        );
        assert_eq!(path_of("http://10.0.0.5"), "/");
    }

    #[test]
    fn topics_are_told_apart_by_their_words() {
        assert_eq!(
            kind_of("tns1:RuleEngine/CellMotionDetector/Motion"),
            Some(Kind::Motion)
        );
        assert_eq!(kind_of("tns1:VideoSource/MotionAlarm"), Some(Kind::Motion));
        assert_eq!(
            kind_of("tns1:RuleEngine/MyRuleDetector/PeopleDetect"),
            Some(Kind::Detected(Detection::Person))
        );
        assert_eq!(
            kind_of("tns1:RuleEngine/MyRuleDetector/VehicleDetect"),
            Some(Kind::Detected(Detection::Vehicle))
        );
        assert_eq!(
            kind_of("tns1:RuleEngine/MyRuleDetector/DogCatDetect"),
            Some(Kind::Detected(Detection::Animal))
        );
        assert_eq!(
            kind_of("tns1:VideoSource/GlobalSceneChange/ImagingService"),
            None
        );
        assert_eq!(
            kind_of("tns1:Device/Network/InterfaceDown"),
            None,
            "not a face"
        );
        assert_eq!(
            kind_of("tns1:RuleEngine/FaceDetector/FaceDetect"),
            Some(Kind::Detected(Detection::Face))
        );
    }

    fn notification(topic: &str, source: &str, data: &[(&str, &str)]) -> NotificationMessage {
        NotificationMessage {
            topic: topic.to_string(),
            utc_time: String::new(),
            property_operation: "Changed".to_string(),
            source: HashMap::from([("Source".to_string(), source.to_string())]),
            data: data
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
        }
    }

    #[test]
    fn a_pull_says_each_topics_state_and_leaves_out_the_rest() {
        let pulled = [
            notification(
                "tns1:RuleEngine/CellMotionDetector/Motion",
                "0",
                &[("IsMotion", "true")],
            ),
            notification(
                "tns1:RuleEngine/MyRuleDetector/PeopleDetect",
                "0",
                &[("State", "false")],
            ),
            notification(
                "tns1:Device/Trigger/DigitalInput",
                "0",
                &[("LogicalState", "true")],
            ),
            notification("tns1:VideoSource/MotionAlarm", "0", &[("Level", "high")]),
        ];
        let read: Vec<_> = pulled.iter().filter_map(state_of).collect();
        assert_eq!(
            read.len(),
            2,
            "the digital input is about nothing the app shows, and the last says no state"
        );
        assert_eq!(read[0].1, Kind::Motion);
        assert!(read[0].2);
        assert_eq!(read[1].1, Kind::Detected(Detection::Person));
        assert!(!read[1].2);

        let activity = activity_of(read.iter().map(|(_, kind, on)| (*kind, *on)));
        assert!(activity.motion);
        assert!(activity.detected.is_empty());
    }

    #[test]
    fn a_state_is_kept_for_each_source_and_named_by_its_usual_item() {
        let motion = "tns1:RuleEngine/CellMotionDetector/Motion";
        let first = state_of(&notification(motion, "1", &[("IsMotion", "1")])).unwrap();
        let second = state_of(&notification(motion, "2", &[("IsMotion", "0")])).unwrap();
        assert_ne!(
            first.0, second.0,
            "a recorder's channels each keep their own"
        );
        assert!(first.2 && !second.2);

        let both = notification(motion, "1", &[("Armed", "true"), ("IsMotion", "false")]);
        assert!(
            !state_of(&both).unwrap().2,
            "the motion, not what comes first"
        );
    }

    #[test]
    fn the_topics_say_what_the_camera_can_tell() {
        let topics = [
            "RuleEngine/CellMotionDetector/Motion",
            "RuleEngine/MyRuleDetector/PeopleDetect",
            "RuleEngine/MyRuleDetector/VehicleDetect",
            "RuleEngine/MyRuleDetector/PeopleDetect",
            "Device/Trigger/DigitalInput",
        ]
        .map(str::to_string);
        let features = features_of(&topics);
        assert!(features.motion);
        assert_eq!(features.detections, [Detection::Person, Detection::Vehicle]);
        assert!(!features.spotlight && !features.pan_tilt);
        assert_eq!(features_of(&[]), CameraFeatures::default());
    }

    #[test]
    fn a_light_is_one_the_ptz_service_can_switch_and_not_an_infrared_lamp() {
        let listed = |commands: &[&str]| {
            light_commands(
                &commands
                    .iter()
                    .map(|command| command.to_string())
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            listed(&["tt:Wiper|On", "tt:Light|On", "tt:Light|Off"]),
            Some(("tt:Light|On".into(), "tt:Light|Off".into()))
        );
        assert_eq!(
            listed(&["WhiteLight"]),
            Some(("WhiteLight|On".into(), "WhiteLight|Off".into()))
        );
        assert_eq!(
            listed(&["tt:IRLamp|On", "tt:IRLamp|Off", "tt:Wiper|On"]),
            None
        );
        assert_eq!(listed(&[]), None);
    }

    #[test]
    fn a_refused_login_is_told_apart_from_other_faults() {
        let fault = |subcode: Option<&str>, reason: &str| {
            told(oxvif::OnvifError::Soap(SoapError::Fault {
                code: "s:Sender".into(),
                reason: reason.into(),
                subcode: subcode.map(str::to_string),
                detail: None,
            }))
        };
        assert_eq!(
            fault(Some("ter:NotAuthorized"), "Sender not Authorized"),
            OnvifError::LoginRejected("Sender not Authorized".into())
        );
        assert_eq!(
            fault(Some("wsse:FailedAuthentication"), ""),
            OnvifError::LoginRejected("no reason given".into())
        );
        assert_eq!(
            fault(None, "Sender not authorized"),
            OnvifError::LoginRejected("Sender not authorized".into()),
            "a camera that says it only in words"
        );
        assert_eq!(
            fault(
                Some("ter:ActionNotSupported"),
                "Optional Action Not Implemented"
            ),
            OnvifError::Fault("Optional Action Not Implemented".into())
        );

        let status = |status| {
            told(oxvif::OnvifError::Transport(TransportError::HttpStatus {
                status,
                body: String::new(),
            }))
        };
        assert!(matches!(status(401), OnvifError::LoginRejected(_)));
        assert!(matches!(status(404), OnvifError::Protocol(_)));
        assert!(matches!(
            told(oxvif::OnvifError::Soap(SoapError::MissingBody)),
            OnvifError::Protocol(_)
        ));
    }

    #[test]
    fn a_node_moves_when_it_has_a_velocity_space() {
        let space = |uri: &str| PtzSpaceRange {
            uri: uri.to_string(),
            x_range: (-1.0, 1.0),
            y_range: None,
        };
        assert!(at_a_speed(&[
            space("http://www.onvif.org/ver10/tptz/PanTiltSpaces/PositionGenericSpace"),
            space("http://www.onvif.org/ver10/tptz/PanTiltSpaces/VelocityGenericSpace"),
        ]));
        assert!(!at_a_speed(&[space(
            "http://www.onvif.org/ver10/tptz/ZoomSpaces/PositionGenericSpace"
        )]));
        assert!(!at_a_speed(&[]));
    }

    #[test]
    fn a_camera_that_answers_the_network_says_where_it_is_and_what_it_calls_itself() {
        let device = |addresses: &[&str], scopes: &[&str]| oxvif::discovery::DiscoveredDevice {
            endpoint: "urn:uuid:1".to_string(),
            types: vec!["dn:NetworkVideoTransmitter".to_string()],
            scopes: scopes.iter().map(|scope| scope.to_string()).collect(),
            xaddrs: addresses
                .iter()
                .map(|address| address.to_string())
                .collect(),
        };

        let found = found_of(&device(
            &[
                "http://[fe80::1]/onvif/device_service",
                "http://10.0.0.5:8000/onvif/device_service",
            ],
            &[
                "onvif://www.onvif.org/type/video_encoder",
                "onvif://www.onvif.org/Profile/Streaming",
                "onvif://www.onvif.org/name/Front%20Door",
                "onvif://www.onvif.org/hardware/IPC-123",
                "onvif://www.onvif.org/location/country/china",
                "onvif://www.onvif.org/MAC/aa:bb:cc:00:11:22",
                "onvif://www.onvif.org/extension/unique_identifier/1",
                "onvif://www.onvif.org/location/country/china",
                "http://elsewhere.example/serial/1",
            ],
        ))
        .unwrap();
        assert_eq!(
            found,
            FoundCamera {
                host: "10.0.0.5".into(),
                onvif_port: Some(8000),
                onvif_device_path: Some("/onvif/device_service".into()),
                rtsp_port: None,
                name: "Front Door".into(),
                model: "IPC-123".into(),
                details: vec!["MAC: aa:bb:cc:00:11:22".into()],
            },
            "the address other machines reach, before the link's own"
        );

        let plain = found_of(&device(&["http://10.0.0.6"], &[])).unwrap();
        assert_eq!(
            (plain.onvif_port, plain.onvif_device_path.as_deref()),
            (Some(80), Some("/"))
        );
        assert!(plain.name.is_empty() && plain.model.is_empty() && plain.details.is_empty());

        assert_eq!(
            service_of("http://[fe80::1]:8080/onvif"),
            Some(("fe80::1".into(), 8080, "/onvif".into()))
        );
        for unusable in [
            "http://0.0.0.0/onvif/device_service",
            "https://10.0.0.5/onvif/device_service",
            "http://10.0.0.5:port/onvif",
            "",
        ] {
            assert_eq!(found_of(&device(&[unusable], &[])), None, "{unusable}");
        }
    }

    #[test]
    fn found_cameras_are_in_the_order_of_their_addresses() {
        let mut hosts = ["10.0.0.10", "camera.local", "10.0.0.9", "10.0.1.2"];
        hosts.sort_by(|a, b| by_address(a).cmp(&by_address(b)));
        assert_eq!(hosts, ["10.0.0.9", "10.0.0.10", "10.0.1.2", "camera.local"]);
        assert_eq!(unescaped("Caf%C3%A9%2"), "Café%2");

        let asked: Vec<Ipv4Addr> = neighbors(Ipv4Addr::new(192, 168, 1, 20)).collect();
        assert_eq!(asked.len(), 253, "every address of its network but its own");
        assert_eq!(asked[0], Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(asked[252], Ipv4Addr::new(192, 168, 1, 254));
        assert!(!asked.contains(&Ipv4Addr::new(192, 168, 1, 20)));
    }

    #[test]
    fn a_picture_is_its_size_and_codec_as_far_as_the_camera_says() {
        assert_eq!(picture_of(3840, 2160, "H265"), "3840\u{d7}2160, H265");
        assert_eq!(picture_of(0, 0, "H264"), "H264");
        assert_eq!(picture_of(640, 360, ""), "640\u{d7}360");
    }
}
