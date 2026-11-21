//! Live video, snapshots and recordings against a pretend camera on this machine, never a real one.

#![cfg(windows)]

#[allow(dead_code)]
#[path = "support/server.rs"]
mod support;

use std::{net::TcpStream, time::Duration};

use ipcamera_core::CameraConfig;
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

use support::*;

#[test]
fn a_viewer_gets_the_set_up_then_a_keyframe() {
    let (server, ids) = serve(vec![pretend_camera()]);
    let mut socket = watch(server.port, ids[0].as_str());

    let first = next_json(&mut socket);
    assert_eq!(first["type"], "status", "the status comes at once: {first}");
    let config = loop {
        let message = next_json(&mut socket);
        if message["type"] == "config" {
            break message;
        }
        assert_ne!(message["state"], "failed", "{message}");
    };
    assert!(
        config["codec"]
            .as_str()
            .is_some_and(|codec| codec.starts_with("avc1.")),
        "{config}"
    );
    assert_eq!(
        (config["width"].as_u64(), config["height"].as_u64()),
        (Some(640), Some(360))
    );

    let picture = loop {
        match socket.read().expect("the pictures come") {
            Message::Binary(bytes) => break bytes,
            Message::Text(text) => assert!(!text.contains("failed"), "{text}"),
            _ => {}
        }
    };
    assert_eq!(picture[0] & 1, 1, "the first picture is a keyframe");
    assert!(
        picture[9..].starts_with(&[0, 0, 0, 1]) || picture[9..].starts_with(&[0, 0, 1]),
        "Annex-B after the 9-byte header"
    );
}

fn set_up(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> serde_json::Value {
    loop {
        let message = next_json(socket);
        if message["type"] == "config" {
            return message;
        }
        assert_ne!(message["state"], "failed", "{message}");
    }
}

#[test]
fn a_tile_watches_another_of_its_cameras_streams() {
    let camera = pretend_camera();
    let path = camera.rtsp_path_override.clone();
    let (server, ids) = serve(vec![camera]);
    let id = ids[0].as_str();

    let mut own = watch(server.port, id);
    let mut picked = watch_at(
        server.port,
        &format!(
            "/live/{id}?stream={}",
            ipcamera_core::api::path_segment(&path)
        ),
    );
    assert_eq!(set_up(&mut own)["width"], 640);
    assert_eq!(set_up(&mut picked)["width"], 640);

    let cameras = get(server.port, "/api/status", &[]).json()["cameras"].clone();
    let cameras = cameras.as_array().expect("a list");
    assert_eq!(cameras.len(), 1, "{cameras:?}");
    assert_eq!(cameras[0]["playing"], true, "{cameras:?}");
}

/// With a password, which the server needs before a snapshot, a recording or PTZ.
fn pretend_camera() -> CameraConfig {
    use ipcamera_media::pretend::{self, Pattern};

    CameraConfig {
        password: "test-only".into(),
        ..pretend::start(Pattern {
            width: 640,
            height: 360,
            fps: 25,
            frames: 1_500,
        })
        .expect("the pretend camera starts")
    }
}

#[test]
fn a_snapshot_is_taken_listed_served_and_deleted() {
    let (server, ids) = serve(vec![
        pretend_camera(),
        CameraConfig {
            password: String::new(),
            ..porch()
        },
    ]);
    let port = server.port;
    let before = get(port, "/api/status", &[]).json()["snapshots"].clone();

    let taken = post(
        port,
        "/api/snapshots",
        &serde_json::json!({ "cameras": [ids[0].as_str(), ids[1].as_str()] }).to_string(),
    );
    assert_eq!(taken.status, 200, "{}", taken.body);
    assert_eq!(
        taken.json()["count"],
        1,
        "the camera with no password is left out"
    );

    let listed = get(port, "/api/snapshots", &[]).json();
    assert_eq!(listed.as_array().map(Vec::len), Some(1), "{listed}");
    let name = listed[0]["name"].as_str().expect("its name").to_string();
    assert!(name.starts_with("snapshot-") && name.ends_with(&format!("-{}.png", ids[0])));
    assert!(server.data("snapshots").join(&name).is_file());
    assert_eq!(listed[0]["camera"], ids[0].as_str());
    assert_eq!(
        (
            listed[0]["size"]["width"].as_u64(),
            listed[0]["size"]["height"].as_u64()
        ),
        (Some(640), Some(360))
    );
    let url = listed[0]["url"].as_str().expect("where it is served");
    let file = get(port, url, &[]);
    assert_eq!(file.status, 200, "{}", file.head);
    assert!(
        file.head.to_lowercase().contains("content-type: image/png"),
        "{}",
        file.head
    );
    assert!(file.bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "a real PNG");
    assert_ne!(get(port, "/api/status", &[]).json()["snapshots"], before);
    let thumbnail = listed[0]["thumbnail"].as_str().expect("its thumbnail");
    assert_eq!(thumbnail, format!("/files/snapshots/thumbnails/{name}"));
    let small = get(port, thumbnail, &[]);
    assert_eq!(small.status, 200, "{}", small.head);
    assert!(
        small.bytes.starts_with(b"\x89PNG"),
        "the thumbnail is a PNG"
    );

    assert_eq!(
        get(port, "/files/snapshots/..%5Csettings.json", &[]).status,
        404
    );
    assert_eq!(get(port, "/files/snapshots/notes.txt", &[]).status, 404);
    assert_eq!(
        get(
            port,
            "/files/snapshots/thumbnails/..%5C..%5Csettings.json",
            &[]
        )
        .status,
        404
    );

    assert_eq!(delete(port, &format!("/api/snapshots/{name}")).status, 204);
    assert!(
        !server
            .data("snapshots")
            .join("thumbnails")
            .join(&name)
            .exists(),
        "its thumbnail went with it"
    );
    assert_eq!(
        get(port, "/api/snapshots", &[]).json(),
        serde_json::json!([])
    );
    let again = delete(port, &format!("/api/snapshots/{name}"));
    assert_eq!(again.status, 400);
    assert!(
        again.body.contains("no longer in the snapshots folder"),
        "{}",
        again.body
    );
}

#[test]
fn a_recording_starts_lists_serves_and_stops() {
    let (server, ids) = serve(vec![pretend_camera()]);
    let port = server.port;
    let id = ids[0].as_str();

    let started = post(
        port,
        "/api/recording",
        &serde_json::json!({ "cameras": [id] }).to_string(),
    );
    assert_eq!(started.status, 200, "{}", started.body);
    assert_eq!(started.json()["count"], 1);
    let status = get(port, "/api/status", &[]).json();
    assert_eq!(status["cameras"][0]["id"], id);
    assert_eq!(status["cameras"][0]["recording"], true);

    let mut video = serde_json::Value::Null;
    eventually(
        Duration::from_secs(15),
        "a recording with a thumbnail",
        || {
            video = get(port, "/api/videos", &[]).json()[0].clone();
            // The muxer writes the file's head with its first fragment, about a second in.
            video["recording"] == true
                && video["thumbnail"].is_string()
                && video["bytes"].as_u64().is_some_and(|bytes| bytes >= 100)
        },
    );
    let name = video["name"].as_str().expect("its name").to_string();
    assert_eq!(video["camera"], id);

    let refused = delete(port, &format!("/api/videos/{name}"));
    assert_eq!(refused.status, 409);
    assert_eq!(
        refused.body,
        "That video is still recording. Stop recording first."
    );

    let thumbnail = get(port, video["thumbnail"].as_str().unwrap(), &[]);
    assert_eq!(thumbnail.status, 200, "{}", thumbnail.head);
    assert!(
        thumbnail.bytes.starts_with(b"\x89PNG"),
        "the thumbnail is a PNG"
    );

    let part = get(
        port,
        video["url"].as_str().unwrap(),
        &[("Range", "bytes=0-99")],
    );
    assert_eq!(part.status, 206, "{}", part.head);
    assert_eq!(part.bytes.len(), 100, "{} / {video}", part.head);
    assert_eq!(&part.bytes[4..8], b"ftyp", "an MP4 from its first box");

    let stopped = delete(port, "/api/recording");
    assert_eq!(stopped.json()["count"], 1);
    eventually(Duration::from_secs(5), "the file finished", || {
        get(port, "/api/videos", &[]).json()[0]["recording"] == false
    });
    assert_eq!(
        get(port, "/api/status", &[]).json()["cameras"],
        serde_json::json!([{ "id": id, "playing": true, "recording": false }]),
        "its stream lingers, no longer recording"
    );
    assert_eq!(delete(port, &format!("/api/videos/{name}")).status, 204);
    assert!(
        !server
            .data("videos")
            .join("thumbnails")
            .join(name.replace(".mp4", ".png"))
            .exists()
    );
}
