//! `projects.json`: the folders opened as projects, the one open, and each one's tabs. A project
//! is known by its folder's path.

use std::path::{Path, PathBuf};

use dp_store::{Document, FileStore, Json, StoreError};
use serde::{Deserialize, Serialize};

use crate::ViewMode;

/// The projects file's name.
pub(crate) const FILE: &str = "projects.json";

/// Everything `projects.json` holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedProjects {
    /// The open project's folder; `None` shows New.
    pub selected: Option<PathBuf>,
    /// The projects, newest first.
    pub projects: Vec<Project>,
}

/// A project: a folder of Markdown files the user opened.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    /// The folder.
    pub path: PathBuf,
    /// The file it had in front last.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_file: Option<String>,
    /// The files it had open in tabs, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<Tab>,
}

/// A file open in a tab of a project.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tab {
    /// The file, relative to the project's folder.
    pub file: String,
    /// Kept first, and left open by closing the others.
    #[serde(skip_serializing_if = "is_false")]
    pub pinned: bool,
    /// The tab the next file opened with a single press takes over.
    #[serde(skip_serializing_if = "is_false")]
    pub preview: bool,
    /// How it shows.
    pub mode: ViewMode,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl Project {
    /// The folder's name, or its whole path for a drive's root.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// Whether this is the project of `folder`.
    pub fn is(&self, folder: &Path) -> bool {
        same_path(&self.path, folder)
    }
}

impl Document for SavedProjects {
    type Format = Json;

    fn moved_problem(path: &Path, error: &str, moved: &Path) -> String {
        format!(
            "{} could not be read ({error}). It was moved to {} and the app started with no \
             projects.",
            path.display(),
            moved.display()
        )
    }

    fn kept_problem(path: &Path, reason: &str) -> String {
        format!(
            "The app started with no projects and will not save over {}, because {reason}.",
            path.display()
        )
    }
}

/// The projects file and the copy of it the app is running with.
pub struct ProjectStore {
    file: FileStore<SavedProjects>,
}

impl ProjectStore {
    pub(crate) fn open(path: PathBuf) -> Self {
        Self {
            file: FileStore::open(path),
        }
    }

    pub(crate) fn file(&self) -> &FileStore<SavedProjects> {
        &self.file
    }

    /// What went wrong opening the file, if anything did.
    pub fn problem(&self) -> Option<&str> {
        self.file.problem()
    }

    /// Where the projects file is.
    pub fn path(&self) -> &Path {
        self.file.path()
    }

    /// The projects as they are now.
    pub fn current(&self) -> SavedProjects {
        self.file.current()
    }

    /// Bumped by every saved change.
    pub fn revision(&self) -> u64 {
        self.file.revision()
    }

    /// Opens `folder` as a project, moved first in the list, and selects it.
    pub fn open_project(&self, folder: &Path) -> Result<(), StoreError> {
        if !folder.is_dir() {
            return Err(StoreError::Invalid(format!(
                "{} isn't a folder. Choose a folder of Markdown files.",
                folder.display()
            )));
        }
        let folder = tidy(folder);
        self.file.update(|saved| {
            let project = match saved.projects.iter().position(|each| each.is(&folder)) {
                Some(at) => saved.projects.remove(at),
                None => Project {
                    path: folder.clone(),
                    ..Project::default()
                },
            };
            saved.selected = Some(project.path.clone());
            saved.projects.insert(0, project);
            Ok(())
        })
    }

    /// Selects the project of `folder`, or none with `None` (New).
    pub fn select_project(&self, folder: Option<&Path>) -> Result<(), StoreError> {
        self.file.update(|saved| {
            saved.selected = match folder {
                None => None,
                Some(folder) => Some(
                    saved
                        .projects
                        .iter()
                        .find(|each| each.is(folder))
                        .map(|each| each.path.clone())
                        .ok_or_else(|| not_a_project(folder))?,
                ),
            };
            Ok(())
        })
    }

    /// Takes the project of `folder` off the list; if it was open, the next one opens.
    pub fn remove_project(&self, folder: &Path) -> Result<(), StoreError> {
        self.file.update(|saved| {
            let at = saved
                .projects
                .iter()
                .position(|each| each.is(folder))
                .ok_or_else(|| not_a_project(folder))?;
            saved.projects.remove(at);
            if saved
                .selected
                .as_deref()
                .is_some_and(|selected| same_path(selected, folder))
            {
                let next = saved.projects.get(at).or(saved.projects.last());
                saved.selected = next.map(|project| project.path.clone());
            }
            Ok(())
        })
    }

    /// Remembers the project's tabs in order and `front`, the file in front.
    pub fn set_tabs(
        &self,
        folder: &Path,
        tabs: Vec<Tab>,
        front: Option<String>,
    ) -> Result<(), StoreError> {
        self.file.update(|saved| {
            let project = saved
                .projects
                .iter_mut()
                .find(|each| each.is(folder))
                .ok_or_else(|| not_a_project(folder))?;
            project.tabs = tabs;
            project.last_file = front;
            Ok(())
        })
    }
}

fn not_a_project(folder: &Path) -> StoreError {
    StoreError::NotFound(format!("{} isn't one of the projects.", folder.display()))
}

/// Drops a trailing separator so a folder isn't listed twice; a drive's root keeps its own (`D:\`).
fn tidy(folder: &Path) -> PathBuf {
    let text = folder.to_string_lossy();
    let trimmed = text.trim_end_matches(['\\', '/']);
    if trimmed.is_empty() || trimmed.ends_with(':') {
        return folder.to_path_buf();
    }
    PathBuf::from(trimmed)
}

/// On Windows, case and slash direction are ignored.
fn same_path(a: &Path, b: &Path) -> bool {
    let text = |path: &Path| {
        let tidied = tidy(path).to_string_lossy().into_owned();
        if cfg!(windows) {
            tidied.replace('/', "\\").to_lowercase()
        } else {
            tidied
        }
    };
    text(a) == text(b)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn tab(file: &str) -> Tab {
        Tab {
            file: file.into(),
            ..Tab::default()
        }
    }

    fn store() -> (TempDir, ProjectStore, PathBuf, PathBuf) {
        let dir = TempDir::new().expect("a temporary folder");
        let notes = dir.path().join("notes");
        let blog = dir.path().join("blog");
        fs::create_dir(&notes).expect("notes");
        fs::create_dir(&blog).expect("blog");
        let store = ProjectStore::open(dir.path().join(FILE));
        (dir, store, notes, blog)
    }

    #[test]
    fn opening_a_folder_lists_it_first_and_selects_it() {
        let (_dir, store, notes, blog) = store();
        store.open_project(&notes).expect("notes");
        store.open_project(&blog).expect("blog");

        let saved = store.current();
        let names: Vec<String> = saved.projects.iter().map(Project::name).collect();
        assert_eq!(names, ["blog", "notes"]);
        assert_eq!(saved.selected, Some(blog));
    }

    #[test]
    fn opening_a_project_again_moves_it_first_and_keeps_its_file() {
        let (_dir, store, notes, blog) = store();
        store.open_project(&notes).expect("notes");
        store
            .set_tabs(
                &notes,
                vec![tab("docs/README.md")],
                Some("docs/README.md".into()),
            )
            .expect("its file");
        store.open_project(&blog).expect("blog");

        // The same folder, written with a trailing separator.
        let mut again = notes.clone().into_os_string();
        again.push(std::path::MAIN_SEPARATOR_STR);
        store.open_project(Path::new(&again)).expect("notes again");

        let saved = store.current();
        assert_eq!(saved.projects.len(), 2);
        assert_eq!(saved.projects[0].path, notes);
        assert_eq!(
            saved.projects[0].last_file.as_deref(),
            Some("docs/README.md")
        );
        assert_eq!(saved.selected, Some(notes));
    }

    #[test]
    fn a_file_is_not_a_project() {
        let (dir, store, _, _) = store();
        let file = dir.path().join("README.md");
        fs::write(&file, "# Hi").expect("a file");

        let refused = store.open_project(&file).expect_err("a file");
        assert!(refused.to_string().contains("isn't a folder"));
        assert!(store.current().projects.is_empty());
    }

    #[test]
    fn removing_the_open_project_opens_the_next() {
        let (_dir, store, notes, blog) = store();
        store.open_project(&notes).expect("notes");
        store.open_project(&blog).expect("blog");

        store.remove_project(&blog).expect("blog goes");
        assert_eq!(store.current().selected, Some(notes.clone()));

        store.remove_project(&notes).expect("notes goes");
        assert_eq!(store.current().selected, None);
        assert!(store.current().projects.is_empty());
    }

    #[test]
    fn removing_another_project_keeps_the_open_one() {
        let (_dir, store, notes, blog) = store();
        store.open_project(&notes).expect("notes");
        store.open_project(&blog).expect("blog");

        store.remove_project(&notes).expect("notes goes");
        assert_eq!(store.current().selected, Some(blog));
    }

    #[test]
    fn selecting_none_shows_new_and_an_unknown_folder_is_refused() {
        let (dir, store, notes, _) = store();
        store.open_project(&notes).expect("notes");

        store.select_project(None).expect("New");
        assert_eq!(store.current().selected, None);

        store.select_project(Some(&notes)).expect("notes");
        assert_eq!(store.current().selected, Some(notes));

        assert!(
            store
                .select_project(Some(&dir.path().join("elsewhere")))
                .is_err()
        );
    }

    #[test]
    fn the_projects_read_back_as_written() {
        let (dir, store, notes, _) = store();
        store.open_project(&notes).expect("notes");
        store
            .set_tabs(&notes, vec![tab("README.md")], Some("README.md".into()))
            .expect("its file");

        let text = fs::read_to_string(dir.path().join(FILE)).expect("the file");
        let read: SavedProjects = serde_json::from_str(&text).expect("JSON");
        assert_eq!(read, store.current());
        assert!(text.contains("\"last_file\": \"README.md\""), "{text}");

        let again = ProjectStore::open(dir.path().join(FILE));
        assert_eq!(again.current(), store.current());
    }

    #[test]
    fn a_project_is_named_for_its_folder() {
        let project = Project {
            path: PathBuf::from("D:/Notes/field-notes"),
            ..Project::default()
        };
        assert_eq!(project.name(), "field-notes");
    }

    #[test]
    fn a_projects_tabs_read_back_in_their_order_with_the_one_in_front() {
        let (dir, store, notes, _) = store();
        store.open_project(&notes).expect("notes");
        let tabs = vec![
            Tab {
                file: "README.md".into(),
                pinned: true,
                ..Tab::default()
            },
            Tab {
                file: "docs/setup.md".into(),
                mode: ViewMode::Raw,
                ..Tab::default()
            },
            Tab {
                file: "docs/usage.md".into(),
                preview: true,
                ..Tab::default()
            },
        ];
        store
            .set_tabs(&notes, tabs.clone(), Some("docs/setup.md".into()))
            .expect("tabs");

        let again = ProjectStore::open(dir.path().join(FILE));
        let project = &again.current().projects[0];
        assert_eq!(project.tabs, tabs);
        assert_eq!(project.last_file.as_deref(), Some("docs/setup.md"));

        let text = fs::read_to_string(dir.path().join(FILE)).expect("the file");
        assert_eq!(text.matches("\"pinned\": true").count(), 1, "{text}");
        assert!(!text.contains("\"pinned\": false"), "{text}");
        assert!(!text.contains("\"preview\": false"), "{text}");
    }

    #[test]
    fn closing_every_tab_forgets_the_file_in_front() {
        let (_dir, store, notes, _) = store();
        store.open_project(&notes).expect("notes");
        store
            .set_tabs(
                &notes,
                vec![Tab {
                    file: "README.md".into(),
                    ..Tab::default()
                }],
                Some("README.md".into()),
            )
            .expect("a tab");
        store.set_tabs(&notes, Vec::new(), None).expect("none");

        let project = &store.current().projects[0];
        assert!(project.tabs.is_empty());
        assert_eq!(project.last_file, None);
        assert!(
            store
                .set_tabs(Path::new("elsewhere"), Vec::new(), None)
                .is_err()
        );
    }
}
