//! What the server's tests share: a server of their own on a free local port, with a library and a
//! `settings.json` in a temporary folder, and plain HTTP and WebSocket requests to it, as a browser
//! makes them. Each test file takes it in with `#[path]`, so it is no test program of its own.

use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use ipcamera_core::{CameraConfig, CameraId, Library, config::ConfigStore};
use tempfile::TempDir;
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

/// A server on a free local port, stopped when this is dropped.
pub struct Running {
    /// The port it serves on.
    pub port: u16,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    /// The folder its `settings.json` and `data` are in.
    pub folder: TempDir,
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

impl Running {
    /// A folder inside the server's `data` folder: `settings`, `snapshots` or `videos`.
    pub fn data(&self, name: &str) -> PathBuf {
        self.folder.path().join("data").join(name)
    }
}

/// Serves `cameras` from a library of their own. Returns the server and the cameras' ids.
pub fn serve(cameras: Vec<CameraConfig>) -> (Running, Vec<CameraId>) {
    serve_with(cameras, ipcamera_server::Options::default())
}

/// Serves `cameras` from a library of their own, as `options` say.
pub fn serve_with(
    cameras: Vec<CameraConfig>,
    options: ipcamera_server::Options,
) -> (Running, Vec<CameraId>) {
    serve_set_up(cameras, options, |_| {})
}

/// A password of the tests' own, for a server that asks for one.
pub const PASSWORD: &str = "test-only";

/// Serves `cameras` from a server whose password is [`PASSWORD`].
pub fn serve_with_a_password(cameras: Vec<CameraConfig>) -> (Running, Vec<CameraId>) {
    serve_set_up(cameras, ipcamera_server::Options::default(), |config| {
        let hash = ipcamera_server::hash_password(PASSWORD).expect("it hashes");
        config
            .update(|config| config.server.password = hash)
            .expect("the settings save");
    })
}

/// Serves `cameras` from a library of their own, as `options` say, once `set_up` has changed the
/// server's `settings.json`.
pub fn serve_set_up(
    cameras: Vec<CameraConfig>,
    options: ipcamera_server::Options,
    set_up: impl FnOnce(&ConfigStore),
) -> (Running, Vec<CameraId>) {
    let folder = TempDir::new().expect("a temporary folder");
    let config = ConfigStore::load(folder.path().join("settings.json"));
    set_up(&config);
    let library = Library::open(config.settings_dir());
    let ids = cameras
        .into_iter()
        .map(|camera| library.add_camera(camera, None).expect("the camera saves"))
        .collect();

    let (port_sender, port) = mpsc::channel();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("the runtime starts");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("a local port is free");
            port_sender
                .send(listener.local_addr().expect("it has an address").port())
                .expect("the test waits for the port");
            ipcamera_server::serve(listener, library, config, options, async {
                let _ = stopped.await;
            })
            .await
            .expect("the server runs");
        });
    });

    let port = port
        .recv_timeout(Duration::from_secs(10))
        .expect("the server starts");
    let running = Running {
        port,
        stop: Some(stop),
        folder,
    };
    (running, ids)
}

/// What a request brought back: the status code, the head and the body, as text and as bytes.
pub struct Answer {
    /// The status code.
    pub status: u16,
    /// The head, as text.
    pub head: String,
    /// The body, as text.
    pub body: String,
    /// The body.
    pub bytes: Vec<u8>,
}

impl Answer {
    /// The body as JSON.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|_| panic!("JSON, not {} {}", self.status, self.body))
    }
}

/// Sends `DELETE path`.
pub fn delete(port: u16, path: &str) -> Answer {
    send(port, "DELETE", path, &[], "")
}

/// Asks `check` every 100 ms until it is true, for up to `within`.
pub fn eventually(within: Duration, what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !check() {
        assert!(Instant::now() < deadline, "{what} within {within:?}");
        thread::sleep(Duration::from_millis(100));
    }
}

/// Sends `GET path` with `headers`, naming this machine as the host unless `headers` name one.
pub fn get(port: u16, path: &str, headers: &[(&str, &str)]) -> Answer {
    send(port, "GET", path, headers, "")
}

/// Sends `POST path` with a JSON `body`.
pub fn post(port: u16, path: &str, body: &str) -> Answer {
    send(
        port,
        "POST",
        path,
        &[("Content-Type", "application/json")],
        body,
    )
}

/// Sends `method path` with `headers` and `body`, naming this machine as the host unless `headers`
/// name one.
pub fn send(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Answer {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("the server answers");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("host"))
    {
        request.push_str(&format!("Host: 127.0.0.1:{port}\r\n"));
    }
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();

    let split = response
        .windows(4)
        .position(|four| four == b"\r\n\r\n")
        .expect("a head and a body");
    let head = String::from_utf8_lossy(&response[..split]).into_owned();
    let bytes = response[split + 4..].to_vec();
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("a status code");
    Answer {
        status,
        head,
        body: String::from_utf8_lossy(&bytes).into_owned(),
        bytes,
    }
}

/// Opens camera `id`'s video as the player does, waiting up to 15 s for each message.
pub fn watch(port: u16, id: &str) -> WebSocket<MaybeTlsStream<TcpStream>> {
    watch_at(port, &format!("/live/{id}"))
}

/// Opens the video at `path` as the player does, waiting up to 15 s for each message.
pub fn watch_at(port: u16, path: &str) -> WebSocket<MaybeTlsStream<TcpStream>> {
    let (mut socket, _) =
        tungstenite::connect(format!("ws://127.0.0.1:{port}{path}")).expect("it upgrades");
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
    }
    socket
}

/// The next text message, as JSON.
pub fn next_json(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> serde_json::Value {
    loop {
        match socket.read().expect("the server keeps sending") {
            Message::Text(text) => return serde_json::from_str(&text).expect("JSON"),
            Message::Binary(_) => panic!("pictures before the set-up"),
            _ => {}
        }
    }
}

pub fn porch() -> CameraConfig {
    CameraConfig {
        name: "Porch".into(),
        host: "192.0.2.10".into(),
        user: "porch-admin".into(),
        password: "s3cret-pass".into(),
        ..CameraConfig::default()
    }
}

/// The library as the web build gets it.
pub fn library_of(port: u16) -> serde_json::Value {
    let answer = get(port, "/api/library", &[]);
    assert_eq!(answer.status, 200, "{}", answer.head);
    serde_json::from_str(&answer.body).expect("JSON")
}
