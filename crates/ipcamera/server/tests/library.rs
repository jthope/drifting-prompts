//! The library over HTTP, as the web build reads and changes it; no test reaches a camera.

#[allow(dead_code)]
#[path = "support/server.rs"]
mod support;

use ipcamera_core::{CameraConfig, Library};

use support::*;

#[test]
fn the_camera_list_names_cameras_and_nothing_else() {
    let (server, ids) = serve(vec![porch()]);
    let answer = get(server.port, "/api/cameras", &[]);
    assert_eq!(answer.status, 200, "{}", answer.head);

    let cameras: serde_json::Value = serde_json::from_str(&answer.body).expect("JSON");
    assert_eq!(
        cameras,
        serde_json::json!([{ "id": ids[0].as_str(), "name": "Porch" }])
    );
    for secret in ["s3cret-pass", "porch-admin", "192.0.2.10"] {
        assert!(!answer.body.contains(secret), "{secret} left the server");
    }
}

#[test]
fn an_unknown_camera_is_not_found() {
    let (server, _) = serve(vec![porch()]);
    match tungstenite::connect(format!("ws://127.0.0.1:{}/live/nope", server.port)) {
        Err(tungstenite::Error::Http(response)) => assert_eq!(response.status(), 404),
        Err(other) => panic!("a 404, not {other}"),
        Ok(_) => panic!("there is no camera to watch"),
    }
}

#[test]
fn settings_that_reach_no_camera_say_why_without_trying() {
    // No username, so the session never starts and nothing is sent anywhere.
    let unnamed = CameraConfig {
        user: String::new(),
        ..porch()
    };
    let (server, ids) = serve(vec![unnamed]);
    let mut socket = watch(server.port, ids[0].as_str());
    let status = next_json(&mut socket);
    assert_eq!(status["type"], "status");
    assert_eq!(status["state"], "failed");
    assert!(
        status["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("Enter a username.")),
        "{status}"
    );
}

#[test]
fn the_library_goes_out_without_passwords() {
    let (server, ids) = serve(vec![porch()]);
    let answer = get(server.port, "/api/library", &[]);
    assert!(
        !answer.body.contains("s3cret-pass"),
        "the password left the server"
    );

    let library = library_of(server.port);
    let camera = &library["snapshot"]["cameras"][0];
    assert_eq!(camera["name"], "Porch");
    assert_eq!(
        camera["host"], "192.0.2.10",
        "the web build edits the address"
    );
    assert_eq!(camera["password"], "");
    assert_eq!(
        library["with_password"],
        serde_json::json!([ids[0].as_str()])
    );
    assert_eq!(library["snapshot"]["views"][0]["name"], "Default");
}

#[test]
fn a_change_is_made_and_a_bad_one_refused_in_the_apps_words() {
    let (server, ids) = serve(vec![porch()]);
    let port = server.port;
    let revision = |port| {
        get(port, "/api/status", &[]).json()["revision"]
            .as_u64()
            .expect("a number")
    };
    let before = revision(port);

    let made = post(
        port,
        "/api/changes",
        &serde_json::json!({
            "change": "add_group",
            "name": "Outside",
            "cameras": [ids[0].as_str()],
        })
        .to_string(),
    );
    assert_eq!(made.status, 200, "{}", made.body);
    let added: serde_json::Value = serde_json::from_str(&made.body).expect("JSON");
    let group = added["added"]["group"]
        .as_str()
        .expect("the new group's id");
    assert!(revision(port) > before, "the revision went up");
    assert_eq!(library_of(port)["snapshot"]["groups"][0]["id"], group);

    let refused = post(
        port,
        "/api/changes",
        &serde_json::json!({
            "change": "add_camera",
            "camera": { "name": "Yard", "user": "admin" },
        })
        .to_string(),
    );
    assert_eq!(refused.status, 400);
    assert_eq!(refused.body, "Enter the camera's address.");

    let unknown = post(
        port,
        "/api/changes",
        &serde_json::json!({ "change": "rename_view", "id": "gone", "name": "Night" }).to_string(),
    );
    assert_eq!(unknown.status, 400);
    assert_eq!(unknown.body, "no view `gone`");
}

#[test]
fn an_update_without_the_password_keeps_the_saved_one() {
    let (server, ids) = serve(vec![porch()]);
    let update = serde_json::json!({
        "change": "update_cameras",
        "cameras": [{
            "id": ids[0].as_str(),
            "camera": {
                "name": "Front porch",
                "host": "192.0.2.10",
                "user": "porch-admin",
                "password": "",
            },
            "keep_password": true,
        }],
    });
    let made = post(server.port, "/api/changes", &update.to_string());
    assert_eq!(made.status, 200, "{}", made.body);

    let saved = Library::open(server.data("settings"))
        .camera(&ids[0])
        .expect("still there");
    assert_eq!(saved.name, "Front porch");
    assert_eq!(saved.password, "s3cret-pass", "the saved password stays");
}

#[test]
fn a_camera_with_no_password_is_never_tried() {
    let (server, ids) = serve(vec![CameraConfig {
        password: String::new(),
        ..porch()
    }]);
    let port = server.port;
    let id = ids[0].as_str();
    let why = "Add the camera's password in Settings first.";

    let snapshot = post(
        port,
        "/api/snapshots",
        &serde_json::json!({ "cameras": [id] }).to_string(),
    );
    assert_eq!((snapshot.status, snapshot.body.as_str()), (400, why));
    let record = post(
        port,
        "/api/recording",
        &serde_json::json!({ "cameras": [id] }).to_string(),
    );
    assert_eq!((record.status, record.body.as_str()), (400, why));
    let moved = post(
        port,
        &format!("/api/cameras/{id}/ptz"),
        r#"{ "pan": 1, "tilt": 0, "zoom": 0, "speed": 0.5 }"#,
    );
    assert_eq!((moved.status, moved.body.as_str()), (400, why));
    let mut socket = watch(port, id);
    let status = next_json(&mut socket);
    assert_eq!(
        (status["state"].as_str(), status["reason"].as_str()),
        (Some("failed"), Some(why)),
        "{status}"
    );
    assert_eq!(
        get(port, "/api/status", &[]).json()["cameras"],
        serde_json::json!([])
    );

    let unknown = post(
        port,
        "/api/snapshots",
        &serde_json::json!({ "cameras": ["gone"] }).to_string(),
    );
    assert_eq!(
        (unknown.status, unknown.body.as_str()),
        (400, "There is no camera with that id. Reload the page.")
    );
    let none = post(port, "/api/snapshots", r#"{ "cameras": [] }"#);
    assert_eq!(
        (none.status, none.body.as_str()),
        (400, "There is no camera on screen to take a snapshot of.")
    );
}

#[test]
fn pressing_connect_lets_a_camera_with_no_password_be_tried() {
    // Nothing listens at its address on this machine, so the try reaches no camera.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let rtsp_port = closed.local_addr().unwrap().port();
    drop(closed);
    let (server, ids) = serve(vec![CameraConfig {
        host: "127.0.0.1".into(),
        rtsp_port,
        password: String::new(),
        ..porch()
    }]);
    let port = server.port;
    let id = ids[0].as_str();
    let why = "Add the camera's password in Settings first.";

    let mut refused = watch(port, id);
    assert_eq!(next_json(&mut refused)["reason"].as_str(), Some(why));

    let pressed = post(port, &format!("/api/cameras/{id}/connect"), "");
    assert_eq!(pressed.status, 204, "{}", pressed.body);
    let mut tried = watch(port, id);
    let status = next_json(&mut tried);
    assert_ne!(status["reason"].as_str(), Some(why), "{status}");
    let listed = get(port, "/api/status", &[]).json();
    assert_eq!(
        listed["cameras"][0]["id"].as_str(),
        Some(id),
        "it has a stream now, whatever became of it: {listed}"
    );

    let gone = post(port, "/api/cameras/gone/connect", "");
    assert_eq!(gone.status, 404);
}

#[test]
fn ptz_to_a_camera_without_it_says_so() {
    let (server, ids) = serve(vec![CameraConfig {
        host: "127.0.0.1".into(),
        dvrip_port: 1,
        ..porch()
    }]);
    let port = server.port;
    let id = ids[0].as_str();

    let moved = post(
        port,
        &format!("/api/cameras/{id}/ptz"),
        r#"{ "pan": 1, "tilt": 0, "zoom": 0, "speed": 0.5 }"#,
    );
    assert_eq!(moved.status, 204, "{}", moved.body);
    let status = get(port, "/api/status", &[]).json();
    assert_eq!(status["cameras"][0]["id"], id);
    assert!(
        status["cameras"][0]["ptz_status"]
            .as_str()
            .is_some_and(|said| said.starts_with("PTZ")),
        "{status}"
    );
    assert_eq!(
        post(port, &format!("/api/cameras/{id}/ptz/stop"), "").status,
        204
    );
}

#[test]
fn the_folders_and_their_drives_are_told() {
    let (server, _) = serve(Vec::new());
    let storage = get(server.port, "/api/storage", &[]).json();
    for (kind, folder) in [("snapshots", "snapshots"), ("videos", "videos")] {
        let entry = &storage[kind];
        assert_eq!(entry["is_default"], true, "{storage}");
        assert!(
            entry["path"]
                .as_str()
                .is_some_and(|path| path.ends_with(folder)),
            "{storage}"
        );
        assert!(
            entry["total"].as_u64().is_some_and(|total| total > 0),
            "{storage}"
        );
        assert!(entry.get("problem").is_none(), "{storage}");
    }
    assert_eq!(
        get(server.port, "/api/videos", &[]).json(),
        serde_json::json!([])
    );
    assert_eq!(
        get(server.port, "/files/videos/video-1.mp4", &[]).status,
        404
    );
}
