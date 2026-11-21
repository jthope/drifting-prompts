//! The Reolink client against a fake camera on loopback that records every request; and, when the
//! owner asks, the real one.

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use ipcamera_core::{
    Brand, CameraActivity, CameraConfig, CameraStream, CameraWatch, Detection, Spotlight,
    ptz::PtzVelocity,
    reolink::{ReolinkClient, ReolinkError},
};
use serde_json::{Value, json};

const PASSWORD: &str = "s3cret";

#[derive(Clone, Debug)]
struct Seen {
    query: String,
    commands: Vec<String>,
    body: Value,
}

impl Seen {
    fn token(&self) -> Option<&str> {
        self.query
            .split('&')
            .find_map(|pair| pair.strip_prefix("token="))
    }
}

type Answer = Box<dyn FnMut(&Value, Option<&str>) -> Result<Value, &'static str> + Send>;

/// The stream's first bytes, or `None` to hang up.
type FlvAnswer = Box<dyn Fn(&str) -> Option<Vec<u8>> + Send>;

struct FakeCamera {
    port: u16,
    seen: Arc<Mutex<Vec<Seen>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl FakeCamera {
    fn start(
        answer: impl FnMut(&Value, Option<&str>) -> Result<Value, &'static str> + Send + 'static,
    ) -> Self {
        Self::serving(answer, |_| None)
    }

    fn serving(
        answer: impl FnMut(&Value, Option<&str>) -> Result<Value, &'static str> + Send + 'static,
        flv: impl Fn(&str) -> Option<Vec<u8>> + Send + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the bound address").port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let worker = {
            let seen = seen.clone();
            let stop = stop.clone();
            let mut answer: Answer = Box::new(answer);
            let flv: FlvAnswer = Box::new(flv);
            thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(stream) = stream {
                        serve(stream, &seen, &mut answer, &flv);
                    }
                }
            })
        };

        Self {
            port,
            seen,
            stop,
            worker: Some(worker),
        }
    }

    /// A camera that hands out `tok1`, `tok2`, ….
    fn with_logins(
        answer: impl FnMut(&Value, Option<&str>) -> Result<Value, &'static str> + Send + 'static,
    ) -> Self {
        Self::with_logins_and_flv(answer, |_| None)
    }

    fn with_logins_and_flv(
        mut answer: impl FnMut(&Value, Option<&str>) -> Result<Value, &'static str> + Send + 'static,
        flv: impl Fn(&str) -> Option<Vec<u8>> + Send + 'static,
    ) -> Self {
        let mut logins = 0;
        Self::serving(
            move |command, token| {
                if command["cmd"] == "Login" {
                    logins += 1;
                    Ok(json!({ "Token": { "name": format!("tok{logins}"), "leaseTime": 3600 } }))
                } else if command["cmd"] == "Logout" {
                    Ok(json!({ "rspCode": 200 }))
                } else {
                    answer(command, token)
                }
            },
            flv,
        )
    }

    fn connect(&self) -> Result<ReolinkClient, ReolinkError> {
        ReolinkClient::connect("127.0.0.1", self.port, false, "admin", PASSWORD)
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    fn commands(&self) -> Vec<String> {
        self.seen()
            .into_iter()
            .flat_map(|seen| seen.commands)
            .collect()
    }
}

impl Drop for FakeCamera {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wakes the listener, which then sees the stop.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(stream: TcpStream, seen: &Mutex<Vec<Seen>>, answer: &mut Answer, flv: &FlvAnswer) {
    let mut reader = BufReader::new(stream.try_clone().expect("a second handle"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let target = request_line.split_whitespace().nth(1).unwrap_or_default();
    let query = target
        .split_once('?')
        .map(|(_, query)| query)
        .unwrap_or_default();
    if target.starts_with("/flv?") {
        seen.lock().unwrap().push(Seen {
            query: query.to_string(),
            commands: Vec::new(),
            body: Value::Null,
        });
        let name = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("stream="))
            .unwrap_or_default();
        if let Some(bytes) = flv(name) {
            let mut stream = stream;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: video/x-flv\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(&bytes);
            let _ = stream.flush();
        }
        return;
    }

    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("the body");
    let body: Value = serde_json::from_slice(&body).expect("a JSON body");
    let commands: Vec<Value> = body.as_array().cloned().unwrap_or_default();
    let request = Seen {
        query: query.to_string(),
        commands: commands
            .iter()
            .map(|command| command["cmd"].as_str().unwrap_or_default().to_string())
            .collect(),
        body: body.clone(),
    };
    let token = request.token().map(str::to_string);
    seen.lock().unwrap().push(request);

    let replies: Vec<Value> = commands
        .iter()
        .map(|command| match answer(command, token.as_deref()) {
            Ok(value) => json!({ "cmd": command["cmd"], "code": 0, "value": value }),
            Err(detail) => json!({ "cmd": command["cmd"], "code": 1, "error": { "rspCode": -1, "detail": detail } }),
        })
        .collect();
    // Some firmware answers a request with one command it lacks with a single reply.
    let spoiled = commands.len() > 1
        && replies
            .iter()
            .any(|reply| reply["error"]["detail"] == SPOILS_THE_REQUEST);
    let replies = if spoiled {
        vec![
            json!({ "cmd": "Unknown", "code": 1, "error": { "rspCode": -9, "detail": SPOILS_THE_REQUEST } }),
        ]
    } else {
        replies
    };
    let text = serde_json::to_string(&replies).unwrap();
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    let _ = stream.flush();
}

#[test]
fn it_logs_in_once_and_sends_the_token_with_every_call() {
    let camera = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("GetDevInfo") => Ok(
            json!({ "DevInfo": { "model": "RLC-811A", "firmVer": "v3.1.0", "hardVer": "IPC_523" } }),
        ),
        Some("GetMdState") => Ok(json!({ "state": 1 })),
        Some("GetAiState") => Ok(json!({
            "people": { "alarm_state": 1, "support": 1 },
            "vehicle": { "alarm_state": 0, "support": 1 },
        })),
        _ => Err("not support"),
    });

    let mut client = camera.connect().unwrap();
    assert_eq!(client.device_info().unwrap().model, "RLC-811A");
    assert_eq!(
        client.activity(1, false).unwrap(),
        CameraActivity {
            motion: true,
            detected: vec![Detection::Person],
            spotlight: None,
        }
    );

    let seen = camera.seen();
    assert_eq!(
        camera.commands(),
        ["Login", "GetDevInfo", "GetMdState", "GetAiState"]
    );
    assert_eq!(seen[0].query, "cmd=Login", "a login carries no token");
    assert!(
        seen[1..].iter().all(|seen| seen.token() == Some("tok1")),
        "{seen:?}"
    );
    assert_eq!(
        seen[2].body[0]["param"]["channel"], 0,
        "channel 1 is Reolink's 0"
    );
    assert!(
        seen.iter().all(|seen| !seen.query.contains(PASSWORD)),
        "the password never goes in a URL"
    );
}

#[test]
fn a_refused_login_is_one_attempt() {
    let camera = FakeCamera::start(|_, _| Err("login failed"));

    let refused = camera.connect().err().expect("a refusal");

    assert_eq!(refused, ReolinkError::LoginRejected("login failed".into()));
    assert!(
        refused
            .to_string()
            .contains("Check the username and password"),
        "{refused}"
    );
    assert_eq!(camera.commands(), ["Login"]);
}

#[test]
fn a_locked_account_says_so() {
    let camera = FakeCamera::start(|_, _| Err("Login has been locked"));

    assert_eq!(camera.connect().err(), Some(ReolinkError::Locked));
    assert_eq!(camera.commands(), ["Login"]);
}

#[test]
fn a_forgotten_token_means_one_more_login_and_the_call_again() {
    let camera = FakeCamera::with_logins(|command, token| match (command["cmd"].as_str(), token) {
        (_, Some("tok1")) => Err("please login first"),
        (Some("GetWhiteLed"), _) => {
            Ok(json!({ "WhiteLed": { "channel": 0, "state": 1, "bright": 60 } }))
        }
        _ => Err("not support"),
    });

    let mut client = camera.connect().unwrap();
    let spotlight = client.spotlight(1).unwrap();

    assert_eq!(
        spotlight,
        Spotlight {
            on: true,
            brightness: 60
        }
    );
    assert_eq!(
        camera.commands(),
        ["Login", "GetWhiteLed", "Login", "GetWhiteLed"]
    );
    assert_eq!(camera.seen()[3].token(), Some("tok2"));
}

#[test]
fn a_password_changed_since_stops_the_client_for_good() {
    let mut logins = 0;
    let camera = FakeCamera::start(move |command, _| match command["cmd"].as_str() {
        Some("Login") => {
            logins += 1;
            if logins == 1 {
                Ok(json!({ "Token": { "name": "tok1", "leaseTime": 3600 } }))
            } else {
                Err("password wrong")
            }
        }
        _ => Err("please login first"),
    });

    let mut client = camera.connect().unwrap();
    let first = client.activity(1, false).unwrap_err();
    let second = client.activity(1, false).unwrap_err();

    assert_eq!(first, ReolinkError::LoginRejected("password wrong".into()));
    assert_eq!(second, first, "the refusal is remembered");
    assert_eq!(
        camera.commands(),
        ["Login", "GetMdState", "GetAiState", "Login"],
        "nothing reaches the camera after the refusal"
    );
}

#[test]
fn the_features_are_what_the_camera_says_it_has() {
    let camera = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("GetAbility") => {
            Ok(json!({ "Ability": { "abilityChn": [{ "floodLight": { "ver": 1 } }] } }))
        }
        Some("GetMdState") => Ok(json!({ "state": 0 })),
        Some("GetAiState") => Ok(json!({
            "people": { "alarm_state": 0, "support": 1 },
            "dog_cat": { "alarm_state": 0, "support": 1 },
            "face": { "alarm_state": 0, "support": 0 },
        })),
        _ => Err("not support"),
    });

    let features = camera.connect().unwrap().features(1).unwrap();

    assert!(features.motion);
    assert!(features.spotlight);
    assert_eq!(features.detections, [Detection::Person, Detection::Animal]);
    assert_eq!(
        camera.seen()[1].body[0]["param"]["User"]["userName"],
        "admin"
    );
}

#[test]
fn a_camera_without_ai_still_reports_motion() {
    let camera = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("GetMdState") => Ok(json!({ "state": 1 })),
        _ => Err("not support"),
    });

    let activity = camera.connect().unwrap().activity(1, false).unwrap();

    assert!(activity.motion);
    assert!(activity.detected.is_empty());
}

#[test]
fn the_spotlight_switches_and_the_session_ends_with_the_client() {
    let camera = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("SetWhiteLed") => Ok(json!({ "rspCode": 200 })),
        _ => Err("not support"),
    });

    let mut client = camera.connect().unwrap();
    client
        .set_spotlight(
            1,
            Spotlight {
                on: true,
                brightness: 80,
            },
        )
        .unwrap();
    drop(client);

    let seen = camera.seen();
    assert_eq!(camera.commands(), ["Login", "SetWhiteLed", "Logout"]);
    assert_eq!(
        seen[1].body[0]["param"]["WhiteLed"],
        json!({ "channel": 0, "state": 1, "bright": 80 })
    );
    assert_eq!(seen[2].token(), Some("tok1"));
}

#[test]
fn the_activity_carries_the_spotlight_when_asked() {
    let camera = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("GetMdState") => Ok(json!({ "state": 0 })),
        Some("GetWhiteLed") => Ok(json!({ "WhiteLed": { "state": 1, "bright": 40 } })),
        _ => Err("not support"),
    });

    let activity = camera.connect().unwrap().activity(1, true).unwrap();

    assert_eq!(
        activity.spotlight,
        Some(Spotlight {
            on: true,
            brightness: 40
        })
    );
    assert_eq!(
        camera.commands()[1..4],
        ["GetMdState", "GetAiState", "GetWhiteLed"],
        "in one request"
    );
    assert_eq!(camera.seen()[1].commands.len(), 3);
}

#[test]
fn a_ptz_move_goes_at_a_speed_only_where_the_camera_takes_one() {
    let camera = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("GetAbility") => {
            Ok(json!({ "Ability": { "abilityChn": [{ "ptzType": { "ver": 2 } }] } }))
        }
        Some("PtzCtrl") | Some("GetMdState") => Ok(json!({ "rspCode": 200, "state": 0 })),
        _ => Err("not support"),
    });

    let mut client = camera.connect().unwrap();
    client
        .ptz_move(1, PtzVelocity::new(-1.0, 0.0, 0.0))
        .unwrap();
    let features = client.features(1).unwrap();
    client.ptz_move(1, PtzVelocity::new(0.0, 0.5, 0.0)).unwrap();
    client.ptz_stop(1).unwrap();

    assert!(features.pan_tilt && features.zoom);
    let moves: Vec<Value> = camera
        .seen()
        .into_iter()
        .filter(|seen| seen.commands == ["PtzCtrl"])
        .map(|seen| seen.body[0]["param"].clone())
        .collect();
    assert_eq!(
        moves,
        [
            json!({ "channel": 0, "op": "Left" }),
            json!({ "channel": 0, "op": "Up", "speed": 32 }),
            json!({ "channel": 0, "op": "Stop" }),
        ]
    );
}

#[test]
fn a_zoom_the_camera_refuses_without_a_speed_is_sent_again_with_one() {
    let camera = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("GetAbility") => {
            Ok(json!({ "Ability": { "abilityChn": [{ "ptzType": { "ver": 1 } }] } }))
        }
        Some("GetMdState") => Ok(json!({ "state": 0 })),
        Some("PtzCtrl") => {
            let param = &command["param"];
            if param["op"] == "Stop" || param["speed"].is_number() {
                Ok(json!({ "rspCode": 200 }))
            } else {
                Err("param error")
            }
        }
        _ => Err("not support"),
    });

    let mut client = camera.connect().unwrap();
    assert!(client.features(1).unwrap().zoom);
    client.ptz_move(1, PtzVelocity::new(0.0, 0.0, 0.5)).unwrap();
    client
        .ptz_move(1, PtzVelocity::new(0.0, 0.0, -0.5))
        .unwrap();
    client.ptz_stop(1).unwrap();

    let moves: Vec<Value> = camera
        .seen()
        .into_iter()
        .filter(|seen| seen.commands == ["PtzCtrl"])
        .map(|seen| seen.body[0]["param"].clone())
        .collect();
    assert_eq!(
        moves,
        [
            json!({ "channel": 0, "op": "ZoomInc" }),
            json!({ "channel": 0, "op": "ZoomInc", "speed": 32 }),
            json!({ "channel": 0, "op": "ZoomDec", "speed": 32 }),
            json!({ "channel": 0, "op": "Stop" }),
        ],
        "refused once, then with a speed from then on"
    );
}

/// A command the fake lacks, which spoils a request of several.
const SPOILS_THE_REQUEST: &str = "not support";

#[test]
fn a_camera_that_answers_several_commands_with_one_reply_is_asked_one_at_a_time() {
    let camera = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("GetAbility") => {
            Ok(json!({ "Ability": { "abilityChn": [{ "ptzType": { "ver": 1 } }] } }))
        }
        Some("GetMdState") => Ok(json!({ "state": 0 })),
        Some("PtzCtrl") => Ok(json!({ "rspCode": 200 })),
        _ => Err(SPOILS_THE_REQUEST),
    });

    let mut client = camera.connect().unwrap();
    let features = client.features(1).unwrap();
    assert!(features.zoom && features.motion && features.detections.is_empty());
    client.ptz_move(1, PtzVelocity::new(0.0, 0.0, 1.0)).unwrap();

    let asked: Vec<Vec<String>> = camera
        .seen()
        .into_iter()
        .skip(1)
        .map(|seen| seen.commands)
        .collect();
    assert_eq!(
        asked,
        [
            vec!["GetAbility", "GetMdState", "GetAiState"],
            vec!["GetAbility"],
            vec!["GetMdState"],
            vec!["GetAiState"],
            vec!["PtzCtrl"],
        ],
        "together first, then each alone"
    );
}

#[test]
fn a_reolink_camera_is_told_apart_without_a_login() {
    let camera = FakeCamera::with_logins(|_, token| match token {
        None => Err("please login first"),
        Some(_) => Err("not support"),
    });
    assert!(ReolinkClient::answers("127.0.0.1", camera.port, false));
    assert_eq!(camera.commands(), ["GetDevInfo"], "no login");

    let other = FakeCamera::start(|_, _| Err("not support"));
    assert!(!ReolinkClient::answers("127.0.0.1", other.port, false));
    // Hangs up at once, as a closed port would, which the system takes seconds to refuse.
    let hangs_up = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = hangs_up.local_addr().unwrap().port();
    thread::spawn(move || hangs_up.incoming().for_each(drop));
    assert!(!ReolinkClient::answers("127.0.0.1", port, false));
}

#[test]
fn a_watch_learns_the_camera_polls_it_and_switches_its_spotlight() {
    let camera = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("GetAbility") => {
            Ok(json!({ "Ability": { "abilityChn": [{ "floodLight": { "ver": 1 } }] } }))
        }
        Some("GetDevInfo") => Ok(json!({ "DevInfo": { "model": "RLC-811A" } })),
        Some("GetMdState") => Ok(json!({ "state": 1 })),
        Some("GetWhiteLed") => Ok(json!({ "WhiteLed": { "state": 0, "bright": 100 } })),
        Some("SetWhiteLed") => Ok(json!({ "rspCode": 200 })),
        _ => Err("not support"),
    });
    let setting = CameraConfig {
        brand: Brand::Reolink,
        host: "127.0.0.1".into(),
        user: "admin".into(),
        password: PASSWORD.into(),
        api_port: camera.port,
        api_https: false,
        ..CameraConfig::default()
    };

    let watch = CameraWatch::start(&setting).expect("a Reolink camera with a password is watched");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !watch.watched().activity.motion {
        assert!(Instant::now() < deadline, "{:?}", watch.watched());
        thread::sleep(Duration::from_millis(20));
    }
    let watched = watch.watched();
    assert_eq!(watched.device.unwrap().model, "RLC-811A");
    assert!(watched.features.unwrap().spotlight);
    assert_eq!(
        watched.activity.spotlight,
        Some(Spotlight {
            on: false,
            brightness: 100
        })
    );

    let on = Spotlight {
        on: true,
        brightness: 100,
    };
    let answer = watch.set_spotlight(on);
    assert_eq!(answer.recv_timeout(Duration::from_secs(5)), Ok(Ok(())));
    assert_eq!(watch.watched().activity.spotlight, Some(on));
    assert_eq!(
        camera
            .commands()
            .iter()
            .filter(|command| *command == "Login")
            .count(),
        1,
        "one login for the whole watch"
    );
}

/// Reads the camera `IPCAMERA_REOLINK_HOST`, `_USER` and `_PASSWORD` name, changing nothing. Not
/// `IPCAMERA_*`'s camera: a Reolink login sent there would fail.
#[test]
#[ignore = "needs the owner's Reolink camera"]
fn the_real_camera_says_what_it_is_and_sees() {
    let variable = |name: &str| {
        std::env::var(format!("IPCAMERA_REOLINK_{name}"))
            .ok()
            .filter(|value| !value.is_empty())
    };
    let (Some(host), Some(user), Some(password)) =
        (variable("HOST"), variable("USER"), variable("PASSWORD"))
    else {
        // Never a blank or guessed login: without all three, nothing is sent.
        println!(
            "Set IPCAMERA_REOLINK_HOST, IPCAMERA_REOLINK_USER and IPCAMERA_REOLINK_PASSWORD to run this."
        );
        return;
    };
    let port = variable("PORT")
        .and_then(|port| port.parse().ok())
        .unwrap_or(443);
    let https = variable("HTTPS").is_none_or(|https| https != "0");

    let mut client = ReolinkClient::connect(&host, port, https, &user, &password).unwrap();
    println!("device:    {:?}", client.device_info().unwrap());
    println!("features:  {:?}", client.features(1).unwrap());
    println!("activity:  {:?}", client.activity(1, true).unwrap());
    match client.spotlight(1) {
        Ok(spotlight) => println!("spotlight: {spotlight:?}"),
        Err(error) => println!("spotlight: {error}"),
    }
}

#[test]
fn a_camera_that_says_where_its_zoom_is_can_be_zoomed_to_a_position() {
    let camera = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("GetZoomFocus") => Ok(json!({ "ZoomFocus": { "zoom": { "pos": 12 } } })),
        Some("StartZoomFocus") => Ok(json!({ "rspCode": 200 })),
        _ => Err("not support"),
    });
    let mut client = camera.connect().unwrap();

    assert_eq!(client.zoom_position(1).unwrap(), None);
    client.zoom_to(1, 20).unwrap();

    let sent = camera.seen().pop().unwrap();
    assert_eq!(
        sent.body[0]["param"],
        json!({ "ZoomFocus": { "channel": 0, "op": "ZoomPos", "pos": 20 } })
    );

    let without = FakeCamera::with_logins(|_, _| Err("not support"));
    let mut client = without.connect().unwrap();
    assert_eq!(
        client.zoom_position(1).unwrap(),
        None,
        "a camera without the command has no zoom position"
    );
}

#[test]
fn the_streams_offered_are_the_ones_the_camera_lists() {
    let camera = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("GetRtspUrl") => Ok(json!({ "rtspUrl": {
            "channel": 0,
            "mainStream": "rtsp://192.0.2.10:554/one",
            "subStream": "rtsp://192.0.2.10:554/two",
        } })),
        _ => Err("not support"),
    });
    let mut client = camera.connect().unwrap();
    let mut listed: Vec<(String, String)> = client
        .streams(1)
        .unwrap()
        .into_iter()
        .map(|stream| (stream.name, stream.path))
        .collect();
    listed.sort();
    assert_eq!(
        listed,
        [
            ("mainStream".to_string(), "/one".to_string()),
            ("subStream".to_string(), "/two".to_string()),
        ]
    );
    assert_eq!(
        camera.seen().pop().unwrap().body[0]["param"],
        json!({ "channel": 0 })
    );

    let silent = FakeCamera::with_logins(|_, _| Err("not support"));
    let mut client = silent.connect().unwrap();
    assert!(
        client.streams(1).unwrap().is_empty(),
        "a camera without the command lists none, and none is made up for it"
    );
}

fn flv_start(width: f64, height: f64, codec: u8) -> Vec<u8> {
    fn tag(kind: u8, body: &[u8]) -> Vec<u8> {
        let size = body.len() as u32;
        let mut tag = vec![kind];
        tag.extend_from_slice(&size.to_be_bytes()[1..]);
        tag.extend_from_slice(&[0; 7]);
        tag.extend_from_slice(body);
        tag.extend_from_slice(&(size + 11).to_be_bytes());
        tag
    }
    let mut script = vec![2, 0, 10];
    script.extend_from_slice(b"onMetaData");
    script.extend_from_slice(&[8, 0, 0, 0, 3]);
    for (key, value) in [
        ("width", width),
        ("height", height),
        ("videocodecid", f64::from(codec)),
    ] {
        script.extend_from_slice(&(key.len() as u16).to_be_bytes());
        script.extend_from_slice(key.as_bytes());
        script.push(0);
        script.extend_from_slice(&value.to_be_bytes());
    }
    script.extend_from_slice(&[0, 0, 9]);
    let mut bytes = b"FLV\x01\x00\x00\x00\x00\x09\x00\x00\x00\x00".to_vec();
    bytes.extend(tag(18, &script));
    bytes.extend(tag(9, &[0x10 | codec, 0, 0, 0, 0]));
    bytes
}

#[test]
fn the_streams_it_sends_only_as_flv_are_offered_by_their_own_names() {
    let camera = FakeCamera::with_logins_and_flv(
        |command, _| match command["cmd"].as_str() {
            Some("GetAbility") => Ok(
                json!({ "Ability": { "httpFlv": { "permit": 6, "ver": 1 }, "abilityChn": [{}] } }),
            ),
            Some("GetNetPort") => Ok(json!({ "NetPort": { "rtmpEnable": 1, "rtmpPort": 1936 } })),
            _ => Err("not support"),
        },
        |name| match name {
            "channel0_ext.bcs" => Some(flv_start(896.0, 672.0, 7)),
            "channel0_main.bcs" => Some(flv_start(3840.0, 2160.0, 12)),
            "channel0_autotrack_sub.bcs" => Some(flv_start(896.0, 512.0, 7)),
            // The same lens, answering to another lens's name too.
            "channel0_telephoto_sub.bcs" => Some(flv_start(896.0, 512.0, 7)),
            "channel0_telephoto_main.bcs" => Some(b"<html>not found</html>".to_vec()),
            _ => None,
        },
    );
    let mut client = camera.connect().unwrap();
    let rtsp = [CameraStream {
        name: "mainStream".into(),
        path: "/Preview_01_main".into(),
        detail: String::new(),
    }];

    let ext = CameraStream {
        name: "ext".into(),
        path: "flv:channel0_ext.bcs".into(),
        detail: "896×672, H264".into(),
    };
    let tracking = CameraStream {
        name: "autotrack_sub".into(),
        path: "flv:channel0_autotrack_sub.bcs".into(),
        detail: "896×512, H264".into(),
    };
    let offered = [ext, tracking];
    assert_eq!(client.flv_streams(1, &rtsp).unwrap(), offered);
    let asked: Vec<Seen> = camera
        .seen()
        .into_iter()
        .filter(|seen| seen.query.contains("app=bcs"))
        .collect();
    assert_eq!(asked.len(), 6, "each name but main, which RTSP has");
    for request in &asked {
        assert!(request.query.starts_with("port=1936&"), "{}", request.query);
        assert_eq!(request.token(), Some("tok1"));
        assert!(!request.query.contains(PASSWORD));
    }

    assert_eq!(client.flv_streams(1, &[]).unwrap(), offered);

    let url = client.flv_url("channel0_ext.bcs").unwrap();
    assert_eq!(
        url,
        format!(
            "http://127.0.0.1:{}/flv?port=1936&app=bcs&stream=channel0_ext.bcs&token=tok1",
            camera.port
        )
    );

    let plain = FakeCamera::with_logins(|command, _| match command["cmd"].as_str() {
        Some("GetAbility") => Ok(json!({ "Ability": {} })),
        _ => Err("not support"),
    });
    assert!(
        plain
            .connect()
            .unwrap()
            .flv_streams(1, &[])
            .unwrap()
            .is_empty(),
        "a camera that doesn't send FLV isn't asked for any"
    );
    assert!(
        plain
            .seen()
            .iter()
            .all(|seen| !seen.query.contains("app=bcs"))
    );
}
