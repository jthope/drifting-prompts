//! Tests of the web build's backend, held to the desktop's contract.

use super::*;
use ipcamera_core::Snapshot;

fn state(id: &str) -> Result<CameraState, String> {
    Ok(state_of(&camera(id)?))
}

fn filled(name: &str) -> CameraForm {
    CameraForm {
        name: name.to_string(),
        host: "10.0.0.7".to_string(),
        ..blank_form()
    }
}

/// Reads the revision without starting the server check, which needs nsh's engine.
fn page_revision() -> u64 {
    WEB.with(|web| web.borrow().revision())
}

fn first() -> String {
    list()[0].id.clone()
}

fn add_here(form: &CameraForm) -> String {
    make_here(&Change::AddCamera {
        camera: form::to_config(form),
    })
    .unwrap()
    .added_id()
}

fn save_many_here(ids: &[String], changes: &CameraChanges) -> Result<u32, String> {
    let Some((change, changed)) = with_library(|saved| library::save_changes(saved, ids, changes))?
    else {
        return Ok(0);
    };
    make_here(&change)?;
    Ok(changed.len() as u32)
}

fn served_by(address: &str, cameras: Vec<CameraConfig>) -> Vec<String> {
    let saved = Library::in_memory(cameras);
    let ids = saved
        .cameras()
        .iter()
        .map(|camera| camera.id.to_string())
        .collect();
    let copy = saved.snapshot().for_client();
    let player = format!("{address}-player");
    WEB.with(|web| web.borrow_mut().take(address.to_string(), copy, player));
    ids
}

fn porch() -> CameraConfig {
    CameraConfig {
        name: "Porch".into(),
        host: "10.0.0.7".into(),
        user: "admin".into(),
        password: "s3cret".into(),
        ..CameraConfig::default()
    }
}

#[test]
fn saving_the_form_renames_the_camera_and_shows_its_address() {
    let id = first();
    make_here(&camera_update(&id, &filled("Driveway"))).unwrap();

    let camera = state(&id).unwrap();
    assert_eq!(camera.name, "Driveway");
    assert_eq!(camera.stream_label, "admin@10.0.0.7:554  ·  ch1 main");
    assert_eq!(list()[0].name, "Driveway");
    assert_eq!(
        form(&id).unwrap().host,
        "10.0.0.7",
        "the form opens as it was saved"
    );
    assert!(!camera.ready, "nothing connects in the preview");
    assert!(!camera.connected);
    assert_eq!(live_url(&id, ""), "", "the preview has no player");
}

#[test]
fn a_camera_with_no_address_says_so() {
    assert_eq!(list()[0].name, NAME);
    assert_eq!(state(&first()).unwrap().stream_label, NO_ADDRESS);
}

#[test]
fn the_preview_refuses_the_forms_the_desktop_refuses() {
    let mut form = filled("Porch");
    form.rtsp_port = "70000".to_string();

    let fields: Vec<_> = form_problems(&form)
        .into_iter()
        .map(|problem| problem.field)
        .collect();

    assert_eq!(fields, ["rtspPort"]);
    assert_eq!(
        make_here(&Change::AddCamera {
            camera: form::to_config(&CameraForm {
                host: String::new(),
                ..filled("Porch")
            }),
        })
        .unwrap_err(),
        "Enter the camera's address.",
        "the library refuses what the form would"
    );
    assert_eq!(states().len(), 1, "nothing was added");
}

#[test]
fn an_added_camera_lists_and_a_deleted_one_leaves_every_group_and_tile() {
    let first = first();
    let id = add_here(&filled("Porch"));
    assert_eq!(
        states()
            .iter()
            .map(|camera| camera.name.as_str())
            .collect::<Vec<_>>(),
        [NAME, "Porch"]
    );

    let group = make_here(&library::add_group("Outside", &[]))
        .unwrap()
        .added_id();
    make_here(&library::add_to_group(&group, &id)).unwrap();
    let view = with_library(library::views)[0].id.clone();
    make_here(&library::set_tile(&view, 1, &id, "sub")).unwrap();
    let before = page_revision();

    make_here(&Change::DeleteCamera {
        id: CameraId::from(id.as_str()),
    })
    .unwrap();

    assert!(state(&id).is_err());
    with_library(|saved| {
        assert!(library::groups(saved)[0].cameras.is_empty());
        assert_eq!(
            library::views(saved)[0].tiles[..2],
            [first, String::new()],
            "its tile is empty, the others as they were"
        );
    });
    assert!(
        page_revision() > before,
        "the sidebar and the stage hear about it"
    );
}

#[test]
fn a_moved_camera_lists_in_its_new_place() {
    let first = first();
    let porch = add_here(&filled("Porch"));
    let before = page_revision();

    make_here(&library::move_camera(&porch, 0)).unwrap();

    let order: Vec<_> = states().into_iter().map(|camera| camera.id).collect();
    assert_eq!(order, [porch, first]);
    assert!(page_revision() > before, "the sidebar hears about it");
}

#[test]
fn changes_reach_the_cameras_named_as_on_the_desktop() {
    let porch = add_here(&filled("Porch"));
    let yard = add_here(&filled("Yard"));
    let changes = CameraChanges {
        host: Some("10.0.0.20".to_string()),
        ..CameraChanges::default()
    };

    assert_eq!(
        save_many_here(&[porch.clone(), yard.clone()], &changes),
        Ok(2)
    );

    for id in [&porch, &yard] {
        assert_eq!(form(id).unwrap().host, "10.0.0.20");
    }
    assert_eq!(form(&porch).unwrap().name, "Porch", "names stay their own");
    assert_eq!(
        form(&first()).unwrap().host,
        "",
        "a camera not named is left alone"
    );
    assert_eq!(
        save_many_here(&[porch], &changes),
        Ok(0),
        "nothing left to change"
    );
}

#[test]
fn changes_the_desktop_refuses_change_nothing() {
    let porch = add_here(&filled("Porch"));
    let before = page_revision();

    let bad = CameraChanges {
        dvrip_port: Some("0".to_string()),
        ..CameraChanges::default()
    };
    assert_eq!(
        save_many_here(std::slice::from_ref(&porch), &bad),
        Err("Use a port from 1 to 65535.".to_string())
    );
    let good = CameraChanges {
        user: Some("viewer".to_string()),
        ..CameraChanges::default()
    };
    assert!(save_many_here(&[porch.clone(), "gone".to_string()], &good).is_err());

    assert_eq!(page_revision(), before);
    assert_eq!(form(&porch).unwrap().user, "admin");
}

#[test]
fn the_servers_copy_replaces_the_preview_and_plays_through_its_player() {
    let before = page_revision();
    let ids = served_by(
        "http://127.0.0.1:8770",
        vec![
            porch(),
            CameraConfig {
                name: "Yard".into(),
                password: String::new(),
                ..porch()
            },
        ],
    );
    assert!(page_revision() > before, "the page hears about the copy");
    assert_eq!(list().len(), 2, "the preview camera is gone");

    let shown = state(&ids[0]).unwrap();
    assert!(shown.connected, "the player shows the picture, not the app");
    assert_eq!(shown.status, "Plays through the server.");
    assert_eq!(
        live_url(&ids[0], ""),
        format!("http://127.0.0.1:8770-player/watch?camera={}", ids[0])
    );
    assert_eq!(
        live_url(&ids[0], "/cam/realmonitor?channel=1&subtype=1"),
        format!(
            "http://127.0.0.1:8770-player/watch?camera={}&stream=%2Fcam%2Frealmonitor%3Fchannel%3D1%26subtype%3D1",
            ids[0]
        ),
        "a tile's stream goes escaped"
    );
    assert_eq!(live_url("gone", ""), "");
}

#[test]
fn the_server_keeps_the_password_the_web_was_never_told() {
    let ids = served_by(
        "",
        vec![
            porch(),
            CameraConfig {
                name: "Yard".into(),
                password: String::new(),
                ..porch()
            },
        ],
    );
    let porch_form = form(&ids[0]).unwrap();
    assert_eq!(porch_form.password, "", "the copy holds no password");
    assert_eq!(porch_form.password_note, PASSWORD_KEPT);
    assert_eq!(
        form(&ids[1]).unwrap().password_note,
        "",
        "nothing saved to keep"
    );

    let keeps = |change: Change| match change {
        Change::UpdateCameras { cameras } => cameras[0].keep_password,
        other => panic!("an update, not {other:?}"),
    };
    assert!(
        keeps(camera_update(&ids[0], &porch_form)),
        "left empty, it stays"
    );
    assert!(!keeps(camera_update(
        &ids[0],
        &CameraForm {
            password: "n3w".into(),
            ..porch_form.clone()
        }
    )));
    assert!(!keeps(camera_update(&ids[1], &form(&ids[1]).unwrap())));
    assert_eq!(
        live_url(&ids[0], ""),
        format!("-player/watch?camera={}", ids[0]),
        "from where the server says"
    );
}

#[test]
fn the_servers_status_reaches_each_camera() {
    let ids = served_by("http://127.0.0.1:8770", vec![porch(), porch()]);
    WEB.with(|web| {
        web.borrow_mut().status = Status {
            revision: 1,
            snapshots: 4,
            videos: 9,
            cameras: vec![CameraStatus {
                id: CameraId::from(ids[0].as_str()),
                playing: true,
                stream_problem: None,
                recording: true,
                record_problem: None,
                ptz_status: Some("PTZ: ready".into()),
                watch: Some(api::WatchStatus {
                    motion: true,
                    detected: vec![ipcamera_core::Detection::Vehicle],
                    spotlight: true,
                    spotlight_on: true,
                    summary: "RLC-811A: Motion, Vehicle, Spotlight.".into(),
                }),
            }],
        }
    });

    let recording = state(&ids[0]).unwrap();
    assert!(recording.recording);
    assert_eq!(recording.ptz_status, "PTZ: ready");
    assert!(recording.motion && recording.spotlight && recording.spotlight_on);
    assert_eq!(recording.detected, ["vehicle"]);
    assert_eq!(
        recording.watch_status,
        "RLC-811A: Motion, Vehicle, Spotlight."
    );
    let idle = state(&ids[1]).unwrap();
    assert!(!idle.recording);
    assert_eq!(idle.ptz_status, "PTZ: not checked yet");
    assert!(!idle.motion && !idle.spotlight && idle.detected.is_empty());
    assert_eq!(idle.watch_status, "", "not watched");
    assert_eq!(WEB.with(|web| web.borrow().status.videos), 9);

    WEB.with(|web| {
        web.borrow_mut()
            .ptz_refused
            .insert(CameraId::from(ids[1].as_str()), "Add the password.".into())
    });
    assert_eq!(state(&ids[1]).unwrap().ptz_status, "Add the password.");
}

#[test]
fn a_server_that_stops_answering_is_said_to_be_out_of_reach_until_it_answers() {
    let ids = served_by("http://127.0.0.1:8770", vec![porch()]);
    WEB.with(|web| {
        web.borrow_mut().status.cameras = vec![CameraStatus {
            id: CameraId::from(ids[0].as_str()),
            recording: true,
            ..CameraStatus::default()
        }]
    });
    let missed = || WEB.with(|web| web.borrow_mut().checked(false));

    missed();
    missed();
    let still = state(&ids[0]).unwrap();
    assert!(
        still.connected && still.recording,
        "a missed check or two changes nothing"
    );
    assert_eq!(still.tone, StatusTone::Normal);

    missed();
    let gone = state(&ids[0]).unwrap();
    assert!(!gone.connected && !gone.recording);
    assert_eq!(gone.status, OUT_OF_REACH);
    assert_eq!(gone.tone, StatusTone::Warn);
    assert!(
        gone.ready,
        "it still has its password, for when the server is back"
    );

    WEB.with(|web| web.borrow_mut().checked(true));
    let back = state(&ids[0]).unwrap();
    assert!(back.connected && back.recording, "as the server last said");
    assert_eq!(back.status, "Plays through the server.");
}

#[test]
fn a_camera_whose_stream_gave_up_says_why_and_offers_connect() {
    let ids = served_by("http://127.0.0.1:8770", vec![porch()]);
    let said = |status: CameraStatus| {
        WEB.with(|web| web.borrow_mut().status.cameras = vec![status]);
        state(&ids[0]).unwrap()
    };

    let refused = said(CameraStatus {
        id: CameraId::from(ids[0].as_str()),
        stream_problem: Some("The camera refused the login.".into()),
        ..CameraStatus::default()
    });
    assert!(!refused.connected, "so the tile offers Connect");
    assert!(
        refused.ready,
        "it has a password: Connect, not its settings"
    );
    assert_eq!(refused.status, "The camera refused the login.");
    assert_eq!(refused.tone, StatusTone::Warn);

    let playing = said(CameraStatus {
        id: CameraId::from(ids[0].as_str()),
        playing: true,
        ..CameraStatus::default()
    });
    assert!(playing.connected);
    assert_eq!(playing.status, "Plays through the server.");
}

#[test]
fn a_camera_with_no_saved_password_waits_until_someone_connects_it() {
    let ids = served_by(
        "http://127.0.0.1:8770",
        vec![CameraConfig {
            password: String::new(),
            ..porch()
        }],
    );

    let waiting = state(&ids[0]).unwrap();
    assert!(!waiting.connected && !waiting.ready);
    assert_eq!(waiting.status, NO_PASSWORD);

    WEB.with(|web| {
        web.borrow_mut().status.cameras = vec![CameraStatus {
            id: CameraId::from(ids[0].as_str()),
            playing: true,
            ..CameraStatus::default()
        }]
    });
    let playing = state(&ids[0]).unwrap();
    assert!(playing.connected);
    assert_eq!(playing.status, "Plays through the server.");
}

#[test]
fn a_sessions_key_goes_with_what_the_browser_fetches_itself() {
    let ids = served_by("http://127.0.0.1:8770", vec![porch()]);
    assert!(!signed_in());
    assert_eq!(keyed("/files/a.png".into()), "/files/a.png", "none yet");

    WEB.with(|web| web.borrow_mut().key = Some("abc123".into()));
    assert!(signed_in());
    assert_eq!(
        live_url(&ids[0], "sub"),
        format!(
            "http://127.0.0.1:8770-player/watch?camera={}&stream=sub&key=abc123",
            ids[0]
        )
    );
    assert_eq!(
        served("http://127.0.0.1:8770", Some("/files/snapshots/a.png")),
        "http://127.0.0.1:8770/files/snapshots/a.png?key=abc123"
    );
    assert_eq!(served("http://127.0.0.1:8770", None), "");
}

#[test]
fn a_server_that_turns_the_page_away_is_signed_in_to_again() {
    served_by("http://127.0.0.1:8770", vec![porch()]);
    let needed = || WEB.with(|web| web.borrow().sign_in_needed);
    assert!(!needed());

    WEB.with(|web| web.borrow_mut().turned_away(None));
    assert!(needed());

    // An answer to a request sent before signing in changes nothing.
    WEB.with(|web| {
        let mut web = web.borrow_mut();
        web.key = Some("new".into());
        web.sign_in_needed = false;
        web.turned_away(Some("old"));
    });
    assert!(!needed() && signed_in());

    WEB.with(|web| web.borrow_mut().turned_away(Some("new")));
    assert!(needed() && !signed_in());
    // It answered, so it doesn't count towards being out of reach.
    for _ in 0..OUT_OF_REACH_AFTER {
        WEB.with(|web| {
            let mut web = web.borrow_mut();
            let answered = web.sign_in_needed;
            web.checked(answered);
        });
    }
    assert!(!WEB.with(|web| web.borrow().out_of_reach()));
}

#[test]
fn the_preview_never_counts_as_out_of_reach() {
    for _ in 0..5 {
        WEB.with(|web| web.borrow_mut().checked(false));
    }
    assert_eq!(
        state(&first()).unwrap().status,
        "Preview only: start ipcamera-server for live video."
    );
}

#[test]
fn the_preview_records_nothing_and_says_what_ptz_needs() {
    let camera = state(&first()).unwrap();
    assert!(!camera.recording);
    assert_eq!(
        camera.ptz_status,
        "PTZ needs ipcamera-server or the desktop app."
    );
    assert_eq!(storage_abilities().reveal_label, "Open in a new tab");
    assert!(!storage_abilities().can_choose_folder);
}

#[test]
fn files_are_shown_from_the_servers_urls() {
    let shot = snapshot_info(
        "http://127.0.0.1:8770",
        SnapshotEntry {
            name: "snapshot-5-a.png".into(),
            url: "/files/snapshots/snapshot-5-a.png".into(),
            thumbnail: Some("/files/snapshots/thumbnails/snapshot-5-a.png".into()),
            camera: Some("a".into()),
            tags: vec!["vehicle".into()],
            taken_ms: 5_000,
            bytes: 10,
            size: Some(api::PixelSize {
                width: 640,
                height: 360,
            }),
        },
    );
    assert_eq!(
        shot.path,
        "http://127.0.0.1:8770/files/snapshots/snapshot-5-a.png"
    );
    assert_eq!(
        shot.thumbnail,
        "http://127.0.0.1:8770/files/snapshots/thumbnails/snapshot-5-a.png"
    );
    assert_eq!((shot.taken_at, shot.width), (5_000.0, 640));
    assert_eq!(
        shot.tags,
        ["vehicle"],
        "the server's tags, from the file's name"
    );
    assert_eq!(shot.event, "", "no event log on the web");

    let entry = VideoEntry {
        name: "video-5-a.mp4".into(),
        url: "/files/videos/video-5-a.mp4".into(),
        thumbnail: None,
        camera: Some("a".into()),
        tags: Vec::new(),
        started_ms: 5_000,
        length_ms: 2_500,
        bytes: 10,
        recording: true,
    };
    let video = video_info("", entry.clone());
    assert_eq!(
        video.path, "/files/videos/video-5-a.mp4",
        "the page's own server"
    );
    assert_eq!(video.thumbnail, "", "none yet");
    assert_eq!(video.seconds, 2.5);
    let with_thumbnail = video_info(
        "",
        VideoEntry {
            thumbnail: Some("/files/videos/thumbnails/video-5-a.png".into()),
            ..entry
        },
    );
    assert_eq!(
        with_thumbnail.thumbnail,
        "/files/videos/thumbnails/video-5-a.png"
    );
}

#[test]
fn the_servers_folders_show_once_it_has_said() {
    assert_eq!(storage_folder("videos").unwrap().path, "data/videos");
    served_by("http://127.0.0.1:8770", vec![porch()]);
    WEB.with(|web| {
        web.borrow_mut().storage = Some(api::Storage {
            snapshots: api::StorageEntry {
                path: r"D:\cams\snapshots".into(),
                is_default: false,
                ..api::StorageEntry::default()
            },
            videos: api::StorageEntry {
                path: r"D:\server\data\videos".into(),
                is_default: true,
                ..api::StorageEntry::default()
            },
        })
    });
    let snapshots = storage_folder("snapshots").unwrap();
    assert_eq!(snapshots.path, r"D:\cams\snapshots");
    assert!(!snapshots.is_default);
    assert!(storage_folder("videos").unwrap().is_default);
    assert!(storage_folder("settings").is_err());
    assert_eq!(reset_storage("videos").unwrap_err(), FOLDERS_ON_THE_SERVER);
}

#[test]
fn a_held_move_ends_when_let_go_or_pressed_again() {
    let right = PtzMove {
        pan: 1.0,
        tilt: 0.0,
        zoom: 0.0,
        speed: 0.5,
    };
    let first = hold(right);
    assert_eq!(held(first), Some(right));

    let up = PtzMove {
        pan: 0.0,
        tilt: 1.0,
        ..right
    };
    let second = hold(up);
    assert_eq!(held(first), None, "the newer press takes its place");
    assert_eq!(held(second), Some(up));

    release(first);
    assert_eq!(
        held(second),
        Some(up),
        "letting go of an old press leaves the new one"
    );
    release(second);
    assert_eq!(held(second), None);
}

#[test]
fn a_copy_has_no_first_view_of_its_own() {
    let saved = Library::from_snapshot(Snapshot::default());
    let copy = saved.snapshot().for_client();
    WEB.with(|web| web.borrow_mut().take(String::new(), copy, String::new()));
    assert!(
        with_library(library::views).is_empty(),
        "the server's views are the ones there are"
    );
}
