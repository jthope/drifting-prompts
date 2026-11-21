//! Watching a camera for what it sees, and switching its spotlight, on a thread per camera. A
//! refused login ends the watch for good, since retrying could lock the account; dropping a watch
//! never waits for its thread.

use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    Brand, CameraActivity, CameraConfig, CameraEvent, CameraFeatures, DeviceInfo, EventRules,
    EventStep, EventTracker, QUIET_FOR_MS, Spotlight,
    api::WatchStatus,
    dahua_http::{DahuaHttp, DahuaHttpError, DahuaLight},
    onvif::{OnvifClient, OnvifError, OnvifEvents},
    reolink::{ReolinkClient, ReolinkError},
    sync::LockExt,
};

const POLL_EVERY: Duration = Duration::from_secs(1);

/// Each ask reads a Dahua's whole lighting table.
const LIGHT_POLL_EVERY: Duration = Duration::from_secs(5);

const RETRY_EVERY: Duration = Duration::from_secs(15);

/// What a watch has learned of its camera so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Watched {
    /// What the camera says it is, once it has said.
    pub device: Option<DeviceInfo>,
    /// What it can do, once it has said.
    pub features: Option<CameraFeatures>,
    /// What it saw at the last poll.
    pub activity: CameraActivity,
    /// What went wrong last, until a poll works again.
    pub problem: Option<String>,
    /// Whether the camera refused the login: nothing should log in again until its settings change.
    pub gave_up: bool,
}

impl Watched {
    /// What went wrong, or what the camera is and has: `RLC-811A: Motion, Person, Spotlight.`
    pub fn summary(&self) -> String {
        if let Some(problem) = &self.problem {
            return problem.clone();
        }
        let Some(features) = &self.features else {
            return "Asking the camera what it has\u{2026}".to_string();
        };
        let model = self
            .device
            .as_ref()
            .map(|device| device.model.as_str())
            .filter(|model| !model.is_empty())
            .unwrap_or("The camera");
        let has = features.titles();
        if has.is_empty() {
            format!("{model}: nothing to watch beyond its stream.")
        } else {
            format!("{model}: {}.", has.join(", "))
        }
    }

    /// As the server's API reports it.
    pub fn status(&self) -> WatchStatus {
        let spotlight = self.activity.spotlight;
        WatchStatus {
            motion: self.activity.motion,
            detected: self.activity.detected.clone(),
            spotlight: self
                .features
                .as_ref()
                .is_some_and(|features| features.spotlight),
            spotlight_on: spotlight.is_some_and(|spotlight| spotlight.on),
            summary: self.summary(),
        }
    }
}

/// An event as a watch saw it start, change or end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeenEvent {
    /// What this answer did to it.
    pub step: EventStep,
    /// The event as it is now.
    pub event: CameraEvent,
    /// What the camera sees now, as a file's tags; none as a sighting ends.
    pub tags: Vec<&'static str>,
    /// The camera's own JPEG as the event started, from a camera that hands one out.
    pub still: Option<Vec<u8>>,
}

enum Request {
    Spotlight(Spotlight, mpsc::Sender<Result<(), String>>),
}

/// One camera's watch. Dropping it ends the thread at its next poll, without waiting for it.
pub struct CameraWatch {
    requests: mpsc::Sender<Request>,
    watched: Arc<Mutex<Watched>>,
    events: Option<Arc<Mutex<Vec<SeenEvent>>>>,
}

/// A program that stops taking events loses the oldest rather than holding pictures without end.
const MOST_KEPT: usize = 64;

/// Whether the app can watch `camera`; it needs a password, as a login without one can only fail.
pub fn watches(camera: &CameraConfig) -> bool {
    !camera.password.is_empty()
        && (camera.brand == Brand::Reolink
            || (camera.brand == Brand::Dahua && camera.http_api)
            || camera.onvif_events)
}

impl CameraWatch {
    /// Starts watching `camera` on a thread of its own; `None` when [`watches`] says it can't be.
    pub fn start(camera: &CameraConfig) -> Option<Self> {
        Self::start_with(camera, None)
    }

    /// Starts watching `camera`, keeping its events, grouped by `rules`, for
    /// [`take_events`](Self::take_events).
    pub fn start_logging(camera: &CameraConfig, rules: EventRules) -> Option<Self> {
        Self::start_with(camera, Some(rules))
    }

    fn start_with(camera: &CameraConfig, logging: Option<EventRules>) -> Option<Self> {
        if !watches(camera) {
            return None;
        }
        let (requests, received) = mpsc::channel();
        let watched = Arc::new(Mutex::new(Watched::default()));
        let events = logging.map(|_| Arc::new(Mutex::new(Vec::new())));
        let camera = camera.clone();
        let shared = Shared {
            watched: watched.clone(),
            events: events.clone(),
            rules: logging.unwrap_or(EventRules::grouping(QUIET_FOR_MS)),
        };
        thread::Builder::new()
            .name("ipcamera-watch".into())
            .spawn(move || run(&camera, &received, &shared))
            .ok()?;
        Some(Self {
            requests,
            watched,
            events,
        })
    }

    /// What the watch has learned so far. Never waits on the camera.
    pub fn watched(&self) -> Watched {
        self.watched.locked().clone()
    }

    /// The events seen since this was last called, oldest first. Never waits on the camera.
    pub fn take_events(&self) -> Vec<SeenEvent> {
        self.events
            .as_ref()
            .map(|events| std::mem::take(&mut *events.locked()))
            .unwrap_or_default()
    }

    /// Asks the thread to switch the spotlight; the answer comes on the returned channel.
    pub fn set_spotlight(&self, spotlight: Spotlight) -> mpsc::Receiver<Result<(), String>> {
        let (answer, answered) = mpsc::channel();
        if self
            .requests
            .send(Request::Spotlight(spotlight, answer.clone()))
            .is_err()
        {
            let _ = answer.send(Err("The camera is no longer watched.".to_string()));
        }
        answered
    }

    /// Switches the spotlight on at its last brightness, or off; refused for a camera without one.
    pub fn switch_spotlight(&self, on: bool) -> Result<mpsc::Receiver<Result<(), String>>, String> {
        let watched = self.watched();
        if !watched
            .features
            .as_ref()
            .is_some_and(|features| features.spotlight)
        {
            return Err("This camera has no spotlight the app can switch.".to_string());
        }
        let brightness = watched
            .activity
            .spotlight
            .map(|spotlight| spotlight.brightness)
            .filter(|brightness| *brightness > 0)
            .unwrap_or(100);
        Ok(self.set_spotlight(Spotlight { on, brightness }))
    }
}

trait Watcher {
    fn learn(&mut self) -> (Option<DeviceInfo>, Option<CameraFeatures>);
    fn activity(&mut self) -> Result<CameraActivity, Failure>;
    fn set_spotlight(&mut self, spotlight: Spotlight) -> Result<(), String>;
    /// How long to wait for a request between polls: none when a poll waits on the camera itself.
    fn pause(&self) -> Duration;
    fn still(&mut self) -> Option<Vec<u8>> {
        None
    }
}

/// `Final` for a refused login, never retried.
enum Failure {
    Final(String),
    Retry(String),
}

impl From<ReolinkError> for Failure {
    fn from(error: ReolinkError) -> Self {
        match error {
            ReolinkError::LoginRejected(_) | ReolinkError::Locked => Self::Final(error.to_string()),
            _ => Self::Retry(error.to_string()),
        }
    }
}

impl From<DahuaHttpError> for Failure {
    fn from(error: DahuaHttpError) -> Self {
        match error {
            DahuaHttpError::LoginRejected(_) => Self::Final(error.to_string()),
            _ => Self::Retry(error.to_string()),
        }
    }
}

impl From<OnvifError> for Failure {
    fn from(error: OnvifError) -> Self {
        match error {
            OnvifError::LoginRejected(_) => Self::Final(error.to_string()),
            _ => Self::Retry(error.to_string()),
        }
    }
}

struct ReolinkWatcher {
    client: ReolinkClient,
    channel: u32,
    spotlight: bool,
}

impl Watcher for ReolinkWatcher {
    fn learn(&mut self) -> (Option<DeviceInfo>, Option<CameraFeatures>) {
        let features = self.client.features(self.channel).ok();
        self.spotlight = features.as_ref().is_some_and(|features| features.spotlight);
        (self.client.device_info().ok(), features)
    }

    fn activity(&mut self) -> Result<CameraActivity, Failure> {
        Ok(self.client.activity(self.channel, self.spotlight)?)
    }

    fn set_spotlight(&mut self, spotlight: Spotlight) -> Result<(), String> {
        self.client
            .set_spotlight(self.channel, spotlight)
            .map_err(|error| error.to_string())
    }

    fn pause(&self) -> Duration {
        POLL_EVERY
    }

    fn still(&mut self) -> Option<Vec<u8>> {
        self.client.snapshot(self.channel).ok()
    }
}

/// A Dahua's light over its HTTP API, and its motion over ONVIF's events when those are on.
struct DahuaWatcher {
    client: DahuaHttp,
    device: DeviceInfo,
    features: CameraFeatures,
    light: Option<DahuaLight>,
    events: Option<OnvifEvents>,
    lit: Option<Spotlight>,
    asked: Option<Instant>,
}

impl Watcher for DahuaWatcher {
    fn learn(&mut self) -> (Option<DeviceInfo>, Option<CameraFeatures>) {
        (Some(self.device.clone()), Some(self.features.clone()))
    }

    fn activity(&mut self) -> Result<CameraActivity, Failure> {
        let mut activity = match &mut self.events {
            Some(events) => events.activity()?,
            None => CameraActivity::default(),
        };
        if let Some(light) = &self.light {
            if self
                .asked
                .is_none_or(|asked| asked.elapsed() >= LIGHT_POLL_EVERY)
            {
                self.lit = Some(self.client.light_state(light)?);
                self.asked = Some(Instant::now());
            }
            activity.spotlight = self.lit;
        }
        Ok(activity)
    }

    fn set_spotlight(&mut self, spotlight: Spotlight) -> Result<(), String> {
        let Some(light) = &self.light else {
            return Err("This camera has no light the app can switch.".to_string());
        };
        self.client
            .set_light(light, spotlight.on)
            .map_err(|error| error.to_string())?;
        // The brightness stays the camera's.
        self.lit = Some(Spotlight {
            on: spotlight.on,
            brightness: self.lit.map_or(spotlight.brightness, |lit| lit.brightness),
        });
        Ok(())
    }

    fn pause(&self) -> Duration {
        match self.events {
            // A pull waits on the camera itself.
            Some(_) => Duration::ZERO,
            None => POLL_EVERY,
        }
    }
}

struct OnvifWatcher {
    link: Link,
    device: DeviceInfo,
    features: CameraFeatures,
    light: Option<Light>,
    /// As last switched: ONVIF can't ask.
    lit: bool,
}

enum Link {
    Events(OnvifEvents),
    Plain(OnvifClient),
}

pub(crate) struct Light {
    ptz: String,
    profile: String,
    on: String,
    off: String,
}

impl OnvifWatcher {
    fn client(&mut self) -> &mut OnvifClient {
        match &mut self.link {
            Link::Events(events) => events.client(),
            Link::Plain(client) => client,
        }
    }

    fn spotlight(&self) -> Option<Spotlight> {
        self.light.as_ref().map(|_| Spotlight {
            on: self.lit,
            brightness: 100,
        })
    }
}

impl Watcher for OnvifWatcher {
    fn learn(&mut self) -> (Option<DeviceInfo>, Option<CameraFeatures>) {
        (Some(self.device.clone()), Some(self.features.clone()))
    }

    fn activity(&mut self) -> Result<CameraActivity, Failure> {
        let mut activity = match &mut self.link {
            Link::Events(events) => events.activity()?,
            Link::Plain(_) => CameraActivity::default(),
        };
        activity.spotlight = self.spotlight();
        Ok(activity)
    }

    fn set_spotlight(&mut self, spotlight: Spotlight) -> Result<(), String> {
        let Some(light) = &self.light else {
            return Err("This camera has no light the app can switch.".to_string());
        };
        let (ptz, profile) = (light.ptz.clone(), light.profile.clone());
        let command = if spotlight.on {
            light.on.clone()
        } else {
            light.off.clone()
        };
        self.client()
            .auxiliary(&ptz, &profile, &command)
            .map_err(|error| error.to_string())?;
        self.lit = spotlight.on;
        Ok(())
    }

    fn pause(&self) -> Duration {
        match self.link {
            // A pull waits on the camera itself.
            Link::Events(_) => Duration::ZERO,
            Link::Plain(_) => POLL_EVERY,
        }
    }
}

pub(crate) struct OnvifCamera {
    pub(crate) client: OnvifClient,
    pub(crate) device: DeviceInfo,
    pub(crate) features: CameraFeatures,
    pub(crate) events: Option<String>,
    pub(crate) media: Option<String>,
    pub(crate) light: Option<Light>,
}

/// What an ONVIF camera is and has, with one login.
pub(crate) fn learn_onvif(camera: &CameraConfig) -> Result<OnvifCamera, OnvifError> {
    let (mut client, device) = OnvifClient::connect(
        &camera.host,
        camera.onvif_port,
        &camera.onvif_device_path,
        &camera.user,
        &camera.password,
    )?;
    let services = client.services()?;
    let mut features = CameraFeatures::default();
    if let Some(events) = &services.events {
        // A camera that won't list its topics still sends them.
        match client.features(events) {
            Ok(topics) => {
                features.motion = topics.motion;
                features.detections = topics.detections;
            }
            Err(_) => features.motion = true,
        }
    }
    let mut light = None;
    if let (Some(ptz), Some(media)) = (&services.ptz, &services.media)
        && let Ok(node) = client.ptz_node(ptz)
    {
        features.pan_tilt = node.pan_tilt;
        features.zoom = node.zoom;
        if let Some((on, off)) = node.light
            && let Ok(profile) = client.profile_token(media)
        {
            features.spotlight = true;
            light = Some(Light {
                ptz: ptz.clone(),
                profile,
                on,
                off,
            });
        }
    }
    Ok(OnvifCamera {
        client,
        device,
        features,
        events: services.events,
        media: services.media,
        light,
    })
}

/// `camera`'s ONVIF events, if it has them; a refused ONVIF login ends the watch, so it is never
/// tried again.
fn onvif_events(
    camera: &CameraConfig,
    features: &mut CameraFeatures,
) -> Result<Option<OnvifEvents>, Failure> {
    let onvif = match learn_onvif(camera) {
        Ok(onvif) => onvif,
        Err(error @ OnvifError::LoginRejected(_)) => return Err(error.into()),
        Err(_) => return Ok(None),
    };
    let Some(path) = onvif.events else {
        return Ok(None);
    };
    match OnvifEvents::subscribe(onvif.client, &path) {
        Ok(events) => {
            features.motion = onvif.features.motion;
            features.detections = onvif.features.detections;
            Ok(Some(events))
        }
        Err(failed) => match *failed {
            (error @ OnvifError::LoginRejected(_), _) => Err(error.into()),
            _ => Ok(None),
        },
    }
}

fn connect(camera: &CameraConfig) -> Result<Box<dyn Watcher>, Failure> {
    if camera.brand == Brand::Dahua && camera.http_api {
        let (mut client, device) = DahuaHttp::connect(
            &camera.host,
            camera.api_port,
            camera.api_https,
            &camera.user,
            &camera.password,
        )?;
        let mut features = CameraFeatures::default();
        if let Some(caps) = client.ptz_caps(camera.channel)? {
            features.pan_tilt = caps.pan_tilt;
            features.zoom = caps.zoom;
        }
        let light = client.light(camera.channel)?;
        features.spotlight = light.is_some();
        let events = if camera.onvif_events {
            onvif_events(camera, &mut features)?
        } else {
            None
        };
        return Ok(Box::new(DahuaWatcher {
            client,
            device,
            features,
            light,
            events,
            lit: None,
            asked: None,
        }));
    }
    if camera.brand == Brand::Reolink {
        let client = ReolinkClient::connect(
            &camera.host,
            camera.api_port,
            camera.api_https,
            &camera.user,
            &camera.password,
        )?;
        return Ok(Box::new(ReolinkWatcher {
            client,
            channel: camera.channel,
            spotlight: false,
        }));
    }
    let OnvifCamera {
        client,
        device,
        mut features,
        events,
        light,
        ..
    } = learn_onvif(camera)?;
    let link = match events {
        Some(path) => match OnvifEvents::subscribe(client, &path) {
            Ok(events) => Link::Events(events),
            // A refused login ends the watch; a camera that can't subscribe still has its light.
            Err(failed) => match *failed {
                (error @ (OnvifError::LoginRejected(_) | OnvifError::Unreachable(_)), _) => {
                    return Err(error.into());
                }
                (_, client) => {
                    features.motion = false;
                    features.detections.clear();
                    Link::Plain(client)
                }
            },
        },
        None => Link::Plain(client),
    };
    Ok(Box::new(OnvifWatcher {
        link,
        device,
        features,
        light,
        lit: false,
    }))
}

struct Shared {
    watched: Arc<Mutex<Watched>>,
    events: Option<Arc<Mutex<Vec<SeenEvent>>>>,
    rules: EventRules,
}

impl Shared {
    fn keep(&self, seen: SeenEvent) {
        if let Some(events) = &self.events {
            let mut kept = events.locked();
            if kept.len() >= MOST_KEPT {
                kept.remove(0);
            }
            kept.push(seen);
        }
    }

    fn end(&self, tracker: &mut EventTracker) {
        if let Some(event) = tracker.end() {
            self.keep(SeenEvent {
                step: EventStep::Closed,
                event,
                tags: Vec::new(),
                still: None,
            });
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

fn run(camera: &CameraConfig, requests: &mpsc::Receiver<Request>, shared: &Shared) {
    let watched = &*shared.watched;
    loop {
        let mut watcher = match connect(camera) {
            Ok(watcher) => watcher,
            Err(failure) => {
                if !stopped(watched, failure) || !wait(requests, RETRY_EVERY) {
                    return;
                }
                continue;
            }
        };

        let (device, features) = watcher.learn();
        let tells_apart = features
            .as_ref()
            .is_some_and(|features| !features.detections.is_empty());
        let mut tracker = EventTracker::new(camera.id.clone(), tells_apart, shared.rules);
        {
            let mut seen = watched.locked();
            seen.device = device;
            seen.features = features;
            seen.problem = None;
        }

        let failure = loop {
            match watcher.activity() {
                Ok(activity) => {
                    if let Some((step, event)) = tracker.see(&activity, now_ms())
                        && shared.events.is_some()
                    {
                        // Asked before the next poll, so it shows what started the event.
                        let still = (step == EventStep::Started)
                            .then(|| watcher.still())
                            .flatten();
                        let tags = tracker.tags(&activity);
                        shared.keep(SeenEvent {
                            step,
                            event,
                            tags,
                            still,
                        });
                    }
                    let mut seen = watched.locked();
                    seen.activity = activity;
                    seen.problem = None;
                }
                Err(failure) => {
                    shared.end(&mut tracker);
                    break failure;
                }
            }
            match requests.recv_timeout(watcher.pause()) {
                Ok(Request::Spotlight(wanted, answer)) => {
                    let done = watcher.set_spotlight(wanted);
                    if done.is_ok() {
                        watched.locked().activity.spotlight = Some(wanted);
                    }
                    let _ = answer.send(done);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                // The program ends its own events when it drops a watch.
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        };

        // Logs out before the wait, where the camera still answers.
        drop(watcher);
        if !stopped(watched, failure) || !wait(requests, RETRY_EVERY) {
            return;
        }
    }
}

/// Whether to try again: never after a refused login.
fn stopped(watched: &Mutex<Watched>, failure: Failure) -> bool {
    let mut seen = watched.locked();
    seen.activity = CameraActivity::default();
    match failure {
        Failure::Final(why) => {
            seen.problem = Some(why);
            seen.gave_up = true;
            false
        }
        Failure::Retry(why) => {
            seen.problem = Some(why);
            true
        }
    }
}

/// False when the watch was dropped meanwhile.
fn wait(requests: &mpsc::Receiver<Request>, how_long: Duration) -> bool {
    let until = std::time::Instant::now() + how_long;
    loop {
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return true;
        }
        match requests.recv_timeout(left) {
            Ok(Request::Spotlight(_, answer)) => {
                let _ = answer.send(Err(
                    "The camera isn't answering. Check that it is on the network.".to_string(),
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => return true,
            Err(mpsc::RecvTimeoutError::Disconnected) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reolink_a_dahua_over_its_api_or_onvif_with_a_password_is_watched() {
        let reolink = CameraConfig {
            brand: Brand::Reolink,
            password: "s3cret".into(),
            ..CameraConfig::default()
        };
        assert!(watches(&reolink));
        assert!(!watches(&CameraConfig {
            password: String::new(),
            ..reolink.clone()
        }));
        let dahua = CameraConfig {
            brand: Brand::Dahua,
            ..reolink
        };
        assert!(!watches(&dahua), "not over DVRIP without ONVIF events on");
        assert!(watches(&CameraConfig {
            onvif_events: true,
            ..dahua.clone()
        }));
        assert!(watches(&CameraConfig {
            http_api: true,
            ..dahua.clone()
        }));
        assert!(!watches(&CameraConfig {
            brand: Brand::Other,
            http_api: true,
            ..dahua
        }));
        assert!(CameraWatch::start(&CameraConfig::default()).is_none());
    }
}
