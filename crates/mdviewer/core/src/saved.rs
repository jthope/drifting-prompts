//! Everything Markdown Viewer saves, opened together: `settings.json` (the window's preferences
//! and place, [`ConfigStore`]) and `projects.json` (the projects, [`ProjectStore`]), side by side
//! beside the executable.
//!
//! Each file still missing is written from the defaults as they open, so there is a file to look
//! at and edit from the first launch.

use std::path::{Path, PathBuf};

use crate::{
    ConfigStore, ProjectStore, config::FILE as SETTINGS_FILE, projects::FILE as PROJECTS_FILE,
};

/// The app's settings and its projects, each in a file of its own.
pub struct Saved {
    /// `settings.json`: the window's preferences and where it was.
    pub settings: ConfigStore,
    /// `projects.json`: the projects and the one open.
    pub projects: ProjectStore,
}

impl Saved {
    /// Opens the two files in `folder`. A file that can't be used never stops the app: it is
    /// moved aside or left alone, and [`Saved::problem`] says which.
    pub fn open(folder: &Path) -> Self {
        let settings = ConfigStore::open(folder.join(SETTINGS_FILE));
        let projects = ProjectStore::open(folder.join(PROJECTS_FILE));
        // If writing either fails, its first save writes it instead. One that is there but could
        // not be read is left alone, as every write to it is refused.
        if !settings.path().exists() {
            let _ = settings.file().create_file();
        }
        if !projects.path().exists() {
            let _ = projects.file().create_file();
        }
        Self { settings, projects }
    }

    /// The two files beside the executable.
    pub fn beside_the_executable() -> Self {
        let folder = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        Self::open(&folder)
    }

    /// What went wrong opening the files, if anything did: the first, for the app to say once.
    pub fn problem(&self) -> Option<&str> {
        self.settings.problem().or(self.projects.problem())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::{Config, SavedProjects, ViewMode};

    #[test]
    fn a_first_launch_writes_both_files_with_the_defaults() {
        let dir = TempDir::new().expect("a temporary folder");
        let saved = Saved::open(dir.path());

        assert!(saved.problem().is_none(), "{:?}", saved.problem());
        assert!(dir.path().join("settings.json").exists());
        assert!(dir.path().join("projects.json").exists());
        assert_eq!(saved.settings.current(), Config::default());
        assert_eq!(saved.settings.current().ui.mode, ViewMode::Rendered);
        assert_eq!(saved.projects.current(), SavedProjects::default());
    }

    #[test]
    fn each_file_holds_its_own_part() {
        let dir = TempDir::new().expect("a temporary folder");
        let notes = dir.path().join("notes");
        fs::create_dir(&notes).expect("notes");
        let saved = Saved::open(dir.path());
        saved
            .settings
            .update(|config| config.ui.files_width = 300.0)
            .expect("a preference");
        saved.projects.open_project(&notes).expect("a project");

        let settings_text = fs::read_to_string(dir.path().join("settings.json")).unwrap();
        let projects_text = fs::read_to_string(dir.path().join("projects.json")).unwrap();
        assert!(!settings_text.contains("projects"), "{settings_text}");
        assert!(!projects_text.contains("files_width"), "{projects_text}");

        let again = Saved::open(dir.path());
        assert_eq!(again.settings.current().ui.files_width, 300.0);
        assert_eq!(again.projects.current().selected, Some(notes));
    }

    #[test]
    fn a_projects_file_that_is_not_json_is_said_once_and_the_app_starts_with_no_projects() {
        let dir = TempDir::new().expect("a temporary folder");
        fs::write(dir.path().join("projects.json"), "{ not json").unwrap();

        let saved = Saved::open(dir.path());

        let problem = saved.problem().expect("the user must be told");
        assert!(problem.contains("no projects"), "{problem}");
        assert!(saved.projects.current().projects.is_empty());
    }
}
