//! Markdown Viewer's settings, saved as `settings.json` beside the executable: the window's
//! preferences and where the window was. The projects are `projects.json`'s ([`crate::projects`]).
//!
//! [`ConfigStore`] owns the file and the copy the app is running with, through the suite's store
//! (`dp_store`): a change is made to a copy, written (write-then-rename), and only then used, and
//! a file that can't be read is never written over. Like IP Camera's, the file sits beside the
//! executable, never in `%APPDATA%`. [`crate::Saved`] opens it, beside `projects.json`.

use std::path::{Path, PathBuf};

pub use dp_store::WindowState;
use dp_store::{Appearance, Document, FileStore, Json, StoreError};
use serde::{Deserialize, Serialize};

/// The settings file's name.
pub(crate) const FILE: &str = "settings.json";

/// Everything `settings.json` holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The window's preferences.
    pub ui: Ui,
    /// Where the window was, to reopen there.
    pub window: WindowState,
}

/// How the files panel lists a project's files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilesView {
    /// The folders nested as they are on disk, each opening and closing.
    #[default]
    Tree,
    /// One heading for each folder that holds files, its whole path, with its files under it.
    Folder,
    /// Every file in one list, each with its folder beside it.
    List,
}

/// How a file shows: its text as written, the page it makes, or both side by side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewMode {
    /// The Markdown as written, highlighted as a code editor highlights it.
    Raw,
    /// The text on the left and the page on the right.
    Split,
    /// The page the Markdown makes.
    #[default]
    Rendered,
}

/// What the tab strip does with more tabs than fit in one row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabOverflow {
    /// One row, which scrolls sideways.
    #[default]
    Scroll,
    /// As many rows as the tabs take.
    Wrap,
}

/// The window's preferences: its appearance, its panels and their sizes, how a file shows and how its
/// tabs fit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ui {
    /// How the window looks: its mode, themes, texture and elevation, among these fields in the
    /// file as in every product's.
    #[serde(flatten)]
    pub appearance: Appearance,
    /// The sidebar's width, in points.
    pub sidebar_width: f32,
    /// The files panel's width, in points.
    pub files_width: f32,
    /// Whether the files panel shows.
    pub files_open: bool,
    /// Whether the outline shows beside the file.
    pub outline_open: bool,
    /// Whether a map of the whole file shows beside each view of it.
    pub map_open: bool,
    /// How a file opened in a new tab shows; each tab keeps its own after that.
    pub mode: ViewMode,
    /// How the files panel lists the files.
    pub files_view: FilesView,
    /// What the tab strip does with more tabs than fit in one row.
    pub tab_overflow: TabOverflow,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            sidebar_width: 232.0,
            files_width: 240.0,
            files_open: true,
            outline_open: true,
            map_open: true,
            mode: ViewMode::Rendered,
            files_view: FilesView::Tree,
            tab_overflow: TabOverflow::Scroll,
        }
    }
}

impl Document for Config {
    type Format = Json;

    fn moved_problem(path: &Path, error: &str, moved: &Path) -> String {
        format!(
            "{} could not be read ({error}). It was moved to {} and the app started with the \
             default settings.",
            path.display(),
            moved.display()
        )
    }

    fn kept_problem(path: &Path, reason: &str) -> String {
        format!(
            "The app started with the default settings and will not save over {}, because \
             {reason}.",
            path.display()
        )
    }
}

/// The settings file and the copy of it the app is running with. [`crate::Saved`] opens it.
pub struct ConfigStore {
    file: FileStore<Config>,
}

impl ConfigStore {
    /// Opens the file at `path`, writing nothing: [`crate::Saved`] writes the defaults when it is
    /// missing.
    pub(crate) fn open(path: PathBuf) -> Self {
        Self {
            file: FileStore::open(path),
        }
    }

    /// The file underneath, for [`crate::Saved`].
    pub(crate) fn file(&self) -> &FileStore<Config> {
        &self.file
    }

    /// What went wrong opening the file, if anything did.
    pub fn problem(&self) -> Option<&str> {
        self.file.problem()
    }

    /// Where the settings file is.
    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// The settings as they are now.
    pub fn current(&self) -> Config {
        self.file.current()
    }

    /// Bumped by every saved change, so the app can tell when to look again.
    pub fn revision(&self) -> u64 {
        self.file.revision()
    }

    /// Saves a change to the settings. A change that changes nothing writes nothing.
    pub fn update(&self, change: impl FnOnce(&mut Config)) -> Result<(), StoreError> {
        self.file.update(|config| {
            change(config);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn store() -> (TempDir, ConfigStore) {
        let dir = TempDir::new().expect("a temporary folder");
        let store = ConfigStore::open(dir.path().join(FILE));
        (dir, store)
    }

    #[test]
    fn the_settings_read_back_as_written() {
        let (dir, store) = store();
        store
            .update(|config| {
                config.ui.mode = ViewMode::Split;
                config.window.width = 1200.0;
            })
            .expect("preferences");

        let text = fs::read_to_string(dir.path().join(FILE)).expect("the file");
        let read: Config = serde_json::from_str(&text).expect("JSON");
        assert_eq!(read, store.current());
        assert!(text.contains("\"mode\": \"split\""), "{text}");

        let again = ConfigStore::open(dir.path().join(FILE));
        assert_eq!(again.current(), store.current());
    }

    #[test]
    fn the_files_view_starts_as_a_tree_and_reads_back() {
        let (dir, store) = store();
        assert_eq!(store.current().ui.files_view, FilesView::Tree);
        store
            .update(|config| config.ui.files_view = FilesView::Folder)
            .expect("the view");
        let text = fs::read_to_string(dir.path().join(FILE)).expect("the file");
        assert!(text.contains("\"files_view\": \"folder\""), "{text}");
        assert_eq!(
            ConfigStore::open(dir.path().join(FILE))
                .current()
                .ui
                .files_view,
            FilesView::Folder
        );
    }

    #[test]
    fn the_tabs_start_in_one_row_that_scrolls_and_wrapping_reads_back() {
        let (dir, store) = store();
        assert_eq!(store.current().ui.tab_overflow, TabOverflow::Scroll);
        store
            .update(|config| config.ui.tab_overflow = TabOverflow::Wrap)
            .expect("the tabs");
        let text = fs::read_to_string(dir.path().join(FILE)).expect("the file");
        assert!(text.contains("\"tab_overflow\": \"wrap\""), "{text}");
        assert_eq!(
            ConfigStore::open(dir.path().join(FILE))
                .current()
                .ui
                .tab_overflow,
            TabOverflow::Wrap
        );
    }

    #[test]
    fn a_missing_field_keeps_its_default() {
        let config: Config = serde_json::from_str(r#"{ "ui": { "files_open": false } }"#)
            .expect("a hand-edited file");
        assert!(!config.ui.files_open);
        assert_eq!(config.ui.files_width, 240.0);
        assert_eq!(config.ui.mode, ViewMode::Rendered);
        assert_eq!(config.ui.tab_overflow, TabOverflow::Scroll);
        assert_eq!(config.window, WindowState::default());
    }
}
