//! ONVIF, the standard most IP cameras speak beside their own APIs: what a camera is, which events
//! it can send, and motion and detections from its event service.
//!
//! ONVIF is SOAP: an XML envelope POSTed to a service's URL. The protocol is the `oxvif` crate's:
//! the envelopes, both logins, and reading the replies. This module is what the app asks of a
//! camera through it, and the app's own rules about the answers:
//!
//! - **The device service** (`onvif_device_path` on `onvif_port`): the camera's time, which
//!   answers without a login; what it is (make, model, firmware); and where its event, media and
//!   PTZ services are ([`Services`]).
//! - **The media services**: the profiles, for the one a PTZ move names and the streams the
//!   camera lists. The second media service is asked what each picture is, since the first can't
//!   name every codec.
//! - **The PTZ service**: whether it pans and tilts, zooms, and the auxiliary commands it takes,
//!   among which a light bulb camera's light; moves, a stop, and the command that switches that
//!   light (`Light|On`, `Light|Off`).
//! - **The event service**: the topics the camera can send, which say whether it reports motion
//!   and what it tells apart; a pull-point subscription, pulled in a loop and renewed before it
//!   runs out.
//!
//! **Calls block.** `oxvif` is async, and the app asks each camera on a thread of its own, so
//! every [`OnvifClient`] has a small tokio runtime that its calls run to their end on. A client
//! is made, used and dropped off any async runtime's own threads.
//!
//! **The login** is WS-Security's digest, with the camera's time, which the client asks first
//! (without a login) and keeps the difference to; and HTTP's digest for a camera that asks for
//! it. A camera that refuses (`NotAuthorized`, or HTTP 401) is refused for good: the client
//! remembers it and sends nothing more, since a failed ONVIF login counts against the account
//! like any other. `oxvif` never repeats a call by itself.
//!
//! **Events** say what changed: a topic (`tns1:RuleEngine/CellMotionDetector/Motion`,
//! `tns1:VideoSource/MotionAlarm`, `.../MyRuleDetector/PeopleDetect`) and a state (`IsMotion`,
//! `State`: true or false). Most cameras send each state once as the subscription starts, then on
//! every change, so [`OnvifEvents`] keeps the last state of each topic. Topics are told apart by
//! words in their names, which vendors share: motion, people or human, vehicle, dog/cat or
//! animal, face. A recorder's channels all count as one camera.
//!
//! **Finding cameras** ([`discover`]) is WS-Discovery: one question to every device on the
//! network at once, which a camera with ONVIF on answers with where its device service is, its
//! name and its model. Wi-Fi often drops a question sent to everyone, so the same question also
//! goes to each address of this machine's private networks by itself. It logs in to nothing, and
//! reaches only this machine's own networks.
//!
//! Many cameras give their services' addresses with an IP they don't answer on (behind NAT, or
//! 0.0.0.0), so only the path of an address the camera gives is used, on the address and port the
//! camera's settings name.

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

/// How long [`OnvifClient::answers`] waits. Every other call has `oxvif`'s ten seconds, and a
/// pull waits up to [`PULL_WAIT`] on the camera's side, well inside them.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// How long the camera may hold a pull open waiting for an event: a motion badge that lags by a
/// second at most, as Reolink's polling.
const PULL_WAIT: &str = "PT1S";

/// How long a subscription lasts unless renewed, and how often it is renewed.
const SUBSCRIPTION: &str = "PT60S";
const RENEW_EVERY: Duration = Duration::from_secs(30);

/// How often [`discover`] asks the network, how long it listens each time, and the pause between.
const ROUNDS: usize = 2;
const ROUND_WAIT: Duration = Duration::from_millis(1500);
const ROUND_PAUSE: Duration = Duration::from_millis(300);

/// How long [`discover`] waits for an address it asked by itself to answer, and how many of this
/// machine's networks it asks address by address: its own few, not every virtual one.
const SWEEP_WAIT: Duration = Duration::from_secs(2);
const SWEEP_NETWORKS: usize = 4;

/// Why a call to a camera's ONVIF services failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OnvifError {
    /// The camera refused the login, for the reason given. Nothing retries it.
    LoginRejected(String),
    /// The camera could not be reached, or the connection broke, for the reason given.
    Unreachable(String),
    /// The camera answered with a SOAP fault, for the reason it gave: it may not have the service.
    Fault(String),
    /// The camera answered with something that isn't ONVIF, for the reason given.
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

/// What an ONVIF call returns.
type Result<T> = std::result::Result<T, OnvifError>;

/// What `oxvif` returns.
type Asked<T> = std::result::Result<T, oxvif::OnvifError>;

/// Where a camera's services are, as paths on its address. A service it doesn't have is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Services {
    /// The event service, for motion and detections.
    pub events: Option<String>,
    /// The media service, for the profile a PTZ move names.
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
    /// The auxiliary commands that switch its light on and off, for a camera with one: a light
    /// bulb camera's bulb, not an infrared lamp.
    pub light: Option<(String, String)>,
}

/// A logged-in connection to a camera's ONVIF services.
pub struct OnvifClient {
    /// The client, which holds the login. Before the runtime, so it is dropped first.
    client: oxvif::OnvifClient,
    /// What the client's calls run on, on the thread that makes them.
    runtime: Runtime,
    /// `http://host:port`, where every service is asked.
    base: String,
    /// Why the camera refused the login, after which nothing is sent.
    refused: Option<OnvifError>,
}

impl OnvifClient {
    /// Whether an ONVIF device service answers at `host` on `port` and `path`. It asks the
    /// camera's time, which needs no login: never a login, so never a failed one.
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

    /// Asks the camera's time (no login), then logs in once by asking what it is. The device
    /// information comes back with the client.
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
        // Asked by a client with no login to send. A camera that doesn't say its time is asked
        // with this machine's; one that isn't there is not asked again.
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

    /// The token of the media profile a PTZ move names: the first with a PTZ configuration, else
    /// the first.
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

    /// What the camera's second media service says of each profile's picture, by the profile's
    /// token; empty for a camera without that service, or that won't say. The first media
    /// service can only name JPEG, MPEG-4 and H.264, so a camera sending H.265 calls it H.264
    /// there, and only says what it is here.
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

    /// The streams the camera's media service lists: each profile's name, what it says of its
    /// picture, and the RTSP address the camera gives for it. A profile the camera gives no
    /// address for is left out. Where the camera has the second media service, what that says
    /// of a profile's picture is taken over the first's, which can't name every codec
    /// ([`pictures`](Self::pictures)).
    pub fn streams(&mut self, media_path: &str) -> Result<Vec<CameraStream>> {
        let url = self.url(media_path);
        let profiles = self.ask(async |client| client.get_profiles(&url).await)?;
        // A camera that won't say what its encoders send still lists its streams.
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
                // The login or the connection: the rest would fail the same way.
                Err(error @ (OnvifError::LoginRejected(_) | OnvifError::Unreachable(_))) => {
                    return Err(error);
                }
                // A profile with no stream of its own.
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

    /// Starts moving at `velocity` (each part -1 to 1), as `profile` names the PTZ to move; a move
    /// lasts until [`stop`](Self::stop). Zoom wins over pan and tilt, as everywhere in the app.
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

    /// Sends an auxiliary command (`Light|On`), as the PTZ service of `profile` lists them.
    pub fn auxiliary(&mut self, ptz_path: &str, profile: &str, command: &str) -> Result<()> {
        let url = self.url(ptz_path);
        self.ask(async |client| {
            let sent = client
                .ptz_send_auxiliary_command(&url, profile, command)
                .await;
            match sent {
                // The reply's text is optional, and a camera that leaves it out took the command.
                Ok(_) | Err(oxvif::OnvifError::Soap(SoapError::MissingField(_))) => Ok(()),
                Err(error) => Err(error),
            }
        })
    }

    /// What the camera can tell: motion, and what it tells apart, from the topics it can send.
    pub fn features(&mut self, events_path: &str) -> Result<CameraFeatures> {
        let url = self.url(events_path);
        let properties = self.ask(async |client| client.get_event_properties(&url).await)?;
        Ok(features_of(&properties.topics))
    }

    /// The address of `path` on the camera.
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, with_slash(path))
    }

    /// One call with the login, run to its end on this thread. A refused login is remembered, so
    /// nothing is sent after it.
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

/// A camera's events, pulled from a subscription of its own, with the last state of each topic.
/// Dropping it ends the subscription.
pub struct OnvifEvents {
    client: OnvifClient,
    /// The subscription's address: the path the camera gave, on the camera's own address.
    subscription: String,
    renew_at: Instant,
    /// The last state of each topic that is about motion or a detection.
    states: HashMap<String, (Kind, bool)>,
}

impl OnvifEvents {
    /// Subscribes to the events of the camera `client` is logged in to, at `events_path`. When it
    /// can't, the client comes back with why, for whatever else it can do (boxed: a client is
    /// large to carry in an error).
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

    /// The client the events come through, for the camera's other services.
    pub fn client(&mut self) -> &mut OnvifClient {
        &mut self.client
    }

    /// Waits up to a second for events, and returns what the camera sees after them. Renews the
    /// subscription when it is due.
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
    /// Ends the subscription, so the camera's few don't run out; the camera ends it by itself
    /// within a minute anyway, so a failure changes nothing. It can wait for up to ten seconds.
    fn drop(&mut self) {
        let url = &self.subscription;
        let _ = self
            .client
            .ask(async |client| client.unsubscribe(url).await);
    }
}

/// Asks the network which ONVIF cameras are on it (WS-Discovery: a question sent to every device
/// at once, over UDP multicast, which each camera answers with where its device service is), and
/// returns those that answered, by address. No camera is logged in to. It asks [`ROUNDS`] times,
/// since a camera can miss one, and meanwhile asks each address of this machine's networks by
/// itself ([`sweep`]), for the cameras a question to everyone doesn't reach. It takes about three
/// seconds; a network that can't be asked finds none.
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

/// Asks each address of this machine's networks the same question by itself, all at once, and
/// returns the devices that answered. Wi-Fi access points and some switches don't pass a question
/// sent to everyone on to the cameras behind them, and a camera asked by its own address answers
/// all the same. Only private networks are asked (a home's or an office's own), and each as the
/// 254 addresses that share this machine's first three numbers, which is what such networks are.
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

/// This machine's addresses on private networks (a home's or an office's own), the first
/// [`SWEEP_NETWORKS`] of them: the networks a search asks address by address.
pub(crate) fn private_networks() -> Vec<Ipv4Addr> {
    oxvif::discovery::discovery_interfaces()
        .unwrap_or_default()
        .into_iter()
        .map(|interface| interface.address)
        .filter(Ipv4Addr::is_private)
        .take(SWEEP_NETWORKS)
        .collect()
}

/// The other addresses that share `own`'s first three numbers.
pub(crate) fn neighbors(own: Ipv4Addr) -> impl Iterator<Item = Ipv4Addr> {
    let [a, b, c, mine] = own.octets();
    (1..=254)
        .filter(move |last| *last != mine)
        .map(move |last| Ipv4Addr::new(a, b, c, last))
}

/// What the camera form needs of a device that answered: the first of its addresses the app can
/// ask (HTTP, on an address other machines reach), and the name and model among its scopes.
/// `None` for a device that gave no such address.
fn found_of(device: &oxvif::discovery::DiscoveredDevice) -> Option<FoundCamera> {
    let (host, onvif_port, onvif_device_path) = device
        .xaddrs
        .iter()
        .filter_map(|address| service_of(address))
        // An IPv4 address before an IPv6 one, which is often the link's own.
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

/// The host, port and path of a device service's `address` (`http://10.0.0.5:8000/onvif/x`);
/// `None` for one that isn't HTTP, or names no machine (`0.0.0.0`).
fn service_of(address: &str) -> Option<(String, u16, String)> {
    let rest = address.trim().strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.strip_prefix('[') {
        // An IPv6 address in its brackets.
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

/// What a device's `scopes` say under `kind`: `Front Door` from
/// `onvif://www.onvif.org/name/Front%20Door`. Empty when they say nothing.
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

/// Whatever else a device's `scopes` say of it, each as `Kind: what it says`: what a maker adds
/// (a serial number, a MAC address). Its name and model have fields of their own, and the rest
/// of what ONVIF itself defines says nothing a person choosing a camera reads: its type, the
/// profiles it keeps to, its location, which cameras leave as the country that made them, and
/// its extensions, which are for other programs.
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
        // `serial` as `Serial`; a kind in capitals (`MAC`) stays as it is.
        let mut label = kind.to_string();
        label[..1].make_ascii_uppercase();
        let detail = format!("{label}: {value}");
        if !details.contains(&detail) {
            details.push(detail);
        }
    }
    details
}

/// `text` with its `%20`-style escapes read, as a scope writes a space or a letter outside ASCII.
fn unescaped(text: &str) -> String {
    percent_encoding::percent_decode_str(text)
        .decode_utf8_lossy()
        .into_owned()
}

/// What addresses sort by: an IPv4 address by its numbers, so `.9` comes before `.10`, and any
/// other after those, by its text.
fn by_address(host: &str) -> (bool, [u8; 4], &str) {
    match host.parse::<Ipv4Addr>() {
        Ok(address) => (false, address.octets(), host),
        Err(_) => (true, [0; 4], host),
    }
}

/// A runtime for one client's calls, which run on the thread that makes them.
fn runtime() -> Result<Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| OnvifError::Protocol(format!("ONVIF couldn't be started: {error}.")))
}

/// Why a call failed, in this app's terms: a refused login apart from everything else, since it
/// alone is never tried again.
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
        // Without the address, which the settings already show.
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

/// Whether a part of a SOAP fault says the login was refused: ONVIF's `ter:NotAuthorized`,
/// WS-Security's `wsse:FailedAuthentication`, or the same in words.
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

/// Whether one of a PTZ node's `spaces` moves at a speed, which a continuous move needs: its
/// velocity spaces, told from its position and translation spaces by their addresses.
fn at_a_speed(spaces: &[PtzSpaceRange]) -> bool {
    spaces
        .iter()
        .any(|space| space.uri.to_ascii_lowercase().contains("velocity"))
}

/// What a video encoder says of its picture, its size and codec: `3840×2160, H265`. A size of
/// nothing is one the camera didn't say.
fn picture_of(width: u32, height: u32, encoding: &str) -> String {
    let size = (width > 0 && height > 0).then(|| format!("{width}\u{d7}{height}"));
    let encoding = Some(encoding.trim().to_string()).filter(|encoding| !encoding.is_empty());
    let parts: Vec<String> = size.into_iter().chain(encoding).collect();
    parts.join(", ")
}

/// The auxiliary commands among `commands` that switch a light on and off: one named for a light
/// or a lamp, but not an infrared one. `Light|On` and `Light|Off` when the camera lists them so,
/// or a bare `Light` with `|On` and `|Off` put after it.
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

/// What an event's topic is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Motion,
    Detected(Detection),
}

/// What a topic, or a topic's path in the topic set, is about, by the words in its parts; `None`
/// for a topic about something else (tampering, an input). A part is matched by its start where a
/// word could hide inside another (`Face` but not `Interface`, `Pet` but not `Competition`).
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

/// What the camera sees, from the last state of each topic.
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

/// What a pulled notification says, when it is about motion or a detection: its topic with its
/// source, what it is about, and its state. A topic and its source together name the state, so
/// a recorder's channels each keep their own. The state is the item most cameras name it by,
/// else the first (by name) that is true or false.
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

/// What a camera can tell, from the topics its event service lists.
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

/// The path (and query) of `address`, which the camera gives whole: `/onvif/event_service`.
fn path_of(address: &str) -> String {
    let after_scheme = address.split_once("://").map_or(address, |(_, rest)| rest);
    match after_scheme.find('/') {
        Some(slash) => after_scheme[slash..].to_string(),
        None => "/".to_string(),
    }
}

/// `http://host:port`, with an IPv6 address in its brackets.
fn base_of(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("http://[{host}]:{port}")
    } else {
        format!("http://{host}:{port}")
    }
}

/// `path` with a leading slash.
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

    /// A notification of `topic` from `source`, with `data`'s items.
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
