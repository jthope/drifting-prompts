//! Who the server answers: the page and its script, the packaged web build at the root, the players
//! under this machine's other name, the this-machine-only check on the host and the origin, the
//! one origin allowed across origins, and a server's password.
//!
//! Each test serves a library of its own from a temporary folder, on a free port.

#[allow(dead_code)]
#[path = "support/server.rs"]
mod support;

use tempfile::TempDir;

use support::*;

#[test]
fn the_page_and_its_script_are_served() {
    let (server, _) = serve(Vec::new());
    for path in ["/", "/watch"] {
        let page = get(server.port, path, &[]);
        assert_eq!(page.status, 200, "{path}");
        assert!(page.body.contains(r#"<script src="/player.js""#), "{path}");
    }
    let script = get(server.port, "/player.js", &[]);
    assert_eq!(script.status, 200);
    assert!(
        script.head.to_lowercase().contains("text/javascript"),
        "{}",
        script.head
    );
}

#[test]
fn another_host_or_another_origin_is_turned_away() {
    let (server, _) = serve(vec![porch()]);
    let port = server.port;
    let elsewhere = get(port, "/api/cameras", &[("Host", "camera.example")]);
    assert_eq!(elsewhere.status, 403);
    assert!(!elsewhere.body.contains("Porch"));

    let other_page = get(port, "/api/cameras", &[("Origin", "http://camera.example")]);
    assert_eq!(other_page.status, 403);
    let own_page = get(
        port,
        "/api/cameras",
        &[("Origin", &format!("http://localhost:{port}"))],
    );
    assert_eq!(own_page.status, 200);
}

#[test]
fn the_allowed_origin_is_answered_across_origins_and_no_other() {
    let (server, _) = serve_with(
        vec![porch()],
        ipcamera_server::Options {
            allow_origins: vec!["http://127.0.0.1:8787".into()],
            ..ipcamera_server::Options::default()
        },
    );
    let preflight = |origin: &str| {
        send(
            server.port,
            "OPTIONS",
            "/api/changes",
            &[
                ("Origin", origin),
                ("Access-Control-Request-Method", "POST"),
                ("Access-Control-Request-Headers", "content-type"),
            ],
            "",
        )
    };
    let allowed = preflight("http://127.0.0.1:8787");
    assert_eq!(allowed.status, 200, "{}", allowed.head);
    assert!(
        allowed
            .head
            .to_lowercase()
            .contains("access-control-allow-origin: http://127.0.0.1:8787"),
        "{}",
        allowed.head
    );
    assert_eq!(preflight("http://camera.example").status, 403);

    let read = get(
        server.port,
        "/api/library",
        &[("Origin", "http://127.0.0.1:8787")],
    );
    assert_eq!(read.status, 200);
    assert!(read.body.contains("Porch"));
}

#[test]
fn the_packaged_app_is_served_at_the_root() {
    let app = TempDir::new().expect("a temporary folder");
    std::fs::write(app.path().join("index.html"), "<title>The app</title>").unwrap();
    std::fs::write(app.path().join("app.0123abcd.js"), "console.log(1)").unwrap();
    let (server, _) = serve_with(
        Vec::new(),
        ipcamera_server::Options {
            app: Some(app.path().to_path_buf()),
            ..ipcamera_server::Options::default()
        },
    );

    assert!(get(server.port, "/", &[]).body.contains("The app"));
    assert_eq!(
        get(server.port, "/app.0123abcd.js", &[]).body,
        "console.log(1)"
    );
    assert!(
        get(server.port, "/watch", &[]).body.contains("/player.js"),
        "the player is still there"
    );
    assert_eq!(get(server.port, "/../Cargo.toml", &[]).status, 404);
}

#[test]
fn players_come_from_this_machines_other_name() {
    let (server, _) = serve(Vec::new());
    let asked = |host: &str| {
        let answer = get(server.port, "/api/library", &[("Host", host)]);
        let library: serde_json::Value = serde_json::from_str(&answer.body).expect("JSON");
        library["player"]
            .as_str()
            .expect("a player address")
            .to_string()
    };
    let port = server.port;
    assert_eq!(
        asked(&format!("localhost:{port}")),
        format!("http://127.0.0.1:{port}")
    );
    assert_eq!(
        asked(&format!("127.0.0.1:{port}")),
        format!("http://localhost:{port}")
    );
}

#[test]
fn a_server_with_no_password_asks_for_none() {
    let (server, _) = serve(vec![porch()]);
    let session = get(server.port, "/api/session", &[]).json();
    assert_eq!(
        session,
        serde_json::json!({ "required": false, "signed_in": true })
    );
    assert_eq!(get(server.port, "/api/cameras", &[]).status, 200);
    let nothing = post(server.port, "/api/session", r#"{ "password": "anything" }"#);
    assert_eq!(nothing.status, 400, "{}", nothing.body);
}

#[test]
fn a_server_with_a_password_answers_only_those_who_signed_in() {
    let (server, ids) = serve_with_a_password(vec![porch()]);
    let port = server.port;
    let saved = std::fs::read_to_string(server.folder.path().join("settings.json")).unwrap();
    assert!(
        saved.contains("$argon2id$") && !saved.contains(PASSWORD),
        "the file holds a hash, never the password"
    );

    // Nothing that tells or changes anything answers, whichever way it is asked.
    for path in [
        "/api/cameras".to_string(),
        "/api/library".to_string(),
        "/api/status".to_string(),
        "/api/snapshots".to_string(),
        "/files/snapshots/a.png".to_string(),
        format!("/live/{}", ids[0]),
        "/api/cameras?key=made-up".to_string(),
    ] {
        let answer = get(port, &path, &[]);
        assert_eq!(
            (answer.status, answer.body.as_str()),
            (401, "Sign in to the server first."),
            "{path}"
        );
    }
    let change = post(port, "/api/changes", "{}");
    assert_eq!(change.status, 401);
    let made_up = get(port, "/api/cameras", &[("Authorization", "Bearer made-up")]);
    assert_eq!(made_up.status, 401);

    // The pages' own files do, and so does asking whether to sign in.
    assert_eq!(get(port, "/watch", &[]).status, 200);
    assert_eq!(get(port, "/player.js", &[]).status, 200);
    let session = get(port, "/api/session", &[]);
    assert_eq!(
        session.json(),
        serde_json::json!({ "required": true, "signed_in": false })
    );
    assert!(
        session
            .head
            .to_lowercase()
            .contains("referrer-policy: no-referrer"),
        "a key in an address goes no further: {}",
        session.head
    );

    let signed_in = post(
        port,
        "/api/session",
        &serde_json::json!({ "password": PASSWORD }).to_string(),
    );
    assert_eq!(signed_in.status, 200, "{}", signed_in.body);
    let key = signed_in.json()["key"].as_str().unwrap().to_string();
    let bearer = format!("Bearer {key}");

    let listed = get(port, "/api/cameras", &[("Authorization", &bearer)]);
    assert_eq!(listed.json()[0]["name"], "Porch");
    assert_eq!(
        get(port, &format!("/api/cameras?key={key}"), &[]).status,
        200,
        "as a player or a picture asks"
    );
    assert_eq!(
        get(port, "/api/session", &[("Authorization", &bearer)]).json(),
        serde_json::json!({ "required": true, "signed_in": true })
    );

    let out = send(
        port,
        "DELETE",
        "/api/session",
        &[("Authorization", &bearer)],
        "",
    );
    assert_eq!(out.status, 204);
    assert_eq!(
        get(port, "/api/cameras", &[("Authorization", &bearer)]).status,
        401,
        "the session is over"
    );

    let wrong = post(port, "/api/session", r#"{ "password": "a guess" }"#);
    assert_eq!(
        (wrong.status, wrong.body.as_str()),
        (401, "That isn't the server's password.")
    );
    let too_soon = post(
        port,
        "/api/session",
        &serde_json::json!({ "password": PASSWORD }).to_string(),
    );
    assert_eq!(
        too_soon.status, 429,
        "even the right one waits: {}",
        too_soon.body
    );
}
