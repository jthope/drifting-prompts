//! Camera PTZ: a velocity contract, each brand's transport, and a worker per camera that sends only
//! the newest command, so a Stop never waits behind queued Moves while the camera turns. A click
//! let go before it was sent still goes out, with its Stop straight after.

#[cfg(not(target_arch = "wasm32"))]
mod dahua_http;
mod dvrip;
#[cfg(not(target_arch = "wasm32"))]
mod onvif;
#[cfg(not(target_arch = "wasm32"))]
mod reolink;

#[cfg(not(target_arch = "wasm32"))]
pub use dahua_http::DahuaHttpPtz;
pub use dvrip::{DvripClient, DvripError};
#[cfg(not(target_arch = "wasm32"))]
pub use onvif::OnvifPtz;
#[cfg(not(target_arch = "wasm32"))]
pub use reolink::ReolinkPtz;

use std::{
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use crate::{camera_config::CameraConfig, sync::LockExt};

const DEAD_ZONE: f64 = 0.01;

fn moves(axis: f64) -> bool {
    axis.abs() > DEAD_ZONE
}

/// Direction and speed, each -1..1. Positive pan is right, tilt is up, zoom is in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PtzVelocity {
    /// Left (negative) or right (positive).
    pub pan: f64,
    /// Down (negative) or up (positive).
    pub tilt: f64,
    /// Out (negative) or in (positive).
    pub zoom: f64,
}

impl PtzVelocity {
    /// A velocity from its three parts, each -1..1.
    pub const fn new(pan: f64, tilt: f64, zoom: f64) -> Self {
        Self { pan, tilt, zoom }
    }

    /// Whether every part is too small to move the camera.
    pub fn is_still(self) -> bool {
        !moves(self.pan) && !moves(self.tilt) && !moves(self.zoom)
    }
}

/// A move in Dahua's own terms. The CGI and DVRIP APIs take the same values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DahuaCommand {
    /// The direction, as Dahua names it: `Up`, `LeftDown`, `ZoomTele`, ...
    pub code: &'static str,
    /// The vertical speed of a diagonal move, 1..8; 0 for any other move.
    pub arg1: i32,
    /// The speed, 1..8: a straight move's or a zoom's, or a diagonal's horizontal one.
    pub arg2: i32,
}

impl DahuaCommand {
    /// `velocity` as a command; `None` when it is no move at all.
    pub fn from_velocity(velocity: PtzVelocity) -> Option<Self> {
        let code = direction_code(velocity)?;

        if moves(velocity.zoom) {
            return Some(Self {
                code,
                arg1: 0,
                arg2: speed(velocity.zoom),
            });
        }

        let horizontal = speed(velocity.pan);
        let vertical = speed(velocity.tilt);
        let diagonal = moves(velocity.pan) && moves(velocity.tilt);

        Some(if diagonal {
            Self {
                code,
                arg1: vertical,
                arg2: horizontal,
            }
        } else {
            Self {
                code,
                arg1: 0,
                arg2: if moves(velocity.pan) {
                    horizontal
                } else {
                    vertical
                },
            }
        })
    }
}

/// What every transport's status says when nothing answers where PTZ goes, `over` naming it.
pub(crate) fn unreachable(over: &str, port: u16) -> String {
    format!(
        "Can't move this camera: nothing answers for PTZ on its {over} port ({port}). Detect, in \
         its settings, finds how it is controlled, or says it has no control the app can reach."
    )
}

/// Whether nothing took the connection, rather than one broke.
fn refused_connection(error: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    matches!(
        error.kind(),
        ErrorKind::ConnectionRefused
            | ErrorKind::TimedOut
            | ErrorKind::HostUnreachable
            | ErrorKind::NetworkUnreachable
            | ErrorKind::AddrNotAvailable
    )
}

fn speed(value: f64) -> i32 {
    ((value.abs() * 8.0).round() as i32).clamp(1, 8)
}

/// The direction `velocity` asks for, in Dahua's words; zoom wins over pan and tilt.
pub(crate) fn direction_code(velocity: PtzVelocity) -> Option<&'static str> {
    if moves(velocity.zoom) {
        return Some(if velocity.zoom > 0.0 {
            "ZoomTele"
        } else {
            "ZoomWide"
        });
    }

    let left = moves(velocity.pan) && velocity.pan < 0.0;
    let right = moves(velocity.pan) && velocity.pan > 0.0;
    let up = moves(velocity.tilt) && velocity.tilt > 0.0;
    let down = moves(velocity.tilt) && velocity.tilt < 0.0;

    Some(match (left, right, up, down) {
        (true, _, true, _) => "LeftUp",
        (_, true, true, _) => "RightUp",
        (true, _, _, true) => "LeftDown",
        (_, true, _, true) => "RightDown",
        (true, _, _, _) => "Left",
        (_, true, _, _) => "Right",
        (_, _, true, _) => "Up",
        (_, _, _, true) => "Down",
        _ => return None,
    })
}

/// Well inside the shortest session seen, so a press and release normally share one session.
const SESSION_LIFETIME: Duration = Duration::from_secs(30);

struct LiveSession {
    client: DvripClient,
    ptz_object: i64,
    opened: Instant,
}

/// PTZ through Dahua's DVRIP protocol, normally on port 37777.
pub struct DvripPtz {
    host: String,
    port: u16,
    user: String,
    password: String,
    channel: u32,
    session: Option<LiveSession>,
    last_command: Option<DahuaCommand>,
    diagonals: bool,
    login_rejected: bool,
    status: String,
}

impl DvripPtz {
    /// A client for `camera`'s DVRIP port, connecting on the first probe or move.
    pub fn new(camera: &CameraConfig) -> Self {
        Self {
            host: camera.host.clone(),
            port: camera.dvrip_port,
            user: camera.user.clone(),
            password: camera.password.clone(),
            channel: camera.channel,
            session: None,
            last_command: None,
            diagonals: false,
            login_rejected: false,
            status: "Dahua DVRIP: not probed.".to_string(),
        }
    }

    /// What happened last, as a sentence.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Logs in and checks that the camera reports something to drive.
    pub fn probe(&mut self) -> bool {
        // A login with no password can only fail, and failures count towards locking the account.
        if self.password.is_empty() {
            self.status = "Dahua DVRIP: skipped because no password is set".to_string();
            return false;
        }

        let Some(session) = self.session(false) else {
            return false;
        };

        let reply =
            match session
                .client
                .call("ptz.getCurrentProtocolCaps", None, Some(session.ptz_object))
            {
                Ok(reply) => reply,
                Err(error) => {
                    self.status =
                        format!("Dahua DVRIP: no response on port {} ({error})", self.port);
                    self.session = None;
                    return false;
                }
            };

        let caps = &reply["params"]["caps"];
        // "Tile" is how the firmware spells tilt.
        if !is_true(&caps["Pan"]) && !is_true(&caps["Tile"]) && !is_true(&caps["Zoom"]) {
            self.status = format!("Dahua DVRIP: no PTZ reported on port {}", self.port);
            return false;
        }

        self.diagonals = !caps["UnSupportDirections"]
            .as_array()
            .is_some_and(|directions| !directions.is_empty());
        self.status = format!("Dahua DVRIP ready on port {}", self.port);
        true
    }

    /// Starts moving at `velocity`; whether the camera took the command.
    pub fn move_by(&mut self, velocity: PtzVelocity) -> bool {
        let Some(command) = DahuaCommand::from_velocity(self.straighten(velocity)) else {
            return true;
        };

        self.last_command = Some(command);
        self.send("ptz.start", command, true)
    }

    /// Stops on the session that started the move, when that session is still alive.
    pub fn stop(&mut self) -> bool {
        match self.last_command {
            Some(command) => self.send("ptz.stop", command, false),
            None => true,
        }
    }

    /// Stops, retrying: a lost Stop leaves the camera moving.
    pub fn stop_with_retries(&mut self) -> bool {
        for attempt in 1..=3 {
            if self.stop() {
                return true;
            }
            if attempt < 3 {
                thread::sleep(Duration::from_millis(200));
            }
        }
        false
    }

    /// Cameras that cannot go diagonally get the larger of the two axes, pan on a tie.
    fn straighten(&self, velocity: PtzVelocity) -> PtzVelocity {
        if self.diagonals || velocity.pan == 0.0 || velocity.tilt == 0.0 {
            return velocity;
        }
        if velocity.tilt.abs() > velocity.pan.abs() {
            PtzVelocity {
                pan: 0.0,
                ..velocity
            }
        } else {
            PtzVelocity {
                tilt: 0.0,
                ..velocity
            }
        }
    }

    fn send(&mut self, method: &str, command: DahuaCommand, fresh: bool) -> bool {
        let parameters = json!({
            "code": command.code,
            "arg1": command.arg1,
            "arg2": command.arg2,
            "arg3": 0,
        });

        // A dead session only shows itself when used, so one retry on a fresh session is routine.
        for attempt in 1..=2 {
            let Some(session) = self.session(fresh) else {
                return false;
            };
            let ptz_object = session.ptz_object;

            match session
                .client
                .call(method, Some(parameters.clone()), Some(ptz_object))
            {
                Ok(reply) => return is_true(&reply["result"]),
                Err(_) if attempt == 1 => {
                    self.session = None;
                }
                Err(_) => {
                    self.session = None;
                    return false;
                }
            }
        }

        false
    }

    /// The session to use, opening one when needed; `fresh` replaces an old one rather than risk it
    /// dying mid-move. A refused login is never tried again.
    fn session(&mut self, fresh: bool) -> Option<&mut LiveSession> {
        if self.login_rejected {
            return None;
        }

        if let Some(session) = self.session.as_ref() {
            if !fresh || session.opened.elapsed() < SESSION_LIFETIME {
                return self.session.as_mut();
            }
            self.session = None;
        }

        match self.open() {
            Ok(session) => self.session = Some(session),
            Err(error @ DvripError::LoginRejected(_)) => {
                self.login_rejected = true;
                self.status = format!("Dahua DVRIP: login refused ({error})");
            }
            Err(DvripError::Io(error)) if refused_connection(&error) => {
                self.status = unreachable("DVRIP", self.port);
            }
            Err(error) => {
                self.status = format!("Dahua DVRIP: no response on port {} ({error})", self.port);
            }
        }
        self.session.as_mut()
    }

    fn open(&self) -> Result<LiveSession, DvripError> {
        let mut client = DvripClient::connect(&self.host, self.port, &self.user, &self.password)?;

        // The API counts channels from 0; the camera's settings, from 1.
        let api_channel = self.channel.max(1) - 1;
        let reply = client.call(
            "ptz.factory.instance",
            Some(json!({ "channel": api_channel })),
            None,
        )?;

        let ptz_object = reply["result"].as_i64().unwrap_or(0);
        if ptz_object == 0 {
            return Err(DvripError::Protocol(format!(
                "the camera has no PTZ on channel {}",
                self.channel
            )));
        }

        Ok(LiveSession {
            client,
            ptz_object,
            opened: Instant::now(),
        })
    }
}

fn is_true(value: &Value) -> bool {
    value.as_bool() == Some(true)
}

trait Driver: Send + 'static {
    fn probe(&mut self) -> bool;
    fn status(&self) -> &str;
    fn move_by(&mut self, velocity: PtzVelocity) -> bool;
    fn stop_with_retries(&mut self) -> bool;
    /// How often a held move is sent again, for a camera that moves a step per command.
    fn repeat_every(&self) -> Option<Duration> {
        None
    }
}

impl Driver for DvripPtz {
    fn probe(&mut self) -> bool {
        DvripPtz::probe(self)
    }

    fn status(&self) -> &str {
        DvripPtz::status(self)
    }

    fn move_by(&mut self, velocity: PtzVelocity) -> bool {
        DvripPtz::move_by(self, velocity)
    }

    fn stop_with_retries(&mut self) -> bool {
        DvripPtz::stop_with_retries(self)
    }
}

enum Command {
    Move(PtzVelocity),
    Stop,
}

/// Owns the PTZ worker and its connection. Dropping it stops the camera first.
pub struct PtzSession {
    tx: mpsc::Sender<Command>,
    status: Arc<Mutex<String>>,
    worker: Option<JoinHandle<()>>,
}

impl PtzSession {
    /// Starts probing in the background over `camera`'s brand's transport; returns at once.
    pub fn connect(camera: &CameraConfig) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        match camera.brand {
            crate::Brand::Reolink => return Self::start(ReolinkPtz::new(camera)),
            crate::Brand::Other => return Self::start(OnvifPtz::new(camera)),
            crate::Brand::Dahua if camera.http_api => {
                return Self::start(DahuaHttpPtz::new(camera));
            }
            crate::Brand::Dahua => {}
        }
        Self::start(DvripPtz::new(camera))
    }

    fn start(ptz: impl Driver) -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new("PTZ: connecting\u{2026}".to_string()));

        let worker = {
            let status = status.clone();
            thread::Builder::new()
                .name("ipcamera-ptz".into())
                .spawn(move || run(ptz, rx, status))
                .ok()
        };

        Self { tx, status, worker }
    }

    /// Queues a move at `velocity`, or a stop when it is still; a newer command replaces it.
    pub fn move_by(&self, velocity: PtzVelocity) {
        if velocity.is_still() {
            self.stop();
        } else {
            let _ = self.tx.send(Command::Move(velocity));
        }
    }

    /// Queues a stop, sent in place of any move still queued before it.
    pub fn stop(&self) {
        let _ = self.tx.send(Command::Stop);
    }

    /// What the worker reported last, as a sentence.
    pub fn status(&self) -> String {
        self.status.locked().clone()
    }
}

impl Drop for PtzSession {
    /// Waits for the worker, which stops the camera on its way out.
    fn drop(&mut self) {
        // The worker waits on the queue until every sender is gone, so this sender must go before
        // the join, or the two wait on each other for good.
        let (closed, _) = mpsc::channel();
        drop(std::mem::replace(&mut self.tx, closed));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(mut ptz: impl Driver, rx: mpsc::Receiver<Command>, status: Arc<Mutex<String>>) {
    if !ptz.probe() {
        set_status(&status, ptz.status());
        return;
    }
    let ready = ptz.status().to_string();
    set_status(&status, &ready);

    let mut moving = false;
    let mut held: Option<PtzVelocity> = None;
    loop {
        let first = match held.zip(ptz.repeat_every()) {
            Some((velocity, every)) => match rx.recv_timeout(every) {
                Ok(first) => first,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    start(&mut ptz, velocity, &ready, &status);
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(first) => first,
                Err(_) => break,
            },
        };
        // A closed queue sends only the stop below.
        let Some(next) = newest(first, &rx, moving) else {
            break;
        };
        if let Some(velocity) = next.click {
            start(&mut ptz, velocity, &ready, &status);
        }
        match next.command {
            // The held move sent again: its steps are already being repeated.
            Command::Move(velocity) if held == Some(velocity) && ptz.repeat_every().is_some() => {}
            Command::Move(velocity) => {
                start(&mut ptz, velocity, &ready, &status);
                moving = true;
                held = Some(velocity);
            }
            Command::Stop => {
                held = None;
                if !ptz.stop_with_retries() {
                    set_status(
                        &status,
                        "PTZ: stop failed, so the camera may still be moving.",
                    );
                }
                moving = false;
            }
        }
    }

    // Never leave the camera moving.
    let _ = ptz.stop_with_retries();
}

fn start(ptz: &mut impl Driver, velocity: PtzVelocity, ready: &str, status: &Arc<Mutex<String>>) {
    if ptz.move_by(velocity) {
        return;
    }
    if ptz.status() == ready {
        set_status(status, "PTZ: the camera did not accept the move.");
    } else {
        set_status(status, ptz.status());
    }
}

struct Next {
    /// A press let go before it was sent, while the camera was at rest: sent before its stop.
    click: Option<PtzVelocity>,
    command: Command,
}

/// The newest queued command, with a click's move; `None` once the queue has closed.
fn newest(first: Command, rx: &mpsc::Receiver<Command>, moving: bool) -> Option<Next> {
    // A move queued while the camera turns is a held button's repeat, which its stop replaces;
    // one that would have started the camera is a press.
    let mut at_rest = !moving;
    let mut pressed = None;
    let mut newest = first;
    loop {
        match newest {
            Command::Stop => at_rest = true,
            Command::Move(velocity) if at_rest => pressed = Some(velocity),
            Command::Move(_) => {}
        }
        match rx.try_recv() {
            Ok(next) => newest = next,
            Err(mpsc::TryRecvError::Empty) => {
                let click = match newest {
                    Command::Stop => pressed,
                    Command::Move(_) => None,
                };
                return Some(Next {
                    click,
                    command: newest,
                });
            }
            Err(mpsc::TryRecvError::Disconnected) => return None,
        }
    }
}

fn set_status(status: &Arc<Mutex<String>>, text: impl Into<String>) {
    *status.locked() = text.into();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_directions_to_dahua_codes() {
        assert_eq!(
            DahuaCommand::from_velocity(PtzVelocity::new(1.0, 0.0, 0.0))
                .unwrap()
                .code,
            "Right"
        );
        assert_eq!(
            DahuaCommand::from_velocity(PtzVelocity::new(-1.0, 0.0, 0.0))
                .unwrap()
                .code,
            "Left"
        );
        assert_eq!(
            DahuaCommand::from_velocity(PtzVelocity::new(0.0, 1.0, 0.0))
                .unwrap()
                .code,
            "Up"
        );
        assert_eq!(
            DahuaCommand::from_velocity(PtzVelocity::new(0.0, 0.0, 1.0))
                .unwrap()
                .code,
            "ZoomTele"
        );
        assert_eq!(
            DahuaCommand::from_velocity(PtzVelocity::new(0.0, 0.0, -1.0))
                .unwrap()
                .code,
            "ZoomWide"
        );
    }

    #[test]
    fn reports_no_command_for_no_movement() {
        assert!(DahuaCommand::from_velocity(PtzVelocity::default()).is_none());
        assert!(PtzVelocity::default().is_still());
    }

    #[test]
    fn a_diagonal_carries_both_speeds() {
        let command = DahuaCommand::from_velocity(PtzVelocity::new(1.0, 1.0, 0.0)).unwrap();
        assert_eq!(command.code, "RightUp");
        assert_eq!(command.arg1, 8);
        assert_eq!(command.arg2, 8);
    }

    #[test]
    fn a_straight_move_keeps_arg1_empty() {
        let command = DahuaCommand::from_velocity(PtzVelocity::new(0.5, 0.0, 0.0)).unwrap();
        assert_eq!(command.arg1, 0);
        assert_eq!(command.arg2, 4);
    }

    #[test]
    fn speed_is_clamped_to_the_api_scale() {
        let command = DahuaCommand::from_velocity(PtzVelocity::new(0.05, 0.0, 0.0)).unwrap();
        assert_eq!(command.arg2, 1);
    }

    struct Recorder(Arc<Mutex<Vec<&'static str>>>);

    impl Driver for Recorder {
        fn probe(&mut self) -> bool {
            true
        }

        fn status(&self) -> &str {
            "ready"
        }

        fn move_by(&mut self, _velocity: PtzVelocity) -> bool {
            self.0.locked().push("move");
            true
        }

        fn stop_with_retries(&mut self) -> bool {
            self.0.locked().push("stop");
            true
        }
    }

    #[test]
    fn a_camera_where_nothing_answers_for_ptz_says_so_plainly() {
        use crate::Brand;
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = closed.local_addr().unwrap().port();
        drop(closed);
        let camera = CameraConfig {
            host: "127.0.0.1".into(),
            user: "admin".into(),
            password: "s3cret".into(),
            dvrip_port: port,
            api_port: port,
            api_https: false,
            onvif_port: port,
            ..CameraConfig::default()
        };

        // All at once: the system takes seconds to refuse a closed local port.
        let sessions: Vec<(&str, PtzSession)> = [
            (Brand::Dahua, false, "DVRIP"),
            (Brand::Dahua, true, "HTTP API"),
            (Brand::Reolink, false, "API"),
            (Brand::Other, false, "ONVIF"),
        ]
        .into_iter()
        .map(|(brand, http_api, over)| {
            let session = PtzSession::connect(&CameraConfig {
                brand,
                http_api,
                ..camera.clone()
            });
            (over, session)
        })
        .collect();
        for (over, session) in sessions {
            let until = Instant::now() + Duration::from_secs(15);
            while session.status().starts_with("PTZ: connecting") && Instant::now() < until {
                thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(session.status(), unreachable(over, port));
        }
    }

    fn asked_for(asked: &Arc<Mutex<Vec<&'static str>>>, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while asked.locked().len() < count {
            assert!(
                Instant::now() < deadline,
                "the camera was asked only {:?}",
                asked.locked()
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn dropping_a_session_stops_the_camera_and_returns() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let session = PtzSession::start(Recorder(asked.clone()));
        session.move_by(PtzVelocity::new(1.0, 0.0, 0.0));
        asked_for(&asked, 1);
        session.move_by(PtzVelocity::new(0.0, 1.0, 0.0));

        let (done, dropped) = mpsc::channel();
        thread::spawn(move || {
            drop(session);
            let _ = done.send(());
        });

        assert!(
            dropped.recv_timeout(Duration::from_secs(5)).is_ok(),
            "dropping the session never returned"
        );
        let asked = asked.locked();
        assert_eq!(asked.first(), Some(&"move"));
        assert_eq!(asked.last(), Some(&"stop"), "the stop is the last word");
        assert!(asked.len() <= 3, "{asked:?}");
    }

    struct Slow {
        asked: Arc<Mutex<Vec<&'static str>>>,
        go: mpsc::Receiver<()>,
    }

    impl Driver for Slow {
        fn probe(&mut self) -> bool {
            true
        }

        fn status(&self) -> &str {
            "ready"
        }

        fn move_by(&mut self, _velocity: PtzVelocity) -> bool {
            self.asked.locked().push("move");
            let _ = self.go.recv();
            true
        }

        fn stop_with_retries(&mut self) -> bool {
            self.asked.locked().push("stop");
            true
        }
    }

    #[test]
    fn a_stop_never_waits_behind_the_moves_queued_before_it() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (go, wait) = mpsc::channel();
        let session = PtzSession::start(Slow {
            asked: asked.clone(),
            go: wait,
        });
        session.move_by(PtzVelocity::new(1.0, 0.0, 0.0));
        asked_for(&asked, 1);

        for _ in 0..5 {
            session.move_by(PtzVelocity::new(1.0, 0.0, 0.0));
        }
        session.stop();
        go.send(()).unwrap();

        asked_for(&asked, 2);
        drop(session);
        assert_eq!(
            *asked.locked(),
            ["move", "stop", "stop"],
            "the stop went out next, the five moves never, and the session stopped on its way out"
        );
    }

    struct SlowToConnect {
        asked: Arc<Mutex<Vec<&'static str>>>,
        go: mpsc::Receiver<()>,
    }

    impl Driver for SlowToConnect {
        fn probe(&mut self) -> bool {
            let _ = self.go.recv();
            true
        }

        fn status(&self) -> &str {
            "ready"
        }

        fn move_by(&mut self, _velocity: PtzVelocity) -> bool {
            self.asked.locked().push("move");
            true
        }

        fn stop_with_retries(&mut self) -> bool {
            self.asked.locked().push("stop");
            true
        }
    }

    #[test]
    fn a_click_let_go_before_it_was_sent_still_moves_the_camera() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (go, wait) = mpsc::channel();
        let session = PtzSession::start(SlowToConnect {
            asked: asked.clone(),
            go: wait,
        });

        session.move_by(PtzVelocity::new(0.0, 0.0, 1.0));
        session.stop();
        go.send(()).unwrap();

        asked_for(&asked, 2);
        assert_eq!(*asked.locked(), ["move", "stop"]);
    }

    #[test]
    fn a_click_while_the_last_stop_is_on_its_way_still_moves_the_camera() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (go, wait) = mpsc::channel();
        let session = PtzSession::start(Slow {
            asked: asked.clone(),
            go: wait,
        });
        session.move_by(PtzVelocity::new(1.0, 0.0, 0.0));
        asked_for(&asked, 1);
        session.stop();
        session.move_by(PtzVelocity::new(0.0, 0.0, 1.0));
        session.stop();
        go.send(()).unwrap();

        asked_for(&asked, 2);
        go.send(()).unwrap();
        asked_for(&asked, 3);
        assert_eq!(*asked.locked(), ["move", "move", "stop"]);
    }

    struct Stepper(Arc<Mutex<Vec<&'static str>>>);

    impl Driver for Stepper {
        fn probe(&mut self) -> bool {
            true
        }

        fn status(&self) -> &str {
            "ready"
        }

        fn move_by(&mut self, _velocity: PtzVelocity) -> bool {
            self.0.locked().push("step");
            true
        }

        fn stop_with_retries(&mut self) -> bool {
            self.0.locked().push("stop");
            true
        }

        fn repeat_every(&self) -> Option<Duration> {
            Some(Duration::from_millis(20))
        }
    }

    #[test]
    fn a_held_move_that_is_a_step_is_sent_again_until_it_is_let_go() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let session = PtzSession::start(Stepper(asked.clone()));
        let zoom_in = PtzVelocity::new(0.0, 0.0, 0.5);

        session.move_by(zoom_in);
        asked_for(&asked, 4);
        let before = asked.locked().len();
        session.move_by(zoom_in);
        session.stop();
        asked_for(&asked, before + 1);
        thread::sleep(Duration::from_millis(100));

        let asked = asked.locked();
        let (stop, steps) = asked.split_last().unwrap();
        assert_eq!(*stop, "stop", "nothing steps after it is let go: {asked:?}");
        assert!(steps.len() >= 4 && steps.iter().all(|each| *each == "step"));
    }

    #[test]
    fn a_move_after_a_stop_is_the_one_sent() {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (go, wait) = mpsc::channel();
        let session = PtzSession::start(Slow {
            asked: asked.clone(),
            go: wait,
        });
        session.move_by(PtzVelocity::new(1.0, 0.0, 0.0));
        asked_for(&asked, 1);

        session.stop();
        session.move_by(PtzVelocity::new(0.0, 1.0, 0.0));
        go.send(()).unwrap();

        asked_for(&asked, 2);
        assert_eq!(*asked.locked(), ["move", "move"]);
        drop(go);
    }

    /// Probes DVRIP on the camera `IPCAMERA_HOST`, `_USER` and `_PASSWORD` name, nudge-pans, stops.
    #[test]
    #[ignore = "needs a real camera with PTZ on the network"]
    fn drives_a_real_camera() {
        let mut camera = CameraConfig::default();
        camera.apply_env();

        let mut ptz = DvripPtz::new(&camera);
        assert!(ptz.probe(), "probe failed: {}", ptz.status());
        println!("probe: {}", ptz.status());

        assert!(
            ptz.move_by(PtzVelocity::new(0.3, 0.0, 0.0)),
            "the camera rejected the move"
        );
        thread::sleep(Duration::from_millis(500));
        assert!(ptz.stop_with_retries(), "the camera rejected the stop");
        println!("move and stop accepted");
    }
}
