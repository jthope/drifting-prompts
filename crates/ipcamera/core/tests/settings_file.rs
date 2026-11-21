//! The settings file as a user meets it, through the public API only: a first launch, a hand edit
//! and a save.

use std::{
    fs,
    path::{Path, PathBuf},
};

use ipcamera_core::config::{Config, ConfigStore};

struct SettingsFolder(tempfile::TempDir);

impl SettingsFolder {
    fn new() -> Self {
        Self(tempfile::TempDir::new().expect("a folder for the test"))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn settings_file(&self) -> PathBuf {
        self.path().join("settings.json")
    }

    fn load(&self) -> ConfigStore {
        ConfigStore::load(self.settings_file())
    }
}

fn defaults_as_json(config: &Config) -> String {
    let mut text = serde_json::to_string_pretty(config).expect("the defaults must serialize");
    text.push('\n');
    text
}

#[test]
fn a_first_launch_writes_the_shipped_defaults() {
    let temp = SettingsFolder::new();
    let config = temp.load();

    assert_eq!(config.path(), Path::new(&temp.settings_file()));
    let text = fs::read_to_string(temp.settings_file())
        .expect("the first launch must create the file, so it can be edited by hand");
    assert_eq!(text, defaults_as_json(&Config::default()));
    assert!(config.problem().is_none());

    assert_eq!(
        config.snapshots_dir(),
        temp.path().join("data").join("snapshots")
    );
}

#[test]
fn a_hand_edited_file_is_used_and_missing_fields_keep_their_defaults() {
    let temp = SettingsFolder::new();
    fs::write(
        temp.settings_file(),
        r#"{ "ui": { "volume": 42.0 }, "storage": { "videos": "clips" } }"#,
    )
    .expect("the hand edit");

    let config = temp.load();

    assert_eq!(config.current().ui.volume, 42.0);
    assert_eq!(config.current().storage.videos, "clips");
    assert_eq!(
        config.current().ui.rail_width,
        248.0,
        "a field left out of the file keeps its default"
    );
}

#[test]
fn a_saved_change_is_on_disk_for_the_next_launch() {
    let temp = SettingsFolder::new();
    let config = temp.load();

    config
        .update(|config| {
            config.storage.snapshots = "shots".into();
            config.window.maximized = true;
        })
        .expect("the save should succeed");

    assert_eq!(config.current().storage.snapshots, "shots");

    let reopened = temp.load();
    assert_eq!(reopened.current().storage.snapshots, "shots");
    assert!(reopened.current().window.maximized);
    assert_eq!(reopened.current().ui.rail_width, 248.0);

    reopened
        .update(|config| config.ui.volume = 12.0)
        .expect("the preference should save");
    assert_eq!(temp.load().current().ui.volume, 12.0);
}
