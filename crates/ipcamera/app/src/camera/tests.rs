//! Tests of the cameras: the hub, one camera, and what the bridge calls.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    thread::sleep,
    time::{Duration, Instant},
};

use ipcamera_core::{
    CameraConfig, CameraEvent, CameraId, Detection, EventFile, EventRules, EventStep, EventVideo,
    Library, SeenEvent, Stream, TileStream,
    config::{Automation, ConfigStore, EventCapture},
    sync::LockExt,
};
use ipcamera_storage::snapshot;

use super::{
    device::{Camera, audio_gain},
    hub::{Hub, Source, StreamOf, seed_test_camera, watch_rules},
};
use crate::{
    bridge::{CameraChanges, CameraForm, StatusTone, StreamTrouble},
    form,
};

/// A folder of its own for one test, removed when it ends. It starts with [`test_camera`], as
/// `nsh test`'s runner does.
struct Folder(tempfile::TempDir);

impl Folder {
    fn new() -> Self {
        let folder = Self(tempfile::TempDir::new().expect("a folder for the test"));
        seed_test_camera(&folder.settings());
        folder
    }

    fn settings(&self) -> Arc<ConfigStore> {
        Arc::new(ConfigStore::load(self.0.path().join("settings.json")))
    }

    /// The library as a fresh launch would read it, to check what reached the files.
    fn library(&self) -> Library {
        Library::open(self.0.path().join("data").join("settings"))
    }

    fn cameras_file(&self) -> PathBuf {
        self.0
            .path()
            .join("data")
            .join("settings")
            .join("cameras.json")
    }
}

fn pattern_hub(folder: &Folder) -> Hub {
    Hub::new(folder.settings(), Source::Pattern)
}

/// The camera every test starts with: [`test_camera`].
fn first(hub: &Hub) -> Arc<Camera> {
    hub.all()[0].clone()
}

fn named(name: &str) -> CameraForm {
    CameraForm {
        name: name.into(),
        host: "10.0.0.9".into(),
        ..form::blank()
    }
}

fn wait_for_frame(camera: &Camera) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while camera.frames().latest().is_none() {
        assert!(Instant::now() < deadline, "no frame arrived");
        sleep(Duration::from_millis(5));
    }
}

/// The cameras' own streams.
fn own(cameras: &[&Arc<Camera>]) -> Vec<StreamOf> {
    cameras
        .iter()
        .map(|camera| StreamOf::own(&camera.id))
        .collect()
}

fn on_screen(hub: &Hub, shown: &[&Arc<Camera>], audible: Option<&Arc<Camera>>) {
    hub.set_on_screen(
        own(shown),
        Vec::new(),
        audible.map(|camera| StreamOf::own(&camera.id)),
    );
    hub.reconcile();
}

/// No tile shows anything, as on the dashboard, and `kept` stay connected.
fn kept_off_screen(hub: &Hub, kept: &[&Arc<Camera>]) {
    hub.set_on_screen(Vec::new(), own(kept), None);
    hub.reconcile();
}

#[test]
fn a_test_run_starts_with_the_test_camera_in_the_first_tile() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);

    let cameras = hub.all();
    assert_eq!(cameras.len(), 1);
    assert_eq!(cameras[0].state().name, "Camera 1");
    let view = hub.library.views()[0].id.clone();
    assert_eq!(
        hub.library.shown_tiles(&view).unwrap()[0],
        Some(CameraId::from(cameras[0].id.as_str()).into())
    );
}

#[test]
fn an_offline_camera_cannot_connect_or_move() {
    let folder = Folder::new();
    let hub = Hub::new(folder.settings(), Source::Offline);
    let camera = first(&hub);

    assert_eq!(camera.connect().unwrap_err(), "this run has no camera");
    assert!(!camera.state().connected);

    camera.ptz_move(1.0, 0.0, 0.0, 0.5);
    assert!(camera.inner.locked().ptz.is_none());
}

#[test]
fn a_camera_is_found_by_its_id_and_nothing_else() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let id = first(&hub).id.clone();

    assert!(hub.camera(&id).is_ok());
    assert_eq!(
        hub.camera("porch").err().as_deref(),
        Some("no camera `porch`")
    );
}

#[test]
fn a_moved_camera_keeps_its_stream_and_every_list_follows() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let first_id = first(&hub).id.clone();
    let yard = hub.add(form::to_config(&named("Yard"))).unwrap();
    let camera = hub.camera(&yard).unwrap();
    camera.connect().unwrap();

    hub.move_camera(&yard, 0).unwrap();

    let held: Vec<_> = hub.all().iter().map(|held| held.id.clone()).collect();
    assert_eq!(held, [yard.clone(), first_id.clone()]);
    let saved: Vec<_> = folder
        .library()
        .cameras()
        .into_iter()
        .map(|saved| saved.id.to_string())
        .collect();
    assert_eq!(saved, [yard.clone(), first_id], "in the file too");
    assert!(
        Arc::ptr_eq(&hub.camera(&yard).unwrap(), &camera),
        "the same camera"
    );
    assert!(camera.state().connected, "its stream carries on");
    assert_eq!(
        hub.move_camera("porch", 0).unwrap_err(),
        "Could not move the camera: no camera `porch`"
    );
    camera.disconnect();
}

#[test]
fn muting_turning_down_or_not_being_selected_lowers_the_gain() {
    assert_eq!(audio_gain(true, 100.0, true), 0.0);
    assert_eq!(audio_gain(false, 100.0, true), 1.0);
    assert_eq!(audio_gain(false, 50.0, true), 0.25);
    assert_eq!(audio_gain(false, 0.0, true), 0.0);
    assert_eq!(audio_gain(false, 100.0, false), 0.0);
}

#[test]
fn a_stream_runs_from_connect_until_disconnect() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);

    assert!(!camera.state().connected);

    camera.connect().unwrap();
    wait_for_frame(&camera);

    let state = camera.state();
    assert!(state.connected);
    assert_eq!(state.status, "Test pattern.");
    assert_eq!(state.tone, StatusTone::Ok);

    camera.disconnect();

    let state = camera.state();
    assert!(!state.connected);
    assert_eq!(state.status, "Idle.");
    assert_eq!(state.stats, "");
    assert!(camera.frames().latest().is_none());
}

#[test]
fn connecting_twice_keeps_the_running_stream() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);

    camera.connect().unwrap();
    wait_for_frame(&camera);
    let produced = camera.frames().produced();

    camera.connect().unwrap();

    assert!(camera.frames().produced() >= produced);
    camera.disconnect();
}

#[test]
fn a_camera_with_no_password_waits_to_be_asked() {
    let folder = Folder::new();
    let hub = Hub::new(folder.settings(), Source::Stream);
    let camera = first(&hub);

    assert!(!camera.ready());
    assert_eq!(
        camera.state().status,
        "Add the camera's password in Settings to connect."
    );

    // Coming on screen doesn't try a login that would fail.
    on_screen(&hub, &[&camera], Some(&camera));
    assert!(!camera.state().connected);
}

#[test]
fn the_test_picture_is_always_ready_and_never_touches_ptz() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);

    assert!(camera.ready());

    camera.ptz_move(1.0, 0.0, 0.0, 0.5);

    assert!(camera.inner.locked().ptz.is_none());
    assert_eq!(camera.state().ptz_status, "PTZ: not checked yet");
}

#[test]
fn the_hub_follows_what_another_program_saved() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    // The server, sharing the app's files: a library of its own over the same folder.
    let server = folder.library();

    let added = server
        .add_camera(form::to_config(&named("From the server")), None)
        .unwrap();
    hub.follow();
    assert_eq!(hub.all().len(), 2);
    assert!(hub.camera(added.as_str()).is_ok(), "the new camera is held");

    let kept = CameraId::from(first(&hub).id.as_str());
    let mut renamed = server.camera(&kept).unwrap();
    renamed.name = "Renamed in the browser".into();
    server.update_cameras(vec![(kept, renamed)]).unwrap();
    server.delete_camera(&added).unwrap();
    hub.follow();

    assert_eq!(hub.all().len(), 1, "the deleted camera is gone");
    assert_eq!(first(&hub).summary().name, "Renamed in the browser");

    // And a change made here starts from what the server saved, so neither loses the other's.
    hub.add(form::to_config(&named("From the desktop")))
        .unwrap();
    let names: Vec<String> = folder
        .library()
        .cameras()
        .into_iter()
        .map(|camera| camera.name)
        .collect();
    assert_eq!(names, ["Renamed in the browser", "From the desktop"]);
}

#[test]
fn releasing_ptz_closes_it_until_the_next_move() {
    let folder = Folder::new();
    let hub = Hub::new(folder.settings(), Source::Stream);
    let camera = first(&hub);

    // Nothing listens on this port, so PTZ fails at once rather than reaching a real camera.
    let mut form = camera.form();
    form.host = "127.0.0.1".into();
    form.dvrip_port = "1".into();
    hub.save(&camera.id, &form).unwrap();

    camera.ptz_move(1.0, 0.0, 0.0, 0.5);
    assert!(camera.inner.locked().ptz.is_some());

    camera.release_ptz();
    assert!(camera.inner.locked().ptz.is_none());

    camera.ptz_move(1.0, 0.0, 0.0, 0.5);
    assert!(
        camera.inner.locked().ptz.is_some(),
        "the next move opens it again"
    );
}

#[test]
fn a_snapshot_takes_every_streaming_camera_into_the_data_folder() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let a = first(&hub);
    let b = hub
        .camera(&hub.add(form::to_config(&named("B"))).unwrap())
        .unwrap();
    let both = [a.id.clone(), b.id.clone()];

    assert_eq!(
        hub.snapshot(&both).unwrap_err(),
        "Connect first, then take a snapshot."
    );

    a.connect().unwrap();
    wait_for_frame(&a);
    assert_eq!(
        hub.snapshot(&both),
        Ok(1),
        "the camera that isn't connected is left out"
    );

    b.connect().unwrap();
    wait_for_frame(&b);
    assert_eq!(hub.snapshot(&both), Ok(2));

    let listed = snapshot::list(&hub.config.snapshots_dir());
    assert_eq!(
        listed.len(),
        3,
        "a second one of a camera takes a name of its own"
    );
    assert!(listed.iter().all(|saved| saved.path.is_file()));
    let of = |camera: &Camera| {
        listed
            .iter()
            .filter(|saved| saved.camera.as_deref() == Some(camera.id.as_str()))
            .count()
    };
    assert_eq!(
        (of(&a), of(&b)),
        (2, 1),
        "the gallery can tell which camera took each"
    );
    a.disconnect();
    b.disconnect();
}

#[test]
fn audio_is_kept_and_saved_in_the_settings() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);

    camera.set_audio(false, 35.0);

    let state = camera.state();
    assert!(!state.muted);
    assert_eq!(state.volume, 35.0);
    assert_eq!(hub.config.current().ui.volume, 35.0);
    assert_eq!(
        folder.settings().current().ui.volume,
        35.0,
        "it reached the file"
    );
}

#[test]
fn saving_the_form_updates_the_file_and_the_labels() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);

    let mut form = camera.form();
    form.name = "Driveway".into();
    form.host = "10.0.0.9".into();
    hub.save(&camera.id, &form).unwrap();

    assert_eq!(camera.state().name, "Driveway");
    assert!(camera.state().stream_label.contains("10.0.0.9"));

    // A fresh launch sees what was saved, so it really reached the file.
    let saved = folder.library().cameras();
    assert_eq!(saved[0].name, "Driveway");
    assert_eq!(saved[0].id.as_str(), camera.id, "the id never changes");
}

#[test]
fn saving_reconnects_a_running_stream_and_leaves_a_stopped_one_alone() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);

    let form = camera.form();
    hub.save(&camera.id, &form).unwrap();
    assert!(!camera.state().connected);

    camera.connect().unwrap();
    wait_for_frame(&camera);
    let before = camera.frames();

    hub.save(&camera.id, &form).unwrap();

    assert!(camera.state().connected);
    wait_for_frame(&camera);
    assert!(
        before.latest().is_none(),
        "the old slot was cleared when its stream stopped"
    );
    camera.disconnect();
}

#[test]
fn a_save_that_cannot_write_says_where_and_changes_nothing() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    let name = camera.state().name;

    // A folder where the file should be makes the rename fail.
    let path = folder.cameras_file();
    std::fs::remove_file(&path).expect("the cameras file should exist");
    std::fs::create_dir(&path).expect("the blocking folder should be created");

    let mut form = camera.form();
    form.name = "Driveway".into();
    let error = hub.save(&camera.id, &form).unwrap_err();

    assert!(error.starts_with("Could not save "), "{error}");
    assert!(error.contains("cameras.json"), "{error}");
    assert_eq!(camera.state().name, name);
}

#[test]
fn changes_reach_every_camera_named_in_one_save_and_leave_the_rest() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let first = first(&hub);
    let porch = hub.add(form::to_config(&named("Porch"))).unwrap();
    let yard = hub.add(form::to_config(&named("Yard"))).unwrap();
    let before = hub.library.revision();

    let changes = CameraChanges {
        user: Some("viewer".into()),
        sub_stream: Some(1),
        ..CameraChanges::default()
    };
    let count = hub
        .save_changes(&[first.id.clone(), porch.clone()], &changes)
        .unwrap();

    assert_eq!(count, 2);
    assert_eq!(hub.library.revision(), before + 1, "one write for both");
    assert!(first.state().stream_label.starts_with("viewer@"));
    let saved = folder.library().cameras();
    for (camera, user) in saved.iter().zip(["viewer", "viewer", "admin"]) {
        assert_eq!(camera.user, user, "{}", camera.name);
    }
    // What the changes leave out stays each camera's own: names, addresses and channels.
    assert_eq!(saved[0].name, "Camera 1");
    assert_eq!(saved[0].host, "192.0.2.10");
    assert_eq!(saved[1].stream, Stream::Sub);
    assert_eq!(saved[2].stream, Stream::Main);
    assert_eq!(hub.camera(&yard).unwrap().state().name, "Yard");
}

#[test]
fn changes_that_change_nothing_save_nothing() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    let before = hub.library.revision();

    // A blank port saves as the default, which the camera already has.
    let changes = CameraChanges {
        user: Some("admin".into()),
        rtsp_port: Some(String::new()),
        ..CameraChanges::default()
    };

    assert_eq!(
        hub.save_changes(std::slice::from_ref(&camera.id), &changes),
        Ok(0)
    );
    assert_eq!(hub.library.revision(), before);
}

#[test]
fn changes_with_a_problem_or_an_unknown_camera_change_nothing() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    let before = hub.library.revision();

    let bad = CameraChanges {
        rtsp_port: Some("70000".into()),
        ..CameraChanges::default()
    };
    assert_eq!(
        hub.save_changes(std::slice::from_ref(&camera.id), &bad),
        Err("Use a port from 1 to 65535.".to_string())
    );

    let good = CameraChanges {
        user: Some("viewer".into()),
        ..CameraChanges::default()
    };
    assert!(
        hub.save_changes(&[camera.id.clone(), "gone".into()], &good)
            .is_err()
    );

    assert_eq!(hub.library.revision(), before);
    assert_eq!(folder.library().cameras()[0].user, "admin");
}

#[test]
fn changes_reconnect_a_running_stream() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    camera.connect().unwrap();
    wait_for_frame(&camera);
    let before = camera.frames();

    let changes = CameraChanges {
        use_tcp: Some(false),
        ..CameraChanges::default()
    };
    assert_eq!(
        hub.save_changes(std::slice::from_ref(&camera.id), &changes),
        Ok(1)
    );

    assert!(camera.state().connected);
    wait_for_frame(&camera);
    assert!(
        before.latest().is_none(),
        "the old slot was cleared when its stream stopped"
    );
    camera.disconnect();
}

#[test]
fn an_added_camera_is_held_and_saved() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);

    let id = hub.add(form::to_config(&named("Porch"))).unwrap();

    assert_eq!(hub.camera(&id).unwrap().state().name, "Porch");
    assert_eq!(hub.all().len(), 2);
    assert!(
        !hub.camera(&id).unwrap().state().connected,
        "it connects when it comes on screen"
    );
    let saved = folder.library().cameras();
    assert_eq!(saved[1].name, "Porch");
    assert_eq!(saved[1].id.as_str(), id);
}

#[test]
fn a_deleted_camera_stops_and_is_gone_everywhere() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    camera.connect().unwrap();
    wait_for_frame(&camera);

    hub.delete(&camera.id).unwrap();

    assert!(hub.camera(&camera.id).is_err());
    assert!(!camera.state().connected);
    assert!(camera.frames().latest().is_none());
    let library = folder.library();
    assert!(library.cameras().is_empty());
    assert_eq!(
        library.views()[0].shown_tiles(),
        vec![None; 4],
        "its tile in the first view is empty"
    );
}

#[test]
fn cameras_on_screen_stream_and_the_rest_stop() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let a = first(&hub);
    let b = hub
        .camera(&hub.add(form::to_config(&named("B"))).unwrap())
        .unwrap();

    on_screen(&hub, &[&a], Some(&a));
    assert!(a.state().connected);
    assert!(!b.state().connected);

    on_screen(&hub, &[&b], Some(&b));
    assert!(!a.state().connected, "off screen, it stops");
    assert!(b.state().connected);

    on_screen(&hub, &[], None);
    assert!(!b.state().connected);
}

#[test]
fn a_kept_camera_stays_connected_and_draws_nothing_until_shown() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    on_screen(&hub, &[&camera], Some(&camera));
    wait_for_frame(&camera);

    kept_off_screen(&hub, &[&camera]);
    assert!(camera.state().connected, "kept, it stays connected");
    assert_eq!(camera.state().tone, StatusTone::Ok);
    sleep(Duration::from_millis(100));
    assert!(
        camera.frames().latest().is_none(),
        "with no tile to show it, it holds no picture"
    );

    on_screen(&hub, &[&camera], Some(&camera));
    wait_for_frame(&camera);

    kept_off_screen(&hub, &[]);
    assert!(
        !camera.state().connected,
        "neither shown nor kept, it stops"
    );
}

#[test]
fn a_kept_camera_connects_without_drawing() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);

    kept_off_screen(&hub, &[&camera]);
    assert!(camera.state().connected);
    sleep(Duration::from_millis(100));
    assert!(camera.frames().latest().is_none());

    kept_off_screen(&hub, &[]);
}

#[test]
fn a_camera_the_user_stopped_stays_stopped_on_screen() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    on_screen(&hub, &[&camera], Some(&camera));
    assert!(camera.state().connected);

    camera.held.store(true, Ordering::SeqCst);
    camera.disconnect();
    on_screen(&hub, &[&camera], Some(&camera));
    assert!(!camera.state().connected);

    camera.held.store(false, Ordering::SeqCst);
    on_screen(&hub, &[&camera], Some(&camera));
    assert!(camera.state().connected);
    camera.disconnect();
}

#[test]
fn recording_needs_a_camera_stream() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);

    assert_eq!(
        camera.start_recording().unwrap_err(),
        "Connect first, then record."
    );

    camera.connect().unwrap();
    assert_eq!(
        camera.start_recording().unwrap_err(),
        "The test picture can't be recorded.",
        "the test picture is drawn here, not sent by a camera"
    );
    assert!(!camera.state().recording);
    assert!(!camera.stop_recording(), "nothing was recording");
    camera.disconnect();
}

#[test]
fn a_recording_camera_stays_connected_off_screen_until_recording_stops() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    on_screen(&hub, &[&camera], Some(&camera));
    // As a camera stream that started recording would be.
    camera.recording.store(true, Ordering::SeqCst);

    on_screen(&hub, &[], None);
    assert!(
        camera.state().connected,
        "looking elsewhere doesn't end a recording"
    );
    assert!(camera.state().recording);

    assert!(camera.stop_recording());
    hub.reconcile();
    assert!(
        !camera.state().connected,
        "once it stops, the camera leaves with the screen"
    );
}

#[test]
fn disconnecting_ends_the_recording() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    camera.connect().unwrap();
    camera.recording.store(true, Ordering::SeqCst);

    camera.disconnect();
    camera.connect().unwrap();

    assert!(
        !camera.state().recording,
        "connecting again doesn't pick it up again"
    );
    camera.disconnect();
}

#[test]
fn only_the_selected_camera_is_heard() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let a = first(&hub);
    let b = hub
        .camera(&hub.add(form::to_config(&named("B"))).unwrap())
        .unwrap();
    a.set_audio(false, 100.0);
    b.set_audio(false, 100.0);

    on_screen(&hub, &[&a, &b], Some(&b));

    assert_eq!(a.inner.locked().gain(), 0.0);
    assert_eq!(b.inner.locked().gain(), 1.0);
    assert!(!a.state().muted, "the user's own mute is left as it was");
    on_screen(&hub, &[], None);
}

/// Camera `camera`'s sub stream, as a tile that picks it says.
fn sub_of(camera: &Camera) -> StreamOf {
    StreamOf {
        stream: TileStream::from_key("sub"),
        ..StreamOf::own(&camera.id)
    }
}

/// The streams of `camera` that run.
fn running(camera: &Camera) -> Vec<Option<TileStream>> {
    let inner = camera.inner.locked();
    let mut keys: Vec<_> = inner
        .runs
        .iter()
        .filter(|(_, run)| run.feed.is_some())
        .map(|(key, _)| key.clone())
        .collect();
    keys.sort_by_key(|key| key.as_ref().map(|stream| stream.key().to_string()));
    keys
}

#[test]
fn a_tile_on_another_stream_is_a_connection_of_its_own() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    let sub = TileStream::from_key("sub");
    camera.set_audio(false, 100.0);

    hub.set_on_screen(
        vec![StreamOf::own(&camera.id), sub_of(&camera)],
        Vec::new(),
        Some(sub_of(&camera)),
    );
    hub.reconcile();
    assert_eq!(running(&camera), [None, sub.clone()]);
    let deadline = Instant::now() + Duration::from_secs(5);
    while camera.output(&sub).frames().latest().is_none() {
        assert!(Instant::now() < deadline, "the sub stream sent no frame");
        sleep(Duration::from_millis(5));
    }
    let state = camera.state();
    assert!(state.connected);
    assert!(
        state.stream_label.ends_with("ch1 sub"),
        "the status follows the selected tile's stream: {}",
        state.stream_label
    );
    {
        let inner = camera.inner.locked();
        assert_eq!(inner.gain_of(&sub), 1.0);
        assert_eq!(
            inner.gain(),
            0.0,
            "only the selected tile's stream is heard"
        );
    }

    // The tile goes back to the camera's own stream.
    on_screen(&hub, &[&camera], Some(&camera));
    assert_eq!(running(&camera), [None]);
    assert!(
        camera.output(&sub).frames().latest().is_none(),
        "a stream that stops holds no picture"
    );
    camera.disconnect();
}

#[test]
fn connect_starts_the_streams_the_tiles_show() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    camera.disconnect();
    camera.held.store(true, Ordering::SeqCst);

    hub.set_on_screen(vec![sub_of(&camera)], Vec::new(), None);
    hub.reconcile();
    assert!(running(&camera).is_empty(), "a disconnected camera waits");

    camera.connect_to(&hub.wanted_streams(&camera.id)).unwrap();
    assert_eq!(
        running(&camera),
        [TileStream::from_key("sub")],
        "only what the tile shows"
    );

    hub.set_on_screen(Vec::new(), Vec::new(), None);
    assert_eq!(
        hub.wanted_streams(&camera.id),
        [None],
        "off screen, Connect starts the camera's own"
    );
    camera.disconnect();
    assert!(running(&camera).is_empty(), "Disconnect stops them all");
}

#[test]
fn recording_is_of_the_cameras_own_stream() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    let ids = [camera.id.clone()];
    assert_eq!(hub.record(&ids).unwrap_err(), "Connect first, then record.");

    hub.set_on_screen(vec![sub_of(&camera)], Vec::new(), None);
    hub.reconcile();
    // The test picture can't record, so the answer says so, once the camera's own stream is up.
    assert_eq!(
        hub.record(&ids).unwrap_err(),
        "The test picture can't be recorded."
    );
    assert_eq!(
        running(&camera),
        [TileStream::from_key("sub")],
        "its own stream, connected for a recording that couldn't start, disconnects again"
    );
    camera.disconnect();
}

#[test]
fn a_recording_no_tile_shows_decodes_nothing() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    // The camera's own stream is up, as recording starts it, and no tile shows it.
    hub.set_on_screen(vec![sub_of(&camera)], vec![StreamOf::own(&camera.id)], None);
    hub.reconcile();

    // As a camera stream that started recording would be: its own stream stays for it.
    camera.recording.store(true, Ordering::SeqCst);
    hub.set_on_screen(vec![sub_of(&camera)], Vec::new(), None);
    hub.reconcile();
    assert_eq!(running(&camera), [None, TileStream::from_key("sub")]);
    sleep(Duration::from_millis(100));
    assert!(
        camera.frames().latest().is_none(),
        "no tile shows the camera's own stream, so it holds no picture"
    );

    assert!(camera.stop_recording());
    hub.reconcile();
    assert_eq!(running(&camera), [TileStream::from_key("sub")]);
    camera.disconnect();
}

#[test]
fn an_events_snapshot_is_the_picture_on_screen_tagged_and_linked() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    camera.connect().unwrap();
    wait_for_frame(&camera);

    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Started, 1_000, None),
    );
    let logged = hub.events.event(&format!("1000-{}", camera.id)).unwrap();
    let name = logged.snapshot.clone().expect("the picture on screen");
    assert!(
        name.ends_with(&format!("-{}.person.png", camera.id)),
        "{name}"
    );
    let saved = hub.config.snapshots_dir().join(&name);
    assert!(saved.is_file(), "in the snapshots folder, with the rest");
    assert_eq!(
        snapshot::list(&hub.config.snapshots_dir())[0].tags,
        ["person"]
    );
    assert!(
        logged.videos.is_empty(),
        "a picture is all Settings asks for"
    );

    // The sighting ending changes the log, keeping the picture it has.
    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Quiet, 1_000, Some(2_000)),
    );
    let ended = hub.events.event(&logged.id).unwrap();
    assert_eq!(ended.ended, Some(2_000));
    assert_eq!(ended.snapshot.as_deref(), Some(name.as_str()));
    assert_eq!(snapshot::list(&hub.config.snapshots_dir()).len(), 1);
    camera.disconnect();
}

#[test]
fn a_camera_no_tile_shows_connects_for_its_events_picture() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    assert!(running(&camera).is_empty());

    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Started, 1_000, None),
    );
    let logged = hub.events.event(&format!("1000-{}", camera.id)).unwrap();
    assert!(logged.snapshot.is_some(), "a picture from its own stream");
    assert!(
        running(&camera).is_empty(),
        "the stream connected for the picture disconnects again"
    );

    // One the user disconnected stays so, and goes without.
    camera.held.store(true, Ordering::SeqCst);
    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Started, 9_000, None),
    );
    let held = hub.events.event(&format!("9000-{}", camera.id)).unwrap();
    assert_eq!(held.snapshot, None);
    assert!(running(&camera).is_empty());
}

#[test]
fn an_event_saves_only_what_settings_ask() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    hub.config
        .update(|config| config.automation.event_capture = EventCapture::Nothing)
        .unwrap();
    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Started, 1_000, None),
    );
    let logged = hub.events.event(&format!("1000-{}", camera.id)).unwrap();
    assert_eq!((logged.snapshot, logged.videos.len()), (None, 0));
    assert!(running(&camera).is_empty(), "nothing connects for nothing");
    assert!(!hub.config.snapshots_dir().exists());
}

#[test]
fn an_event_no_watch_sees_any_more_ends() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    let event = seen_event(&camera, EventStep::Started, 1_000, None).event;
    hub.events.record(event.clone()).unwrap();

    // The test picture has no watch, as a camera that was deleted or changed has none.
    hub.watch_cameras();
    let logged = hub.events.event(&event.id).expect("still logged");
    assert!(logged.ended.is_some_and(|ended| ended > event.started));
    assert!(
        !camera.watching(),
        "the test picture never logs in to a camera"
    );
}

/// An event of `camera`'s as its watch would hand it over at `step`: one the camera sees now, or
/// last saw at `ended`, having seen a person.
fn seen_event(camera: &Camera, step: EventStep, started: u64, ended: Option<u64>) -> SeenEvent {
    SeenEvent {
        step,
        event: CameraEvent {
            id: format!("{started}-{}", camera.id),
            camera: CameraId::from(camera.id.as_str()),
            started,
            ended,
            motion: false,
            detected: vec![Detection::Person],
            sightings: 1,
            snapshot: None,
            videos: Vec::new(),
        },
        tags: if ended.is_none() {
            vec!["person"]
        } else {
            Vec::new()
        },
        still: None,
    }
}

#[test]
fn a_snapshot_past_the_media_limit_is_refused_unless_the_oldest_may_go() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    camera.connect().unwrap();
    wait_for_frame(&camera);

    let videos = hub.config.videos_dir();
    std::fs::create_dir_all(&videos).unwrap();
    let old = videos.join("video-1-old.mp4");
    std::fs::write(&old, vec![0u8; 2 * 1024 * 1024]).unwrap();
    hub.config
        .update(|config| config.automation.media_limit_mb = 1)
        .unwrap();

    let refused = hub.snapshot(std::slice::from_ref(&camera.id)).unwrap_err();
    assert!(
        refused.contains("the most Settings → Automation allows"),
        "{refused}"
    );
    assert!(old.exists(), "nothing is deleted unless the user allows it");
    assert!(hub.record(std::slice::from_ref(&camera.id)).is_err());

    // Nor does an event take one.
    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Started, 1_000, None),
    );
    assert_eq!(
        hub.events
            .event(&format!("1000-{}", camera.id))
            .unwrap()
            .snapshot,
        None
    );

    hub.config
        .update(|config| config.automation.overwrite_oldest = true)
        .unwrap();
    assert_eq!(hub.snapshot(std::slice::from_ref(&camera.id)), Ok(1));
    assert!(!old.exists(), "the oldest went to make room");
    camera.disconnect();
}

#[test]
fn a_recording_a_sighting_started_stops_as_the_sighting_ends() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    hub.config
        .update(|config| config.automation.event_capture = EventCapture::Nothing)
        .unwrap();

    // As a recording a sighting started would be.
    camera.recording.store(true, Ordering::SeqCst);
    camera.recording_by_itself.store(true, Ordering::SeqCst);
    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Started, 1_000, None),
    );
    assert!(camera.is_recording());
    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Quiet, 1_000, Some(2_000)),
    );
    assert!(
        !camera.is_recording(),
        "the sighting is over, and so is its recording"
    );

    // One the user started carries on through an event.
    camera.recording.store(true, Ordering::SeqCst);
    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Started, 3_000, None),
    );
    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Closed, 3_000, Some(4_000)),
    );
    assert!(camera.is_recording());
    camera.stop_recording();
}

#[test]
fn each_sighting_records_only_when_settings_ask_and_says_why_it_cannot() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);

    hub.config
        .update(|config| config.automation.event_capture = EventCapture::Nothing)
        .unwrap();
    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Started, 1_000, None),
    );
    assert!(
        running(&camera).is_empty(),
        "nothing asked: nothing connects"
    );
    assert_eq!(camera.state().record_problem, "");

    hub.config
        .update(|config| config.automation.event_capture = EventCapture::Video)
        .unwrap();
    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::SeenAgain, 1_000, None),
    );
    let problem = camera.state().record_problem;
    assert!(
        problem.starts_with("Didn't record what it saw. The test picture can't be recorded."),
        "{problem}"
    );
    assert!(!camera.is_recording());
    assert!(
        running(&camera).is_empty(),
        "its stream, connected for the recording, disconnects again"
    );
}

#[test]
fn only_the_kinds_picked_save_a_video_and_one_seen_later_starts_it() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    hub.config
        .update(|config| {
            config.automation.event_capture = EventCapture::Video;
            config.automation.event_video.kinds = vec![Detection::Animal];
        })
        .unwrap();

    hub.log_event(
        &camera,
        seen_event(&camera, EventStep::Started, 1_000, None),
    );
    assert_eq!(
        camera.state().record_problem,
        "",
        "a person isn't recorded: no recording was tried"
    );

    // An animal comes into the same sighting: now it records, which the test picture can't.
    let mut added = seen_event(&camera, EventStep::Added, 1_000, None);
    added.tags = vec!["person", "animal"];
    hub.log_event(&camera, added);
    let problem = camera.state().record_problem;
    assert!(
        problem.starts_with("Didn't record what it saw."),
        "{problem}"
    );
}

#[test]
fn a_sighting_lasts_the_post_roll_only_while_events_record_videos() {
    let mut automation = Automation::default();
    let camera = CameraConfig::default();
    assert_eq!(
        watch_rules(&automation, &camera),
        Some(EventRules::grouping(5 * 60_000)),
        "a picture only: sightings end as they always have"
    );

    automation.event_capture = EventCapture::PictureAndVideo;
    assert_eq!(
        watch_rules(&automation, &camera),
        Some(EventRules {
            window_ms: 5 * 60_000,
            quiet_ms: 30_000,
            motion_continues: true,
        })
    );
    let own = CameraConfig {
        event_video: Some(EventVideo {
            after_seconds: 60,
            motion_continues: false,
            ..EventVideo::default()
        }),
        ..CameraConfig::default()
    };
    assert_eq!(
        watch_rules(&automation, &own),
        Some(EventRules {
            window_ms: 5 * 60_000,
            quiet_ms: 60_000,
            motion_continues: false,
        }),
        "a camera's own settings in place of the app's"
    );

    let quiet = CameraConfig {
        log_events: false,
        ..CameraConfig::default()
    };
    assert_eq!(
        watch_rules(&automation, &quiet),
        None,
        "a camera whose events are off is watched only for its badge"
    );

    automation.log_events = false;
    assert_eq!(watch_rules(&automation, &camera), None);
}

#[test]
fn deleting_an_event_deletes_what_it_saved() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    let (snapshots, videos) = (hub.config.snapshots_dir(), hub.config.videos_dir());
    std::fs::create_dir_all(&snapshots).unwrap();
    std::fs::create_dir_all(&videos).unwrap();
    std::fs::write(snapshots.join("snapshot-1-a.person.png"), "picture").unwrap();
    std::fs::write(videos.join("video-1-a.person.mp4"), "video").unwrap();
    std::fs::write(snapshots.join("snapshot-2-a.png"), "taken by hand").unwrap();

    let mut event = seen_event(&camera, EventStep::Started, 1_000, Some(2_000)).event;
    event.snapshot = Some("snapshot-1-a.person.png".into());
    event.videos = vec!["video-1-a.person.mp4".into(), "video-9-a.person.mp4".into()];
    hub.events.record(event.clone()).unwrap();
    hub.events
        .link(
            &event.id,
            EventFile::Snapshot("snapshot-1-a.person.png".into()),
        )
        .unwrap();
    for video in &event.videos {
        hub.events
            .link(&event.id, EventFile::Video(video.clone()))
            .unwrap();
    }

    let gone = hub.events.delete(std::slice::from_ref(&event.id)).unwrap();
    hub.delete_saved(&gone);
    assert!(!snapshots.join("snapshot-1-a.person.png").exists());
    assert!(!videos.join("video-1-a.person.mp4").exists());
    assert!(
        snapshots.join("snapshot-2-a.png").exists(),
        "what no event saved stays"
    );
}

#[test]
fn turning_events_off_stops_what_they_started() {
    let folder = Folder::new();
    let hub = pattern_hub(&folder);
    let camera = first(&hub);
    hub.events
        .record(seen_event(&camera, EventStep::Started, 1_000, None).event)
        .unwrap();
    // As a recording a sighting started would be.
    camera.recording.store(true, Ordering::SeqCst);
    camera.recording_by_itself.store(true, Ordering::SeqCst);

    hub.config
        .update(|config| config.automation.log_events = false)
        .unwrap();
    hub.watch_cameras();
    assert!(!camera.is_recording(), "no event would end it");
    assert!(
        hub.events.events().iter().all(|event| !event.ongoing()),
        "nothing goes on in a log no one keeps"
    );
}

/// A listener on this machine that hangs up on every connection, as a camera's port does when the
/// camera has gone, and counts them; no login ever reaches a real camera.
fn hangs_up() -> (u16, Arc<std::sync::atomic::AtomicUsize>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port for the test");
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = count.clone();
    std::thread::spawn(move || {
        for connection in listener.incoming() {
            counted.fetch_add(1, Ordering::SeqCst);
            drop(connection);
        }
    });
    (port, count)
}

#[test]
fn a_stream_that_gave_up_says_why_and_connect_tries_it_again() {
    let folder = Folder::new();
    let hub = Hub::new(folder.settings(), Source::Stream);
    let camera = first(&hub);
    let (port, connections) = hangs_up();
    let mut form = camera.form();
    form.host = "127.0.0.1".into();
    form.rtsp_port = port.to_string();
    // So the first failure is the last, rather than a reconnect.
    form.auto_reconnect = false;
    hub.save(&camera.id, &form).unwrap();

    camera.connect().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let stream = loop {
        let state = camera.state();
        if let Some(stream) = state
            .streams
            .into_iter()
            .find(|stream| stream.trouble != StreamTrouble::Fine)
        {
            break stream;
        }
        assert!(Instant::now() < deadline, "the stream never gave up");
        sleep(Duration::from_millis(20));
    };
    assert_eq!(stream.trouble, StreamTrouble::Stopped);
    assert!(
        stream.problem.ends_with('.'),
        "a sentence: {}",
        stream.problem
    );
    assert_eq!(stream.last_picture_at, 0.0, "it never had a picture");
    assert!(
        camera.state().connected,
        "it stays the camera's until Disconnect"
    );

    let tried = connections.load(Ordering::SeqCst);
    camera.connect().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while connections.load(Ordering::SeqCst) == tried {
        assert!(Instant::now() < deadline, "Connect didn't try again");
        sleep(Duration::from_millis(20));
    }
    camera.disconnect();
}
