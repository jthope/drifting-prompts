//! Dahua's HTTP API in both dialects, its PTZ and a watch over it, against a fake camera on
//! loopback that checks each digest login; and, when the owner asks, a real one.

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use ipcamera_core::{
    Brand, CameraConfig, CameraWatch, Spotlight,
    dahua_http::{DahuaHttp, DahuaHttpError, DahuaLight, Dialect},
    ptz::{PtzSession, PtzVelocity},
};
use md5::{Digest, Md5};

const PASSWORD: &str = "s3cret";
const REALM: &str = "Login to FAKE0001";

#[derive(Clone, Copy)]
struct Kind {
    xml: bool,
    ptz_channel: u32,
    caps: bool,
    /// A `Lighting_V2` white light; else the coaxial one.
    lighting: bool,
    /// How many logged-in requests a nonce answers; 0 for no limit.
    nonce_lasts: usize,
}

const BULB: Kind = Kind {
    xml: false,
    ptz_channel: 0,
    caps: true,
    lighting: true,
    nonce_lasts: 0,
};

const AMLINK_BULB: Kind = Kind { xml: true, ..BULB };

/// The infrared lamp, the bulb in two profiles, and a stub with no brightness.
const ROWS: [(&str, &str, bool, &str); 4] = [
    ("0][0][0", "InfraredLight", true, "Auto"),
    ("0][0][1", "WhiteLight", true, "Off"),
    ("0][1][1", "WhiteLight", true, "Off"),
    ("0][2][1", "WhiteLight", false, "Off"),
];

#[derive(Default)]
struct State {
    asked: Vec<(String, bool, String)>,
    refused: usize,
    headers: Vec<String>,
    config: BTreeMap<String, String>,
    nonce: u32,
    used: usize,
}

impl State {
    fn mode(&self, row: &str, default: &str) -> String {
        self.config
            .get(row)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }
}

struct FakeDahua {
    port: u16,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl FakeDahua {
    fn start(kind: Kind) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the bound address").port();
        let state = Arc::new(Mutex::new(State::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let (state, stop) = (state.clone(), stop.clone());
            thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(stream) = stream {
                        serve(stream, kind, &state);
                    }
                }
            })
        };
        Self {
            port,
            state,
            stop,
            worker: Some(worker),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .asked
            .iter()
            .filter(|(_, logged_in, _)| *logged_in)
            .map(|(path, _, body)| {
                if body.is_empty() {
                    path.clone()
                } else {
                    format!("{path} {}", fields(body))
                }
            })
            .collect()
    }

    fn refused(&self) -> usize {
        self.state.lock().unwrap().refused
    }

    fn config(&self, key: &str) -> Option<String> {
        self.state.lock().unwrap().config.get(key).cloned()
    }

    fn setting(&self, password: &str) -> CameraConfig {
        CameraConfig {
            brand: Brand::Dahua,
            host: "127.0.0.1".into(),
            user: "admin".into(),
            password: password.into(),
            api_port: self.port,
            api_https: false,
            http_api: true,
            dvrip_port: closed_port(),
            onvif_port: closed_port(),
            ..CameraConfig::default()
        }
    }
}

impl Drop for FakeDahua {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn md5_hex(text: &str) -> String {
    Md5::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn digest_fields(header: &str) -> BTreeMap<String, String> {
    header
        .trim()
        .trim_start_matches("Digest")
        .split(", ")
        .filter_map(|pair| pair.trim().split_once('='))
        .map(|(name, value)| (name.to_string(), value.trim_matches('"').to_string()))
        .collect()
}

fn all_of(xml: &str, name: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find(&format!("<{name}")) {
        let after = &rest[at + name.len() + 1..];
        if !after.starts_with([' ', '>']) {
            rest = after;
            continue;
        }
        let Some(open) = after.find('>') else { break };
        let inside = &after[open + 1..];
        let end = inside.find('<').unwrap_or(inside.len());
        found.push(inside[..end].trim().to_string());
        rest = &inside[end..];
    }
    found
}

fn fields(xml: &str) -> String {
    [
        "name", "channel", "code", "arg1", "arg2", "arg3", "Type", "IO",
    ]
    .into_iter()
    .flat_map(|name| {
        all_of(xml, name)
            .into_iter()
            .map(move |value| format!("{name}={value}"))
    })
    .collect::<Vec<_>>()
    .join(" ")
}

fn serve(stream: TcpStream, kind: Kind, state: &Mutex<State>) {
    let mut reader = BufReader::new(stream.try_clone().expect("a second handle"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = request_line.split_whitespace();
    let (method, path) = (
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
    );
    let (mut length, mut authorization) = (0, None);
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
            if name.eq_ignore_ascii_case("authorization") {
                authorization = Some(value.trim().to_string());
            }
        }
        lines.push(line);
    }
    let mut body = vec![0; length];
    let _ = reader.read_exact(&mut body);
    let body = String::from_utf8(body).unwrap_or_default();

    let mut state = state.lock().unwrap();
    state.headers.extend(lines);
    state
        .asked
        .push((path.clone(), authorization.is_some(), body.clone()));
    let challenge = |nonce: u32| {
        format!(
            "WWW-Authenticate: Digest realm=\"{REALM}\", qop=\"auth\", nonce=\"n{nonce}\", opaque=\"op\"\r\n"
        )
    };

    let ours = if kind.xml {
        method == "POST" && path.starts_with("/dev-api/")
    } else {
        method == "GET" && path.starts_with("/cgi-bin/")
    };
    if !ours {
        return reply(stream, "404 Not Found", "", "");
    }
    let Some(authorization) = authorization else {
        return reply(stream, "401 Unauthorized", &challenge(state.nonce), "");
    };
    let digest = digest_fields(&authorization);
    let field = |name: &str| digest.get(name).cloned().unwrap_or_default();
    if field("nonce") != format!("n{}", state.nonce) {
        // A nonce that ran out: not a wrong password.
        return reply(stream, "401 Unauthorized", &challenge(state.nonce), "");
    }
    let expected = md5_hex(&format!(
        "{}:{}:{}:{}:auth:{}",
        md5_hex(&format!("{}:{REALM}:{PASSWORD}", field("username"))),
        field("nonce"),
        field("nc"),
        field("cnonce"),
        md5_hex(&format!("{method}:{path}")),
    ));
    if field("uri") != path || field("response") != expected || field("opaque") != "op" {
        state.refused += 1;
        return reply(stream, "401 Unauthorized", &challenge(state.nonce), "");
    }
    state.used += 1;
    if kind.nonce_lasts > 0 && state.used >= kind.nonce_lasts {
        state.nonce += 1;
        state.used = 0;
    }

    let (status, text) = if kind.xml {
        answer_xml(&path, &body, kind, &mut state)
    } else {
        answer_cgi(&path, kind, &mut state)
    };
    drop(state);
    reply(stream, status, "", &text);
}

const BAD: (&str, &str) = ("400 Bad Request", "Error\r\nBad Request!\r\n");

fn answer_cgi(path: &str, kind: Kind, state: &mut State) -> (&'static str, String) {
    let query = path.trim_start_matches("/cgi-bin/");
    let channel = |query: &str| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix("channel="))
            .and_then(|channel| channel.parse::<u32>().ok())
    };
    let ok = |text: &str| ("200 OK", text.to_string());
    let bad = || (BAD.0, BAD.1.to_string());
    match query {
        "magicBox.cgi?action=getDeviceType" => ok("type=AL-L411W\r\n"),
        "magicBox.cgi?action=getSoftwareVersion" => {
            ok("version=2.800.0000000.1.R,build:2024-01-01\r\n")
        }
        "magicBox.cgi?action=getHardwareVersion" => ok("version=1.00\r\n"),
        caps if caps.starts_with("ptz.cgi?action=getCurrentProtocolCaps") => {
            if kind.caps && channel(caps) == Some(kind.ptz_channel) {
                ok("caps.Pan=true\r\ncaps.Tile=true\r\ncaps.Zoom=false\r\n")
            } else {
                bad()
            }
        }
        ptz if ptz.starts_with("ptz.cgi?action=") => {
            if channel(ptz) == Some(kind.ptz_channel) {
                ok("OK\r\n")
            } else {
                bad()
            }
        }
        "configManager.cgi?action=getConfig&name=Lighting_V2" => {
            let mut table = String::new();
            for (row, light, bank, mode) in ROWS {
                if light == "WhiteLight" && !kind.lighting {
                    continue;
                }
                table.push_str(&format!(
                    "table.Lighting_V2[{row}].LightType={light}\r\ntable.Lighting_V2[{row}].Mode={}\r\n",
                    state.mode(row, mode)
                ));
                if bank {
                    table.push_str(&format!(
                        "table.Lighting_V2[{row}].NearLight[0].Light=80\r\n"
                    ));
                }
            }
            ok(&table)
        }
        set if set.starts_with("configManager.cgi?action=setConfig&") => {
            for pair in set.split('&').skip(1) {
                if let Some((key, value)) = pair.split_once('=') {
                    let row = key
                        .trim_start_matches("Lighting_V2[")
                        .trim_end_matches("].Mode");
                    state.config.insert(row.to_string(), value.to_string());
                }
            }
            ok("OK\r\n")
        }
        // A bulb has no coaxial light.
        coaxial if coaxial.starts_with("coaxialControlIO.cgi") && kind.lighting => bad(),
        "coaxialControlIO.cgi?action=getStatus&channel=1" => ok(&format!(
            "status.status.Speaker=Off\r\nstatus.status.WhiteLight={}\r\n",
            state.mode("coaxial", "Off")
        )),
        control
            if control.starts_with(
                "coaxialControlIO.cgi?action=control&channel=1&info[0].Type=1&info[0].IO=",
            ) =>
        {
            let on = if control.ends_with("IO=1") {
                "On"
            } else {
                "Off"
            };
            state.config.insert("coaxial".into(), on.into());
            ok("OK\r\n")
        }
        _ => ("404 Not Found", BAD.1.to_string()),
    }
}

fn answer_xml(path: &str, body: &str, kind: Kind, state: &mut State) -> (&'static str, String) {
    let reply = |inside: &str| {
        format!(r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?><root>{inside}</root>"#)
    };
    let ok = |inside: &str| ("200 OK", reply(inside));
    let bad = || (BAD.0, BAD.1.to_string());
    let channel = all_of(body, "channel")
        .first()
        .and_then(|channel| channel.parse::<u32>().ok());
    match path.trim_start_matches("/dev-api/") {
        "magicBox/getDeviceType" => ok(r#"<type type="string">AL-L411W</type>"#),
        "magicBox/getSoftwareVersion" => ok("<version>1.000.0.R,build:2024-11-20</version>"),
        "magicBox/getHardwareVersion" => ok("<version>1.00</version>"),
        "ptz/getCurrentProtocolCaps" => {
            if kind.caps && channel == Some(kind.ptz_channel) {
                ok(
                    r#"<caps><Pan type="bool">True</Pan><Tile type="bool">True</Tile><Zoom type="bool">False</Zoom></caps>"#,
                )
            } else {
                bad()
            }
        }
        "ptz/start" | "ptz/stop" => {
            if channel == Some(kind.ptz_channel) {
                ("200 OK", "OK".to_string())
            } else {
                bad()
            }
        }
        "configManager/getConfig" if body.contains(">Lighting_V2<") => {
            let mut profiles: BTreeMap<&str, String> = BTreeMap::new();
            for (row, light, bank, mode) in ROWS {
                if light == "WhiteLight" && !kind.lighting {
                    continue;
                }
                let profile = &row[..4];
                let bank = if bank {
                    r#"<NearLight><NearLight><Light type="number">80</Light></NearLight></NearLight>"#
                } else {
                    ""
                };
                profiles.entry(profile).or_default().push_str(&format!(
                    r#"<Lighting_V2><LightType type="string">{light}</LightType><Mode type="string">{}</Mode>{bank}</Lighting_V2>"#,
                    state.mode(row, mode)
                ));
            }
            let channel: String = profiles
                .values()
                .map(|lights| format!("<Lighting_V2>{lights}</Lighting_V2>"))
                .collect();
            ok(&format!(
                "<Lighting_V2><Lighting_V2>{channel}</Lighting_V2></Lighting_V2>"
            ))
        }
        "configManager/setConfig" => {
            let rows: Vec<&str> = ROWS
                .iter()
                .filter(|(_, light, _, _)| kind.lighting || *light != "WhiteLight")
                .map(|(row, _, _, _)| *row)
                .collect();
            let modes = all_of(body, "Mode");
            if modes.len() != rows.len() {
                return bad();
            }
            for (row, mode) in rows.into_iter().zip(modes) {
                state.config.insert(row.to_string(), mode);
            }
            ("200 OK", "OK".to_string())
        }
        // A bulb has no coaxial light.
        coaxial if coaxial.starts_with("coaxialControlIO/") && kind.lighting => bad(),
        "coaxialControlIO/getstatus" if channel == Some(0) => ok(&format!(
            r#"<status><whitelight type="string">{}</whitelight><speaker type="string">off</speaker></status>"#,
            state.mode("coaxial", "Off").to_ascii_lowercase()
        )),
        "coaxialControlIO/control" if channel == Some(0) => {
            let on = all_of(body, "IO").first().is_some_and(|io| io == "1");
            state
                .config
                .insert("coaxial".into(), if on { "On" } else { "Off" }.into());
            ("200 OK", "OK".to_string())
        }
        _ => ("404 Not Found", String::new()),
    }
}

fn reply(mut stream: TcpStream, status: &str, headers: &str, text: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\n{headers}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    let _ = stream.flush();
}

fn eventually(mut done: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        if done() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

/// A stop replaces a move not sent yet, so a test that wants both lets the move go out first.
fn sent(camera: &FakeDahua, count: usize) {
    assert!(
        eventually(|| moves(camera).len() >= count),
        "{:?}",
        moves(camera)
    );
}

fn moves(camera: &FakeDahua) -> Vec<String> {
    camera
        .asked()
        .into_iter()
        .filter(|asked| {
            ["action=start", "action=stop", "ptz/start", "ptz/stop"]
                .iter()
                .any(|call| asked.contains(call))
        })
        .collect()
}

#[test]
fn dahuas_api_is_told_apart_without_a_login_in_either_dialect() {
    for kind in [BULB, AMLINK_BULB] {
        let camera = FakeDahua::start(kind);
        assert!(DahuaHttp::answers("127.0.0.1", camera.port, false));
        assert!(camera.asked().is_empty(), "no login");
        assert_eq!(camera.refused(), 0);
    }
    assert!(!DahuaHttp::answers("127.0.0.1", closed_port(), false));
}

#[test]
fn the_login_is_a_digest_and_the_camera_says_what_it_is() {
    let camera = FakeDahua::start(BULB);
    let (mut client, device) =
        DahuaHttp::connect("127.0.0.1", camera.port, false, "admin", PASSWORD).unwrap();

    assert_eq!(client.dialect(), Dialect::Cgi);
    assert_eq!(device.model, "AL-L411W");
    assert_eq!(device.firmware, "2.800.0000000.1.R,build:2024-01-01");
    assert_eq!(device.hardware, "1.00");
    let caps = client.ptz_caps(1).unwrap().unwrap();
    assert!(caps.pan_tilt && !caps.zoom);
    assert_eq!(caps.channel, 0, "counted from 0, as most firmware does");
    assert_eq!(
        client.light(1).unwrap(),
        Some(DahuaLight::Lighting(vec![vec![0, 0, 1], vec![0, 1, 1]])),
        "the white light in each profile that has a brightness, not the infrared lamp"
    );
    assert_eq!(camera.refused(), 0);
    let headers = camera.state.lock().unwrap().headers.join("");
    assert!(!headers.contains(PASSWORD), "the password never crosses");
}

#[test]
fn a_camera_on_the_xml_api_is_logged_into_once_and_says_what_it_is() {
    let camera = FakeDahua::start(AMLINK_BULB);
    let (mut client, device) =
        DahuaHttp::connect("127.0.0.1", camera.port, false, "admin", PASSWORD).unwrap();

    assert_eq!(client.dialect(), Dialect::DevApi);
    assert_eq!(device.model, "AL-L411W");
    assert_eq!(device.firmware, "1.000.0.R,build:2024-11-20");
    let caps = client.ptz_caps(1).unwrap().unwrap();
    assert!(caps.pan_tilt && !caps.zoom);
    assert_eq!(caps.channel, 0);
    assert_eq!(
        client.light(1).unwrap(),
        Some(DahuaLight::Lighting(vec![vec![0, 0, 1], vec![0, 1, 0]])),
        "the bulb in each profile, by where it is in the XML table"
    );
    assert_eq!(camera.refused(), 0);
    let headers = camera.state.lock().unwrap().headers.join("");
    assert!(!headers.contains(PASSWORD), "the password never crosses");
}

#[test]
fn a_wrong_password_is_one_attempt_and_a_watch_stops_for_good() {
    for kind in [BULB, AMLINK_BULB] {
        let camera = FakeDahua::start(kind);
        assert!(matches!(
            DahuaHttp::connect("127.0.0.1", camera.port, false, "admin", "wrong"),
            Err(DahuaHttpError::LoginRejected(_))
        ));
        assert_eq!(camera.refused(), 1);

        let watch = CameraWatch::start(&camera.setting("wrong")).unwrap();
        assert!(eventually(|| watch.watched().problem.is_some()));
        assert!(
            watch
                .watched()
                .problem
                .unwrap()
                .contains("refused the login"),
            "{:?}",
            watch.watched().problem
        );
        thread::sleep(Duration::from_millis(300));
        assert_eq!(camera.refused(), 2, "one attempt each, never retried");
    }
}

#[test]
fn a_nonce_that_ran_out_gets_one_more_try() {
    let camera = FakeDahua::start(Kind {
        nonce_lasts: 2,
        ..BULB
    });
    let (mut client, _) =
        DahuaHttp::connect("127.0.0.1", camera.port, false, "admin", PASSWORD).unwrap();
    for _ in 0..4 {
        client.ptz_caps(1).unwrap().unwrap();
    }
    assert_eq!(camera.refused(), 0);
}

#[test]
fn a_bulb_moves_over_the_api_on_the_channel_it_counts() {
    let camera = FakeDahua::start(Kind {
        ptz_channel: 1,
        ..BULB
    });
    let session = PtzSession::connect(&camera.setting(PASSWORD));
    session.move_by(PtzVelocity::new(1.0, 0.0, 0.0));
    sent(&camera, 1);
    session.stop();
    drop(session);

    assert_eq!(
        moves(&camera),
        [
            "/cgi-bin/ptz.cgi?action=start&channel=1&code=Right&arg1=0&arg2=8&arg3=0",
            "/cgi-bin/ptz.cgi?action=stop&channel=1&code=Right&arg1=0&arg2=0&arg3=0",
        ]
    );
}

#[test]
fn a_bulb_on_the_xml_api_moves_from_channel_0() {
    let camera = FakeDahua::start(AMLINK_BULB);
    let session = PtzSession::connect(&camera.setting(PASSWORD));
    session.move_by(PtzVelocity::new(-1.0, 1.0, 0.0));
    sent(&camera, 1);
    session.stop();
    drop(session);

    assert_eq!(
        moves(&camera),
        [
            "/dev-api/ptz/start channel=0 code=LeftUp arg1=8 arg2=8 arg3=0",
            "/dev-api/ptz/stop channel=0 code=LeftUp arg1=0 arg2=0 arg3=0",
        ]
    );
}

#[test]
fn a_camera_that_wont_say_its_ptz_tries_the_other_channel_numbering_once() {
    let camera = FakeDahua::start(Kind {
        ptz_channel: 1,
        caps: false,
        ..BULB
    });
    let session = PtzSession::connect(&camera.setting(PASSWORD));
    session.move_by(PtzVelocity::new(0.0, -0.5, 0.0));
    sent(&camera, 2);
    session.stop();
    drop(session);

    assert_eq!(
        moves(&camera),
        [
            "/cgi-bin/ptz.cgi?action=start&channel=0&code=Down&arg1=0&arg2=4&arg3=0",
            "/cgi-bin/ptz.cgi?action=start&channel=1&code=Down&arg1=0&arg2=4&arg3=0",
            "/cgi-bin/ptz.cgi?action=stop&channel=1&code=Down&arg1=0&arg2=0&arg3=0",
        ]
    );
}

#[test]
fn a_watch_follows_and_switches_a_bulbs_light_in_either_dialect() {
    for kind in [BULB, AMLINK_BULB] {
        let camera = FakeDahua::start(kind);
        let watch = CameraWatch::start(&camera.setting(PASSWORD)).unwrap();
        assert!(eventually(|| watch.watched().activity.spotlight.is_some()));
        let watched = watch.watched();
        assert_eq!(
            watched.summary(),
            "AL-L411W: Spotlight, Pan and tilt.",
            "{watched:?}"
        );
        assert_eq!(
            watched.activity.spotlight,
            Some(Spotlight {
                on: false,
                brightness: 80
            })
        );

        let answer = watch.set_spotlight(Spotlight {
            on: true,
            brightness: 100,
        });
        assert_eq!(answer.recv_timeout(Duration::from_secs(5)).unwrap(), Ok(()));
        assert_eq!(camera.config("0][0][1").as_deref(), Some("Manual"));
        assert_eq!(camera.config("0][1][1").as_deref(), Some("Manual"));
        let untouched = |row: &str, mode: &str| camera.config(row).is_none_or(|sent| sent == mode);
        assert!(untouched("0][2][1", "Off"), "a stub");
        assert!(untouched("0][0][0", "Auto"), "the infrared lamp");
        assert!(
            watch
                .watched()
                .activity
                .spotlight
                .is_some_and(|light| light.on)
        );
        assert_eq!(camera.refused(), 0);
    }
}

#[test]
fn a_floodlight_without_a_lighting_row_switches_its_coaxial_light_in_either_dialect() {
    for kind in [BULB, AMLINK_BULB] {
        let camera = FakeDahua::start(Kind {
            lighting: false,
            ..kind
        });
        let (mut client, _) =
            DahuaHttp::connect("127.0.0.1", camera.port, false, "admin", PASSWORD).unwrap();
        let light = client.light(1).unwrap().unwrap();
        assert_eq!(light, DahuaLight::Coaxial(1));
        assert!(!client.light_state(&light).unwrap().on);
        client.set_light(&light, true).unwrap();
        assert!(client.light_state(&light).unwrap().on);
        assert_eq!(camera.config("coaxial").as_deref(), Some("On"));
    }
}

#[test]
fn detect_finds_a_dahua_over_its_api_where_dvrip_is_closed_in_either_dialect() {
    // Both at once: the system takes seconds to refuse each closed port.
    let cameras = [BULB, AMLINK_BULB].map(FakeDahua::start);
    let detected = thread::scope(|scope| {
        cameras
            .each_ref()
            .map(|camera| {
                scope.spawn(|| {
                    ipcamera_core::detect(&CameraConfig {
                        brand: Brand::Other,
                        http_api: false,
                        ..camera.setting(PASSWORD)
                    })
                })
            })
            .map(|asked| asked.join().unwrap())
    });
    for (camera, found) in cameras.iter().zip(detected) {
        let found = found.unwrap();
        assert_eq!(found.brand, Brand::Dahua);
        assert_eq!(found.api, Some((camera.port, false)));
        assert!(found.http_api, "PTZ and the light over the API");
        assert_eq!(found.device.unwrap().model, "AL-L411W");
        let features = found.features.unwrap();
        assert!(features.spotlight && features.pan_tilt);
        assert_eq!(camera.refused(), 0);
    }
}

/// Prints which of `host`'s ports answer, and what the HTTP ones say, without a login.
fn survey(host: &str) {
    const PORTS: [u16; 14] = [
        80, 443, 554, 8000, 8080, 8081, 8443, 8554, 8888, 8899, 9000, 34567, 37777, 2020,
    ];
    let open: Vec<u16> = thread::scope(|scope| {
        let asked: Vec<_> = PORTS
            .iter()
            .map(|&port| {
                (
                    port,
                    scope.spawn(move || {
                        (host, port)
                            .to_socket_addrs()
                            .ok()
                            .and_then(|mut addresses| addresses.next())
                            .is_some_and(|address| {
                                TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok()
                            })
                    }),
                )
            })
            .collect();
        asked
            .into_iter()
            .filter_map(|(port, answer)| answer.join().unwrap_or(false).then_some(port))
            .collect()
    });
    println!("open ports: {open:?} (of {PORTS:?})");

    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .disable_verification(true)
                    .build(),
            )
            .timeout_global(Some(Duration::from_secs(4)))
            .http_status_as_error(false)
            .build(),
    );
    for port in open {
        // RTSP and DVRIP don't speak HTTP.
        if [554, 8554, 37777, 34567].contains(&port) {
            continue;
        }
        for https in [false, true] {
            let scheme = if https { "https" } else { "http" };
            for (method, path) in [
                ("GET", "/"),
                ("GET", "/cgi-bin/magicBox.cgi?action=getDeviceType"),
                ("POST", "/dev-api/magicBox/getDeviceType"),
            ] {
                let url = format!("{scheme}://{host}:{port}{path}");
                let answer = if method == "POST" {
                    agent
                        .post(&url)
                        .header("Content-Type", "application/xml")
                        .send(r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?><root></root>"#)
                } else {
                    agent.get(&url).call()
                };
                match answer {
                    Ok(response) => {
                        let header = |name: &str| {
                            response
                                .headers()
                                .get_all(name)
                                .iter()
                                .filter_map(|value| value.to_str().ok())
                                .collect::<Vec<_>>()
                                .join(" | ")
                        };
                        println!(
                            "{method} {url}: {} server [{}] login [{}]",
                            response.status(),
                            header("server"),
                            header("www-authenticate")
                        );
                    }
                    Err(error) => {
                        println!("{method} {url}: {error}");
                        break;
                    }
                }
            }
        }
    }
}

/// Asks the camera `IPCAMERA_DAHUA_HOST`, `_USER` and `_PASSWORD` name what it is and has,
/// changing nothing; a refused login stops it there.
#[test]
#[ignore = "needs the owner's camera"]
fn the_real_camera_says_what_its_api_has() {
    let variable = |name: &str| std::env::var(format!("IPCAMERA_DAHUA_{name}")).ok();
    let (Some(host), Some(user), Some(password)) =
        (variable("HOST"), variable("USER"), variable("PASSWORD"))
    else {
        panic!("set IPCAMERA_DAHUA_HOST, _USER and _PASSWORD");
    };
    let port = variable("PORT")
        .and_then(|port| port.parse().ok())
        .unwrap_or(80);
    let https = variable("HTTPS").is_some_and(|https| https == "1");

    survey(&host);

    let setting = CameraConfig {
        host: host.clone(),
        user: user.clone(),
        password: password.clone(),
        api_port: port,
        api_https: https,
        ..CameraConfig::default()
    };
    let found = match ipcamera_core::detect(&setting) {
        Ok(found) => found,
        // A refused login stops here, before another one.
        Err(why) => return println!("detect: {why}"),
    };
    println!("detect: {found:?}");
    let Some((port, https)) = found.api.filter(|_| found.brand == Brand::Dahua) else {
        return println!("Dahua's HTTP API didn't answer, so there is nothing more to ask.");
    };

    let (mut client, device) = match DahuaHttp::connect(&host, port, https, &user, &password) {
        Ok(connected) => connected,
        Err(error) => return println!("login: {error}"),
    };
    println!("dialect: {:?}; device: {device:?}", client.dialect());
    println!("PTZ: {:?}", client.ptz_caps(1));
    match client.light(1) {
        Ok(Some(light)) => println!("light: {light:?}, now {:?}", client.light_state(&light)),
        other => println!("light: {other:?}"),
    }
    let raw = match client.dialect() {
        Dialect::Cgi => [
            "configManager.cgi?action=getConfig&name=Lighting_V2",
            "configManager.cgi?action=getConfig&name=Lighting",
            "configManager.cgi?action=getConfig&name=LightingScheme",
            "coaxialControlIO.cgi?action=getStatus&channel=1",
            "magicBox.cgi?action=getVendor",
        ]
        .map(|query| (query, client.get(query))),
        Dialect::DevApi => [
            (
                "configManager/getConfig",
                r#"<name type="string">Lighting_V2</name>"#,
            ),
            (
                "configManager/getConfig",
                r#"<name type="string">Lighting</name>"#,
            ),
            (
                "configManager/getConfig",
                r#"<name type="string">LightingScheme</name>"#,
            ),
            (
                "coaxialControlIO/getstatus",
                r#"<channel type="number">0</channel>"#,
            ),
            ("magicBox/getVendor", ""),
        ]
        .map(|(path, fields)| (path, client.post(path, fields))),
    };
    for (asked, said) in raw {
        println!("--- {asked}");
        match said {
            Ok(text) => println!("{}", text.trim_end()),
            Err(error) => println!("{error}"),
        }
    }
}
