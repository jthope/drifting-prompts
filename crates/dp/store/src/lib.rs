//! A document saved in a file, written then renamed so a failure never leaves half a file.
//! A store never overwrites a file it could not read, and rereads one another program wrote since;
//! two writes in one tick of the file system's clock that leave the same size look alike.

pub mod sync;
mod window;

pub use window::{Appearance, WindowState};

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Serialize, de::DeserializeOwned};

use crate::sync::LockExt;

/// How a document is written to its file and read back.
pub trait Format {
    /// The format's name: "JSON".
    const NAME: &'static str;

    /// The document `text` holds, or why it holds none, on one line.
    fn parse<D: DeserializeOwned>(text: &str) -> Result<D, String>;

    /// `document` as the file's text.
    fn render<D: Serialize>(document: &D) -> Result<String, String>;
}

/// The JSON format.
pub struct Json;

impl Format for Json {
    const NAME: &'static str = "JSON";

    fn parse<D: DeserializeOwned>(text: &str) -> Result<D, String> {
        serde_json::from_str(text).map_err(|error| error.to_string())
    }

    fn render<D: Serialize>(document: &D) -> Result<String, String> {
        let mut text = serde_json::to_string_pretty(document).map_err(|error| error.to_string())?;
        text.push('\n');
        Ok(text)
    }
}

/// What a store keeps in its file: the whole file, as one value.
pub trait Document: Clone + Default + PartialEq + Serialize + DeserializeOwned {
    /// How the file is written.
    type Format: Format;

    /// The layout this build writes; a file with a higher number came from a newer app.
    const VERSION: u32 = 0;

    /// Whether the file is its owner's alone (0600 on Unix); on Windows, secrets come encrypted.
    const OWNER_ONLY: bool = false;

    /// The document's layout number, for one that carries one.
    fn version_mut(&mut self) -> Option<&mut u32> {
        None
    }

    /// Repairs what a hand edit can get wrong, such as a repeated id; whether anything changed.
    fn repair(&mut self) -> bool {
        false
    }

    /// What the user reads when the file could not be read and was moved to `moved`.
    fn moved_problem(path: &Path, error: &str, moved: &Path) -> String {
        format!(
            "{} could not be read ({error}). It was moved to {} and the app started without it.",
            path.display(),
            moved.display()
        )
    }

    /// What the user reads when the file is used but never written, because `reason`.
    fn kept_problem(path: &Path, reason: &str) -> String {
        format!(
            "{} is shown but will not be changed, because {reason}.",
            path.display()
        )
    }
}

/// Something a store could not do. The message says what, and names the file where there is one.
#[derive(Debug)]
pub enum StoreError {
    /// The file could not be read, so writing it would overwrite something the app never saw.
    ReadOnly {
        /// The store's file.
        path: PathBuf,
        /// Why it could not be read.
        reason: String,
    },
    /// The file could not be written.
    Write(WriteError),
    /// The data could not be turned into the file's text.
    Serialize(String),
    /// No camera, group or view has the id given.
    NotFound(String),
    /// The change makes no sense as asked, such as a group with no name.
    Invalid(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadOnly { path, reason } => {
                write!(f, "{} was not changed, because {reason}", path.display())
            }
            Self::Write(error) => write!(f, "{error}"),
            Self::Serialize(message) | Self::NotFound(message) | Self::Invalid(message) => {
                f.write_str(message)
            }
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Write(error) => Some(error),
            Self::ReadOnly { .. } | Self::Serialize(_) | Self::NotFound(_) | Self::Invalid(_) => {
                None
            }
        }
    }
}

/// A file that could not be written, with the path that failed in the message.
#[derive(Debug)]
pub enum WriteError {
    /// The folder holding the file could not be created.
    CreateDir {
        /// The folder.
        dir: PathBuf,
        /// What creating it reported.
        source: io::Error,
    },
    /// The file could not be written or renamed into place.
    Write {
        /// The file.
        path: PathBuf,
        /// What writing or renaming it reported.
        source: io::Error,
    },
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateDir { dir, source } => write!(f, "{}: {source}", dir.display()),
            Self::Write { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CreateDir { source, .. } | Self::Write { source, .. } => Some(source),
        }
    }
}

/// One file and the document the app is using.
pub struct FileStore<D> {
    path: PathBuf,
    document: Mutex<D>,
    /// Held from taking the copy to adopting it, so two changes never start from one copy.
    /// Readers wait only for `document`, which is never held while the disk is busy.
    writing: Mutex<()>,
    revision: AtomicU64,
    read_only: Option<String>,
    problem: Option<String>,
    was_missing: bool,
    on_disk: bool,
    seen: Mutex<Option<Look>>,
}

type Look = (SystemTime, u64);

fn look_at(path: &Path) -> Option<Look> {
    let metadata = fs::metadata(path).ok().filter(fs::Metadata::is_file)?;
    Some((metadata.modified().ok()?, metadata.len()))
}

impl<D: Document> FileStore<D> {
    /// Opens the file at `path`; a missing one is written only by the first change.
    pub fn open(path: PathBuf) -> Self {
        let mut store = Self {
            path,
            on_disk: true,
            ..Self::in_memory()
        };

        match fs::read_to_string(&store.path) {
            Ok(text) => store.adopt_file(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => store.was_missing = true,
            Err(error) => store.refuse_writes(format!("it could not be read ({error})")),
        }
        // After any repair's save, so that write isn't taken for another program's.
        *store.seen.locked_mut() = look_at(&store.path);
        store
    }

    fn adopt_file(&mut self, text: &str) {
        match D::Format::parse::<D>(text) {
            Ok(mut document) => {
                let version = document.version_mut().map_or(0, |version| *version);
                if version > D::VERSION {
                    *self.document.locked_mut() = document;
                    self.refuse_writes(format!(
                        "it was saved by a newer version of the app (layout {version})"
                    ));
                    return;
                }
                if document.repair() {
                    stamp(&mut document);
                    // Saved now so new ids stay put; a failed save is left to the next change.
                    let _ = save(&self.path, &document);
                }
                *self.document.locked_mut() = document;
            }
            Err(error) => match move_aside(&self.path) {
                Ok(moved) => self.problem = Some(D::moved_problem(&self.path, &error, &moved)),
                Err(move_error) => self.refuse_writes(format!(
                    "it is not valid {} ({error}) and could not be moved aside ({move_error})",
                    D::Format::NAME
                )),
            },
        }
    }

    /// A store with no file, starting with the default document; changes stay in memory.
    pub fn in_memory() -> Self {
        Self {
            path: PathBuf::new(),
            document: Mutex::new(D::default()),
            writing: Mutex::new(()),
            revision: AtomicU64::new(0),
            read_only: None,
            problem: None,
            was_missing: false,
            on_disk: false,
            seen: Mutex::new(None),
        }
    }

    /// A store with no file that starts with `document`; changes stay in memory.
    pub fn holding(document: D) -> Self {
        let mut store = Self::in_memory();
        *store.document.locked_mut() = document;
        store
    }

    fn refuse_writes(&mut self, reason: String) {
        self.problem = Some(D::kept_problem(&self.path, &reason));
        self.read_only = Some(reason);
    }

    /// Where the file is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What went wrong opening the file, if anything did.
    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    /// Whether there was no file when the store opened.
    pub fn was_missing(&self) -> bool {
        self.was_missing
    }

    /// Bumped by every saved change.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    /// Looks at the document without copying it. `look` must be quick: a writer waits for it.
    pub fn read<R>(&self, look: impl FnOnce(&D) -> R) -> R {
        look(&self.document.locked())
    }

    /// A copy of the document as it is now.
    pub fn current(&self) -> D {
        self.read(D::clone)
    }

    /// Makes `change` to a copy of the file as it is now, saves the copy, then uses it.
    /// Nothing is written when `change` fails or changes nothing.
    pub fn update<R>(
        &self,
        change: impl FnOnce(&mut D) -> Result<R, StoreError>,
    ) -> Result<R, StoreError> {
        self.writable()?;
        let _writing = self.writing.locked();
        self.catch_up()?;
        let before = self.current();
        let mut after = before.clone();
        let result = change(&mut after)?;
        if after == before {
            return Ok(result);
        }

        stamp(&mut after);
        self.save(&after)?;

        *self.document.locked() = after;
        self.revision.fetch_add(1, Ordering::SeqCst);
        Ok(result)
    }

    /// Writes the document as it is, so a first launch leaves a file to look at and edit.
    pub fn create_file(&self) -> Result<(), StoreError> {
        self.writable()?;
        let _writing = self.writing.locked();
        let mut document = self.current();
        stamp(&mut document);
        self.save(&document)?;
        *self.document.locked() = document;
        Ok(())
    }

    /// Takes another program's change to the file, if any; whether that changed anything.
    pub fn refresh(&self) -> bool {
        let _writing = self.writing.locked();
        // An unreadable file is looked at again next time.
        self.catch_up().unwrap_or(false)
    }

    /// Rereads the file when it changed since this store last looked. The caller holds `writing`.
    fn catch_up(&self) -> Result<bool, StoreError> {
        if !self.on_disk || self.read_only.is_some() {
            return Ok(false);
        }
        let now = look_at(&self.path);
        if now == *self.seen.locked() {
            return Ok(false);
        }
        if now.is_none() {
            *self.seen.locked() = None;
            return Ok(false);
        }

        let unread = |reason: String| StoreError::ReadOnly {
            path: self.path.clone(),
            reason,
        };
        let text = fs::read_to_string(&self.path).map_err(|error| {
            unread(format!(
                "another program changed it and it could not be read ({error})"
            ))
        })?;
        let mut document: D = D::Format::parse(&text).map_err(|error| {
            unread(format!(
                "another program changed it and it is not valid {} ({error})",
                D::Format::NAME
            ))
        })?;
        let version = document.version_mut().map_or(0, |version| *version);
        if version > D::VERSION {
            return Err(unread(format!(
                "it was saved by a newer version of the app (layout {version})"
            )));
        }
        // In memory only: the next change saves the repair.
        document.repair();

        // A write between the look and the read makes the file look different next time too.
        *self.seen.locked() = now;
        let changed = {
            let mut current = self.document.locked();
            let changed = *current != document;
            if changed {
                *current = document;
            }
            changed
        };
        if changed {
            self.revision.fetch_add(1, Ordering::SeqCst);
        }
        Ok(changed)
    }

    fn save(&self, document: &D) -> Result<(), StoreError> {
        if self.on_disk {
            save(&self.path, document)?;
            *self.seen.locked() = look_at(&self.path);
        }
        Ok(())
    }

    fn writable(&self) -> Result<(), StoreError> {
        match &self.read_only {
            Some(reason) => Err(StoreError::ReadOnly {
                path: self.path.clone(),
                reason: reason.clone(),
            }),
            None => Ok(()),
        }
    }
}

/// A store over one file.
pub trait Stored {
    /// What the file holds.
    type Doc: Document;

    /// The file.
    fn file(&self) -> &FileStore<Self::Doc>;

    /// Where the file is.
    fn path(&self) -> &Path {
        self.file().path()
    }

    /// What went wrong opening the file.
    fn problem(&self) -> Option<&str> {
        self.file().problem()
    }

    /// Whether there was no file when the store opened.
    fn was_missing(&self) -> bool {
        self.file().was_missing()
    }

    /// Bumped by every saved change.
    fn revision(&self) -> u64 {
        self.file().revision()
    }

    /// Takes another program's change, if any; it reads a file, so never on a window's thread.
    fn refresh(&self) -> bool {
        self.file().refresh()
    }

    /// Writes the document as it is, so a first launch leaves a file to look at and edit.
    fn create_file(&self) -> Result<(), StoreError> {
        self.file().create_file()
    }
}

fn stamp<D: Document>(document: &mut D) {
    if let Some(version) = document.version_mut() {
        *version = D::VERSION;
    }
}

fn save<D: Document>(path: &Path, document: &D) -> Result<(), StoreError> {
    let text = D::Format::render(document).map_err(StoreError::Serialize)?;
    write_atomically(path, &text, D::OWNER_ONLY).map_err(StoreError::Write)
}

fn write_atomically(path: &Path, text: &str, owner_only: bool) -> Result<(), WriteError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|source| WriteError::CreateDir {
        dir: dir.to_path_buf(),
        source,
    })?;

    let mut temp_name = path.file_name().unwrap_or_default().to_os_string();
    temp_name.push(".tmp");
    let temp = path.with_file_name(temp_name);

    write_file(&temp, text, owner_only).map_err(|source| WriteError::Write {
        path: temp.clone(),
        source,
    })?;
    fs::rename(&temp, path).map_err(|source| WriteError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// The mode is set while the file is empty, narrowing a leftover temporary file first.
#[cfg(unix)]
fn write_file(path: &Path, text: &str, owner_only: bool) -> io::Result<()> {
    use std::{
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };

    if !owner_only {
        return fs::write(path, text);
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(text.as_bytes())
}

#[cfg(not(unix))]
fn write_file(path: &Path, text: &str, _owner_only: bool) -> io::Result<()> {
    fs::write(path, text)
}

fn move_aside(path: &Path) -> io::Result<PathBuf> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();

    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".broken-{seconds}"));
    let moved = path.with_file_name(name);

    fs::rename(path, &moved)?;
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use tempfile::TempDir;

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Notes {
        version: u32,
        items: Vec<String>,
    }

    impl Document for Notes {
        type Format = Json;
        const VERSION: u32 = 2;

        fn version_mut(&mut self) -> Option<&mut u32> {
            Some(&mut self.version)
        }

        fn repair(&mut self) -> bool {
            let before = self.items.len();
            self.items.retain(|item| !item.trim().is_empty());
            self.items.len() != before
        }
    }

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Plain {
        items: Vec<String>,
    }

    impl Document for Plain {
        type Format = Json;
    }

    #[cfg(unix)]
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Secret {
        items: Vec<String>,
    }

    #[cfg(unix)]
    impl Document for Secret {
        type Format = Json;
        const OWNER_ONLY: bool = true;
    }

    fn add(store: &FileStore<Notes>, item: &str) -> Result<(), StoreError> {
        store.update(|notes| {
            notes.items.push(item.to_string());
            Ok(())
        })
    }

    #[test]
    fn a_missing_file_starts_empty_and_writes_nothing_until_a_change() {
        let folder = TempDir::new().unwrap();
        let store = FileStore::<Notes>::open(folder.path().join("notes.json"));

        assert!(store.was_missing());
        assert!(store.problem().is_none());
        assert_eq!(store.current(), Notes::default());
        assert!(!folder.path().join("notes.json").exists());
    }

    #[test]
    fn a_change_is_saved_then_used_and_bumps_the_revision() {
        let folder = TempDir::new().unwrap();
        let store = FileStore::<Notes>::open(folder.path().join("notes.json"));

        add(&store, "first").unwrap();

        assert_eq!(store.current().items, ["first"]);
        assert_eq!(store.revision(), 1);

        let reopened = FileStore::<Notes>::open(folder.path().join("notes.json"));
        assert!(!reopened.was_missing());
        assert_eq!(reopened.current().items, ["first"]);
        assert_eq!(
            reopened.current().version,
            Notes::VERSION,
            "a saved file says which layout it has"
        );
    }

    #[test]
    fn a_change_that_changes_nothing_writes_nothing() {
        let folder = TempDir::new().unwrap();
        let store = FileStore::<Notes>::open(folder.path().join("notes.json"));

        store.update(|_| Ok(())).unwrap();

        assert_eq!(store.revision(), 0);
        assert!(!folder.path().join("notes.json").exists());
    }

    #[test]
    fn a_failed_change_writes_nothing_and_keeps_the_document() {
        let folder = TempDir::new().unwrap();
        let store = FileStore::<Notes>::open(folder.path().join("notes.json"));
        add(&store, "kept").unwrap();

        let error = store
            .update(|notes| {
                notes.items.clear();
                Err::<(), _>(StoreError::Invalid("no".into()))
            })
            .unwrap_err();

        assert_eq!(error.to_string(), "no");
        assert_eq!(store.current().items, ["kept"]);
        assert_eq!(store.revision(), 1);
    }

    #[test]
    fn a_write_that_fails_changes_nothing() {
        let folder = TempDir::new().unwrap();
        let store = FileStore::<Notes>::open(folder.path().join("notes.json"));
        // A folder where the file should go makes the rename fail.
        fs::create_dir_all(folder.path().join("notes.json")).unwrap();

        let error = add(&store, "lost").unwrap_err();

        assert!(matches!(error, StoreError::Write(_)), "{error}");
        assert!(store.current().items.is_empty());
        assert_eq!(store.revision(), 0);
    }

    #[test]
    fn a_file_that_is_not_json_is_moved_aside_never_overwritten() {
        let folder = TempDir::new().unwrap();
        let broken = "{ this is not json";
        fs::write(folder.path().join("notes.json"), broken).unwrap();

        let store = FileStore::<Notes>::open(folder.path().join("notes.json"));

        let problem = store.problem().expect("the user must be told");
        assert!(problem.contains("moved to"), "{problem}");
        assert!(
            problem.ends_with("the app started without it."),
            "{problem}"
        );
        assert!(store.current().items.is_empty());
        assert!(!store.was_missing(), "a broken file is not a first launch");

        let kept: Vec<_> = fs::read_dir(folder.path())
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("notes.json.broken-")
            })
            .collect();
        assert_eq!(kept.len(), 1, "the broken file is kept beside it");
        assert_eq!(fs::read_to_string(kept[0].path()).unwrap(), broken);

        add(&store, "fresh").unwrap();
        assert_eq!(
            FileStore::<Notes>::open(folder.path().join("notes.json"))
                .current()
                .items,
            ["fresh"]
        );
    }

    #[test]
    fn a_file_from_a_newer_app_is_read_but_never_written() {
        let folder = TempDir::new().unwrap();
        let newer = r#"{ "version": 3, "items": ["from the future"], "colors": ["red"] }"#;
        fs::write(folder.path().join("notes.json"), newer).unwrap();

        let store = FileStore::<Notes>::open(folder.path().join("notes.json"));

        assert_eq!(store.current().items, ["from the future"]);
        assert!(store.problem().unwrap().contains("newer version"));

        let error = add(&store, "mine").unwrap_err();
        assert!(matches!(error, StoreError::ReadOnly { .. }), "{error}");
        assert!(store.create_file().is_err());
        assert_eq!(
            fs::read_to_string(folder.path().join("notes.json")).unwrap(),
            newer,
            "fields this build does not know must survive"
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_never_written() {
        let folder = TempDir::new().unwrap();
        // A folder stands in for a file that can't be read.
        fs::create_dir_all(folder.path().join("notes.json")).unwrap();

        let store = FileStore::<Notes>::open(folder.path().join("notes.json"));

        assert!(store.problem().unwrap().contains("could not be read"));
        assert!(!store.was_missing());
        assert!(matches!(
            add(&store, "lost").unwrap_err(),
            StoreError::ReadOnly { .. }
        ));
    }

    #[test]
    fn a_repaired_file_is_saved_straight_away() {
        let folder = TempDir::new().unwrap();
        fs::write(
            folder.path().join("notes.json"),
            r#"{ "items": ["kept", "  ", "also kept"] }"#,
        )
        .unwrap();

        let store = FileStore::<Notes>::open(folder.path().join("notes.json"));

        assert_eq!(store.current().items, ["kept", "also kept"]);
        let saved: Notes =
            serde_json::from_str(&fs::read_to_string(folder.path().join("notes.json")).unwrap())
                .unwrap();
        assert_eq!(saved.items, ["kept", "also kept"]);
        assert_eq!(saved.version, Notes::VERSION);
        assert_eq!(store.revision(), 0, "a repair is not a change by the user");
    }

    #[test]
    fn create_file_leaves_an_empty_file_to_edit() {
        let folder = TempDir::new().unwrap();
        let store = FileStore::<Notes>::open(folder.path().join("notes.json"));

        store.create_file().unwrap();

        let text = fs::read_to_string(folder.path().join("notes.json")).unwrap();
        assert_eq!(text, "{\n  \"version\": 2,\n  \"items\": []\n}\n");
    }

    #[test]
    fn a_store_with_no_file_keeps_its_changes_in_memory() {
        let store = FileStore::<Notes>::in_memory();

        add(&store, "kept").unwrap();
        store.create_file().unwrap();

        assert_eq!(store.current().items, ["kept"]);
        assert_eq!(store.revision(), 1, "the page still hears about the change");
        assert!(!store.was_missing(), "it is not a first launch");
        assert!(store.problem().is_none());
    }

    #[test]
    fn two_programs_on_one_file_keep_each_others_changes() {
        let folder = TempDir::new().unwrap();
        let path = folder.path().join("notes.json");
        let desktop = FileStore::<Notes>::open(path.clone());
        let server = FileStore::<Notes>::open(path.clone());

        add(&desktop, "from the desktop").unwrap();
        add(&server, "from the server, which is longer").unwrap();
        add(&desktop, "the desktop again").unwrap();

        let saved = [
            "from the desktop",
            "from the server, which is longer",
            "the desktop again",
        ];
        assert_eq!(desktop.current().items, saved);
        assert_eq!(FileStore::<Notes>::open(path).current().items, saved);
    }

    #[test]
    fn refreshing_takes_another_programs_change_and_bumps_the_revision() {
        let folder = TempDir::new().unwrap();
        let path = folder.path().join("notes.json");
        let desktop = FileStore::<Notes>::open(path.clone());
        let server = FileStore::<Notes>::open(path);
        assert!(!server.refresh(), "nothing has changed");

        add(&desktop, "from the desktop").unwrap();
        assert!(!desktop.refresh(), "its own change is no news to it");
        assert_eq!(desktop.revision(), 1);

        assert!(server.refresh());
        assert_eq!(server.current().items, ["from the desktop"]);
        assert_eq!(server.revision(), 1, "so the page looks again");
        assert!(!server.refresh(), "taken once");
    }

    #[test]
    fn a_file_another_program_broke_refuses_the_change_and_is_left_alone() {
        let folder = TempDir::new().unwrap();
        let path = folder.path().join("notes.json");
        let store = FileStore::<Notes>::open(path.clone());
        add(&store, "kept").unwrap();
        // Writes in one clock tick share a time, so the other program's file must differ in size.
        let broken = "{ this is not json, and somebody is still typing away at it";
        assert_ne!(
            fs::metadata(&path).unwrap().len(),
            broken.len() as u64,
            "the test needs a size of its own"
        );
        fs::write(&path, broken).unwrap();

        let error = add(&store, "lost").unwrap_err();

        assert!(matches!(error, StoreError::ReadOnly { .. }), "{error}");
        assert!(error.to_string().contains("another program"), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), broken);
        assert_eq!(store.current().items, ["kept"]);
        assert!(!store.refresh(), "and a refresh takes nothing from it");

        fs::write(&path, r#"{ "version": 2, "items": ["kept", "by hand"] }"#).unwrap();
        add(&store, "after").unwrap();
        assert_eq!(store.current().items, ["kept", "by hand", "after"]);
    }

    #[test]
    fn a_file_that_went_away_is_written_again_by_the_next_change() {
        let folder = TempDir::new().unwrap();
        let path = folder.path().join("notes.json");
        let store = FileStore::<Notes>::open(path.clone());
        add(&store, "kept").unwrap();
        fs::remove_file(&path).unwrap();

        assert!(!store.refresh());
        add(&store, "more").unwrap();

        assert_eq!(
            FileStore::<Notes>::open(path).current().items,
            ["kept", "more"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_owner_only_file_is_written_for_its_owner_alone() {
        use std::os::unix::fs::PermissionsExt;

        let folder = TempDir::new().unwrap();
        let path = folder.path().join("secret.json");
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let store = FileStore::<Secret>::open(path.clone());
        let add = |item: &str| {
            store.update(|secret| {
                secret.items.push(item.into());
                Ok(())
            })
        };

        add("first").unwrap();
        assert_eq!(mode(&path), 0o600);

        // A leftover temporary file readable by everyone is narrowed before anything is written.
        let temp = folder.path().join("secret.json.tmp");
        fs::write(&temp, "").unwrap();
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o644)).unwrap();
        add("second").unwrap();
        assert_eq!(mode(&path), 0o600);
        assert_eq!(
            FileStore::<Secret>::open(path).current().items,
            ["first", "second"]
        );
    }

    #[test]
    fn a_document_without_a_layout_number_round_trips() {
        let folder = TempDir::new().unwrap();
        let store = FileStore::<Plain>::open(folder.path().join("plain.json"));

        store
            .update(|plain| {
                plain.items.push("one".into());
                Ok(())
            })
            .unwrap();

        let text = fs::read_to_string(folder.path().join("plain.json")).unwrap();
        assert!(
            !text.contains("version"),
            "no layout number appears: {text}"
        );
        let reopened = FileStore::<Plain>::open(folder.path().join("plain.json"));
        assert_eq!(reopened.current().items, ["one"]);
    }
}
