//! The DVRIP client against a fake camera on loopback, which checks the exact frames it sends.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use ipcamera_core::ptz::{DvripClient, DvripError};
use serde_json::{Value, json};

const HEADER: usize = 32;
const CHALLENGE: [u8; 2] = [0xa0, 0x01];
const LOGIN: [u8; 2] = [0xa0, 0x05];
const CODE: u8 = 0xf6;
const LOGIN_ACCEPTED: u16 = 0x0008;
const SESSION: u32 = 0x1234_5678;

#[derive(Debug, Clone, PartialEq)]
enum Seen {
    Challenge,
    Login(String),
    Call {
        method: String,
        object: Option<i64>,
        session: u32,
        params: Value,
    },
}

struct FakeCamera {
    port: u16,
    seen: Arc<Mutex<Vec<Seen>>>,
    worker: Option<JoinHandle<()>>,
}

impl FakeCamera {
    fn start(login_code: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the bound address").port();
        let seen = Arc::new(Mutex::new(Vec::new()));

        let worker = {
            let seen = seen.clone();
            thread::spawn(move || serve(listener, &seen, login_code))
        };

        Self {
            port,
            seen,
            worker: Some(worker),
        }
    }

    fn observed(&self) -> Vec<Seen> {
        self.seen
            .lock()
            .map(|seen| seen.clone())
            .unwrap_or_default()
    }
}

impl Drop for FakeCamera {
    fn drop(&mut self) {
        // The client has closed the socket by now, so the worker's read has returned.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(listener: TcpListener, seen: &Mutex<Vec<Seen>>, login_code: u16) {
    let Ok((mut stream, _)) = listener.accept() else {
        return;
    };
    stream.set_nodelay(true).ok();
    // A stalled read ends the test instead of hanging it.
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();

    while let Some((header, body)) = read_frame(&mut stream) {
        match [header[0], header[1]] {
            CHALLENGE => {
                record(seen, Seen::Challenge);
                let text = "Realm:LoginToMoreWide\r\nRandom:1234567890\r\n";
                write_frame(&mut stream, None, text.as_bytes());
            }
            LOGIN => {
                record(
                    seen,
                    Seen::Login(String::from_utf8_lossy(&body).into_owned()),
                );
                write_login(&mut stream, login_code);
            }
            [CODE, _] => {
                let Ok(request) = serde_json::from_slice::<Value>(&body) else {
                    return;
                };
                let method = request["method"].as_str().unwrap_or_default().to_string();
                let id = request["id"].as_u64().unwrap_or_default();

                record(
                    seen,
                    Seen::Call {
                        method: method.clone(),
                        object: request["object"].as_i64(),
                        session: u32::from_le_bytes([
                            header[24], header[25], header[26], header[27],
                        ]),
                        params: request["params"].clone(),
                    },
                );

                let reply = match method.as_str() {
                    "ptz.factory.instance" => json!({ "id": id, "result": 42 }),
                    "ptz.getCurrentProtocolCaps" => json!({
                        "id": id,
                        "result": true,
                        "params": { "caps": { "Pan": true, "Tile": true, "Zoom": true } },
                    }),
                    _ => json!({ "id": id, "result": true }),
                };

                write_frame(&mut stream, Some(id), reply.to_string().as_bytes());
            }
            _ => return,
        }
    }
}

fn record(seen: &Mutex<Vec<Seen>>, entry: Seen) {
    if let Ok(mut seen) = seen.lock() {
        seen.push(entry);
    }
}

fn read_frame(stream: &mut TcpStream) -> Option<([u8; HEADER], Vec<u8>)> {
    let mut header = [0u8; HEADER];
    stream.read_exact(&mut header).ok()?;

    let length = i32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    if !(0..=1024 * 1024).contains(&length) {
        return None;
    }

    let mut body = vec![0u8; length as usize];
    stream.read_exact(&mut body).ok()?;
    Some((header, body))
}

fn write_frame(stream: &mut TcpStream, id: Option<u64>, body: &[u8]) {
    let mut header = [0u8; HEADER];
    header[0] = CODE;
    header[4..8].copy_from_slice(&(body.len() as i32).to_le_bytes());
    if let Some(id) = id {
        header[8..12].copy_from_slice(&(id as u32).to_le_bytes());
    }
    header[16..20].copy_from_slice(&(body.len() as i32).to_le_bytes());

    stream.write_all(&header).ok();
    stream.write_all(body).ok();
    stream.flush().ok();
}

fn write_login(stream: &mut TcpStream, code: u16) {
    let mut header = [0u8; HEADER];
    header[0] = CODE;
    header[8..10].copy_from_slice(&code.to_be_bytes());
    header[16..20].copy_from_slice(&SESSION.to_le_bytes());

    stream.write_all(&header).ok();
    stream.flush().ok();
}

#[test]
fn logs_in_and_carries_the_session_on_every_call() {
    let camera = FakeCamera::start(LOGIN_ACCEPTED);
    let mut client = DvripClient::connect("127.0.0.1", camera.port, "admin", "secret")
        .expect("the fake camera accepts the login");

    let reply = client
        .call("ptz.factory.instance", Some(json!({ "channel": 0 })), None)
        .expect("the object handle should come back");
    assert_eq!(reply["result"], 42);

    let reply = client
        .call(
            "ptz.start",
            Some(json!({ "code": "Right", "arg1": 0, "arg2": 8, "arg3": 0 })),
            Some(42),
        )
        .expect("the move should be accepted");
    assert_eq!(reply["result"], true);
    drop(client);

    let seen = camera.observed();
    assert_eq!(seen[0], Seen::Challenge);

    match &seen[1] {
        Seen::Login(hash) => {
            assert!(hash.starts_with("admin&&"), "unexpected login text: {hash}");
            assert_eq!(hash.len(), "admin&&".len() + 32 + 32);
        }
        other => panic!("expected the login, got {other:?}"),
    }

    match &seen[2] {
        Seen::Call {
            method,
            object,
            session,
            params,
        } => {
            assert_eq!(method, "ptz.factory.instance");
            assert_eq!(*object, None);
            assert_eq!(*session, SESSION, "every call must carry the session id");
            assert_eq!(params["channel"], 0);
        }
        other => panic!("expected the object call, got {other:?}"),
    }

    match &seen[3] {
        Seen::Call {
            method,
            object,
            params,
            ..
        } => {
            assert_eq!(method, "ptz.start");
            assert_eq!(*object, Some(42), "a move targets the PTZ object");
            assert_eq!(params["code"], "Right");
            assert_eq!(params["arg2"], 8);
            assert_eq!(params["arg3"], 0);
        }
        other => panic!("expected the move, got {other:?}"),
    }
}

#[test]
fn a_refused_login_is_one_attempt_and_a_typed_error() {
    // "Wrong username or password", with the tries left.
    let camera = FakeCamera::start(0x0100);
    let Err(error) = DvripClient::connect("127.0.0.1", camera.port, "admin", "wrong") else {
        panic!("the fake camera refuses this login");
    };

    let DvripError::LoginRejected(rejected) = error else {
        panic!("a refusal must stay typed, or PTZ cannot tell a lockout from a dead port: {error}");
    };
    assert!(
        rejected.to_string().contains("tries left"),
        "the refusal should say how many tries remain: {rejected}"
    );

    let logins = camera
        .observed()
        .into_iter()
        .filter(|entry| matches!(entry, Seen::Login(_)))
        .count();
    assert_eq!(logins, 1, "a second attempt could lock the account");
}
