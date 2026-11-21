//! The library's files as a user meets them, through the public API only. A change across files,
//! one of them unwritable, leaves no worse than a stale id, which a retry or the next launch fixes.

use std::{
    fs,
    path::{Path, PathBuf},
};

use ipcamera_core::{CameraConfig, CameraId, Library, config::ConfigStore};
use serde_json::Value;

/// Named here, not taken from the crate: these tests check what lands on disk.
const CAMERAS_FILE: &str = "cameras.json";
const GROUPS_FILE: &str = "groups.json";
const VIEWS_FILE: &str = "views.json";

struct AppFolder(tempfile::TempDir);

impl AppFolder {
    fn new() -> Self {
        Self(tempfile::TempDir::new().expect("a folder for the test"))
    }

    fn settings(&self) -> ConfigStore {
        ConfigStore::load(self.0.path().join("settings.json"))
    }

    fn settings_dir(&self) -> PathBuf {
        self.0.path().join("data").join("settings")
    }
}

fn json(path: &Path) -> Value {
    let text = fs::read_to_string(path).expect("the file should be there");
    serde_json::from_str(&text).expect("the file should be JSON")
}

#[test]
fn a_first_launch_starts_with_no_cameras_and_a_default_view() {
    let app = AppFolder::new();
    let config = app.settings();
    assert!(config.problem().is_none(), "{:?}", config.problem());
    let config_before = fs::read_to_string(config.path()).unwrap();

    let library = Library::beside(&config);

    assert_eq!(config.settings_dir(), app.settings_dir());
    assert!(library.cameras().is_empty());
    assert_eq!(
        fs::read_to_string(app.settings_dir().join(CAMERAS_FILE)).unwrap(),
        "{\n  \"version\": 1,\n  \"cameras\": []\n}\n"
    );
    assert_eq!(
        fs::read_to_string(app.settings_dir().join(GROUPS_FILE)).unwrap(),
        "{\n  \"version\": 1,\n  \"groups\": []\n}\n"
    );
    let views = json(&app.settings_dir().join(VIEWS_FILE));
    assert_eq!(views["views"][0]["name"], "Default");
    assert_eq!(views["views"][0]["tiles"], serde_json::json!([]));
    assert_eq!(
        fs::read_to_string(config.path()).unwrap(),
        config_before,
        "opening the library leaves settings.json as it was"
    );
}

#[test]
fn a_camera_in_two_groups_is_saved_once_and_the_groups_hold_only_its_id() {
    let app = AppFolder::new();
    let library = Library::beside(&app.settings());
    let outside = library.add_group("Outside", Vec::new()).unwrap();
    let garage = library.add_group("Garage", Vec::new()).unwrap();
    let driveway = library
        .add_camera(
            CameraConfig {
                name: "Driveway".into(),
                password: "hunter2".into(),
                ..CameraConfig::default()
            },
            Some(&outside),
        )
        .unwrap();
    library.add_to_group(&garage, &driveway).unwrap();

    let cameras = json(&app.settings_dir().join(CAMERAS_FILE));
    let saved: Vec<_> = cameras["cameras"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|camera| camera["id"] == driveway.as_str())
        .collect();
    assert_eq!(saved.len(), 1, "the camera's details exist once");
    assert_eq!(saved[0]["name"], "Driveway");

    let groups_text = fs::read_to_string(app.settings_dir().join(GROUPS_FILE)).unwrap();
    let groups: Value = serde_json::from_str(&groups_text).unwrap();
    for group in groups["groups"].as_array().unwrap() {
        assert_eq!(group["cameras"], serde_json::json!([driveway.as_str()]));
    }
    assert!(
        !groups_text.contains("hunter2") && !groups_text.contains("Driveway"),
        "groups.json holds ids, never camera details"
    );

    let reopened = Library::beside(&app.settings());
    let tree = reopened.tree();
    assert_eq!(tree.len(), 2);
    assert!(tree.iter().all(|group| group.cameras == [driveway.clone()]));
}

#[test]
fn a_cameras_password_comes_back_from_the_file_and_is_kept_from_anyone_else() {
    let app = AppFolder::new();
    let library = Library::beside(&app.settings());
    let driveway = library
        .add_camera(
            CameraConfig {
                name: "Driveway".into(),
                password: "hunter2".into(),
                ..CameraConfig::default()
            },
            None,
        )
        .unwrap();
    let path = app.settings_dir().join(CAMERAS_FILE);

    #[cfg(windows)]
    assert!(!fs::read_to_string(&path).unwrap().contains("hunter2"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    let reopened = Library::beside(&app.settings());
    assert_eq!(reopened.camera(&driveway).unwrap().password, "hunter2");
}

/// A folder in the file's place makes every write fail; returns what the file held.
fn block_writes(path: &Path) -> String {
    let text = fs::read_to_string(path).expect("the file should be there");
    fs::remove_file(path).unwrap();
    fs::create_dir(path).unwrap();
    text
}

fn allow_writes(path: &Path, text: &str) {
    fs::remove_dir(path).unwrap();
    fs::write(path, text).unwrap();
}

fn library_with_a_camera_everywhere(app: &AppFolder) -> (Library, CameraId) {
    let library = Library::beside(&app.settings());
    let outside = library.add_group("Outside", Vec::new()).unwrap();
    let driveway = library
        .add_camera(
            CameraConfig {
                name: "Driveway".into(),
                ..CameraConfig::default()
            },
            Some(&outside),
        )
        .unwrap();
    let view = library.views()[0].id.clone();
    library
        .show_in_tile(&view, 0, Some(&driveway.clone().into()))
        .unwrap();
    (library, driveway)
}

fn names(path: &Path, camera: &CameraId) -> bool {
    fs::read_to_string(path).unwrap().contains(camera.as_str())
}

#[test]
fn a_camera_whose_group_could_not_be_saved_is_kept_and_joins_the_group_on_a_retry() {
    let app = AppFolder::new();
    let library = Library::beside(&app.settings());
    let outside = library.add_group("Outside", Vec::new()).unwrap();
    let groups_file = app.settings_dir().join(GROUPS_FILE);
    let groups_before = block_writes(&groups_file);

    let added = library.add_camera(
        CameraConfig {
            name: "Driveway".into(),
            ..CameraConfig::default()
        },
        Some(&outside),
    );

    let error = added.expect_err("the group could not be saved");
    assert!(error.to_string().contains(GROUPS_FILE), "{error}");
    let cameras = library.cameras();
    assert_eq!(cameras.len(), 1, "the camera itself was saved");
    let driveway = cameras[0].id.clone();
    assert!(names(&app.settings_dir().join(CAMERAS_FILE), &driveway));
    assert!(
        library.tree()[0].cameras.is_empty(),
        "it is under All cameras only, as the error says"
    );

    allow_writes(&groups_file, &groups_before);
    library.add_to_group(&outside, &driveway).unwrap();

    let reopened = Library::beside(&app.settings());
    assert_eq!(reopened.cameras().len(), 1, "added once, not twice");
    assert_eq!(reopened.tree()[0].cameras, [driveway]);
}

#[test]
fn a_delete_that_stopped_half_way_hides_the_camera_and_is_finished_by_deleting_again() {
    let app = AppFolder::new();
    let (library, driveway) = library_with_a_camera_everywhere(&app);
    let view = library.views()[0].id.clone();
    let groups_file = app.settings_dir().join(GROUPS_FILE);
    let views_file = app.settings_dir().join(VIEWS_FILE);
    let groups_before = block_writes(&groups_file);

    let error = library
        .delete_camera(&driveway)
        .expect_err("the group could not be saved");
    assert!(error.to_string().contains(GROUPS_FILE), "{error}");

    assert!(library.camera(&driveway).is_none());
    assert!(library.tree()[0].cameras.is_empty());
    assert_eq!(library.shown_tiles(&view).unwrap()[0], None);
    assert!(names(&views_file, &driveway), "the view was never reached");

    allow_writes(&groups_file, &groups_before);
    library
        .delete_camera(&driveway)
        .expect("deleting it again finishes the delete");

    assert!(!names(&groups_file, &driveway));
    assert!(!names(&views_file, &driveway));
    assert!(!names(&app.settings_dir().join(CAMERAS_FILE), &driveway));
}

#[test]
fn a_delete_that_stopped_half_way_is_finished_by_the_next_launch() {
    let app = AppFolder::new();
    let (library, driveway) = library_with_a_camera_everywhere(&app);
    let groups_file = app.settings_dir().join(GROUPS_FILE);
    let views_file = app.settings_dir().join(VIEWS_FILE);
    let groups_before = block_writes(&groups_file);
    library
        .delete_camera(&driveway)
        .expect_err("the group could not be saved");
    drop(library);
    allow_writes(&groups_file, &groups_before);
    assert!(names(&groups_file, &driveway) && names(&views_file, &driveway));

    let reopened = Library::beside(&app.settings());

    assert!(reopened.problems().is_empty(), "{:?}", reopened.problems());
    assert!(reopened.tree()[0].cameras.is_empty());
    assert!(!names(&groups_file, &driveway), "cleared out as it opened");
    assert!(!names(&views_file, &driveway));
}

#[test]
fn hand_written_files_are_used_and_tidied() {
    let app = AppFolder::new();
    fs::create_dir_all(app.settings_dir()).unwrap();
    fs::write(
        app.settings_dir().join(CAMERAS_FILE),
        r#"{ "cameras": [ { "id": "porch", "name": "Porch", "host": "10.0.0.7" }, { "name": "Shed" } ] }"#,
    )
    .unwrap();
    fs::write(
        app.settings_dir().join(GROUPS_FILE),
        r#"{ "groups": [ { "id": "yard", "name": "Yard", "cameras": ["porch", "porhc"] } ] }"#,
    )
    .unwrap();

    let library = Library::beside(&app.settings());

    let porch = library.camera(&CameraId::from("porch")).unwrap();
    assert_eq!(porch.host, "10.0.0.7");
    assert_eq!(porch.rtsp_port, 554, "a field left out keeps its default");
    let shed = library
        .cameras()
        .into_iter()
        .find(|camera| camera.name == "Shed")
        .unwrap();
    assert!(!shed.id.is_empty(), "a camera with no id is given one");

    assert_eq!(
        library.tree()[0].cameras,
        [CameraId::from("porch")],
        "the typo matches no camera, so the group drops it"
    );
    assert!(library.problems().is_empty());

    let saved = json(&app.settings_dir().join(CAMERAS_FILE));
    assert_eq!(
        saved["cameras"][1]["id"],
        shed.id.as_str(),
        "the new id was saved"
    );
}
