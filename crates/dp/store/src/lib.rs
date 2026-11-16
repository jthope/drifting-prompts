//! A file an app saves its settings in, and the copy of it the app is running with: each
//! product's `settings.json`, Markdown Viewer's `projects.json`, and IP Camera's JSON files in
//! `data\settings`.
//!
//! [`FileStore`] does the same for each: a change is made to a copy, the copy is written
//! (write-then-rename, so a failure never leaves half a file), and only then does the app use it.
//! What differs is the [`Document`] a file holds: its [`Format`] (JSON), its layout number, what a
//! hand edit can get wrong, and whether the file is its owner's alone, as IP Camera's
//! `cameras.json` is. A product's own crate keeps one store for each of its
//! files (`ipcamera_core::config::ConfigStore`, `mdviewer_core::ConfigStore`), and says the rest
//! through [`Stored`].
//!
//! These files hold what a user typed in, which the defaults cannot give back. So a store never
//! overwrites a file it could not read:
//!
//! - A file that is not valid JSON is moved aside to `<name>.broken-<seconds>`, and the app starts
//!   without it.
//! - A file that cannot be read, cannot be moved aside, or was written by a newer version of the
//!   app is used as far as it can be but never written. Every change is refused, saying why.
//!
//! Either way [`FileStore::problem`] says what happened, for the app to tell the user.
//!
//! **Two programs, one file.** Two programs can work on the same files (IP Camera's desktop app
//! and its server), each with a copy of its own in memory. So a store remembers what the file
//! looked like when it last read or wrote it (when it was written, and its size), and before every
//! change it looks again: a file another program has written since is read first, and the change
//! starts from that, never from the older copy, which would write over the other program's change.
//! [`FileStore::refresh`] does the same with no change to make, for a program to learn what the
//! other did. Looking costs one request for the file's details, and nothing is read while they are
//! the same. A file that has changed and can't be read refuses that one change, saying why, and is
//! left as it is. Two writes in one tick of the file system's clock that leave the file the same
//! size look alike, so the second program can miss the first's change; a lock file held across the
//! read and the rename would close that if it ever shows.
//!
//! [`sync`] is how every lock in the suite is taken, and [`Appearance`] and [`WindowState`] are
//! what every product saves of its window (`window.rs`).

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
    /// The format's name, for a message: "JSON".
    const NAME: &'static str;

    /// The document `text` holds, or why it holds none, on one line.
    fn parse<D: DeserializeOwned>(text: &str) -> Result<D, String>;

    /// `document` as the file's text.
    fn render<D: Serialize>(document: &D) -> Result<String, String>;
}

/// Every file the suite saves: `settings.json`, `projects.json` and the files in `data\settings`.
pub struct Json;

impl Format for Json {
    const NAME: &'static str = "JSON";

    fn parse<D: DeserializeOwned>(text: &str) -> Result<D, String> {
        serde_json::from_str(text).map_err(|error| error.to_string())
    }

    /// Pretty, with a final newline, so the file reads well and diffs cleanly.
    fn render<D: Serialize>(document: &D) -> Result<String, String> {
        let mut text = serde_json::to_string_pretty(document).map_err(|error| error.to_string())?;
        text.push('\n');
        Ok(text)
    }
}

/// What a store keeps in its file: the whole file, as one value.
///
/// Like a C# interface whose members have default bodies: a document names its format and
/// overrides only what it does differently.
pub trait Document: Clone + Default + PartialEq + Serialize + DeserializeOwned {
    /// How the file is written.
    type Format: Format;

    /// The layout this build writes. A file with a higher number came from a newer app. A document
    /// without a layout number leaves this at 0.
    const VERSION: u32 = 0;

    /// Whether the file is for its owner's eyes only, as one holding passwords is. On Unix it is
    /// written readable and writable by its owner alone (mode 0600). Windows has no such mode: a
    /// file there takes its folder's permissions, and what must stay secret is encrypted before it
    /// reaches the document.
    const OWNER_ONLY: bool = false;

    /// The layout number the document carries, for one that carries one. A file without it reads
    /// as 0.
    fn version_mut(&mut self) -> Option<&mut u32> {
        None
    }

    /// Repairs what a hand edit can get wrong, such as a missing or repeated id. Returns whether
    /// anything changed.
    fn repair(&mut self) -> bool {
        false
    }

    /// What the user reads when the file at `path` could not be read, because of `error`, and was
    /// moved to `moved`.
    fn moved_problem(path: &Path, error: &str, moved: &Path) -> String {
        format!(
            "{} could not be read ({error}). It was moved to {} and the app started without it.",
            path.display(),
            moved.display()
        )
    }

    /// What the user reads when the file at `path` is used as far as it can be but never written,
    /// because `reason`.
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
    /// The file could not be read, when the app started or after another program changed it, so
    /// writing it would overwrite something the app never saw.
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
    /// Held from taking the copy to adopting it, so two changes can never start from the same copy
    /// and lose one, or write the same temporary file at once. Readers never wait for it, only for
    /// `document`, which is never held while the disk is busy.
    writing: Mutex<()>,
    /// Bumped by every change that was saved, so the app can tell when to look again.
    revision: AtomicU64,
    /// Why the file must not be written, when it must not.
    read_only: Option<String>,
    /// What went wrong opening the file, for the app to tell the user.
    problem: Option<String>,
    /// Whether there was no file when the store opened: how a first launch is told apart.
    was_missing: bool,
    /// Whether changes are written to `path`. A store with no file keeps them in memory only.
    on_disk: bool,
    /// What the file looked like when this store last read or wrote it; `None` when there was no
    /// file. A file that looks different now was written by another program.
    seen: Mutex<Option<Look>>,
}

/// What tells one writing of a file from the next without reading it: when it was written, and
/// its size.
type Look = (SystemTime, u64);

/// How the file at `path` looks now; `None` when there is no file there.
fn look_at(path: &Path) -> Option<Look> {
    let metadata = fs::metadata(path).ok().filter(fs::Metadata::is_file)?;
    Some((metadata.modified().ok()?, metadata.len()))
}

impl<D: Document> FileStore<D> {
    /// Opens the file at `path`. A missing file starts the default document and writes nothing
    /// until the first change.
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
        // After any repair was saved, so the store's own write isn't taken for another program's.
        *store.seen.locked_mut() = look_at(&store.path);
        store
    }

    /// Takes what the file held, or deals with it not being usable.
    fn adopt_file(&mut self, text: &str) {
        match D::Format::parse::<D>(text) {
            Ok(mut document) => {
                let version = document.version_mut().map_or(0, |version| *version);
                if version > D::VERSION {
                    *self.document.locked_mut() = document;
                    self.refuse_writes(format!(
                        "it was saved by a newer version of IP Camera (layout {version})"
                    ));
                    return;
                }
                if document.repair() {
                    stamp(&mut document);
                    // Saved now so new ids stay the same from launch to launch. If this fails, the
                    // next change saves them instead.
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

    /// A store with no file: it starts with the default document, and its changes are kept in
    /// memory only, gone when the app closes. The web preview's, which has no disk to save to.
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

    /// A store with no file that starts with `document`: a copy of one kept elsewhere, such as the
    /// server's, whose changes are kept in memory only.
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

    /// Makes `change` to a copy of the document, saves the copy, then uses it.
    ///
    /// Nothing is written, and the revision stays as it was, when `change` fails or changes
    /// nothing. When the write fails the app keeps the document it had, so it is never using
    /// something that is not on disk.
    ///
    /// The copy is of the file as it is now: one another program has written since this store
    /// last looked is read first, so `change` is made to what that program saved.
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

    /// Takes the file as it is now when another program has written it since this store last
    /// looked, and returns whether that changed anything, bumping the revision when it did. A
    /// store with no file, or one that never writes its file, has nothing to take.
    pub fn refresh(&self) -> bool {
        let _writing = self.writing.locked();
        // A file that can't be read just now is looked at again the next time.
        self.catch_up().unwrap_or(false)
    }

    /// Reads the file again when it no longer looks as it did when this store last read or wrote
    /// it. Returns whether the document changed. The caller holds `writing`.
    ///
    /// A file that is gone is left gone: the store keeps what it has, and its next change writes
    /// the file again. One that can't be read, or that a newer version of the app wrote, is an
    /// error, so the change that asked is refused rather than written over it.
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
                "it was saved by a newer version of IP Camera (layout {version})"
            )));
        }
        // In memory only: the next change saves the repair with it.
        document.repair();

        // Had the other program written again between the look and the read, the file looks
        // different from `now` the next time too, and is read once more.
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

    /// Writes `document` to the file, for a store that has one.
    fn save(&self, document: &D) -> Result<(), StoreError> {
        if self.on_disk {
            save(&self.path, document)?;
            *self.seen.locked() = look_at(&self.path);
        }
        Ok(())
    }

    /// Refuses a write to a file the app could not read.
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

/// A store over one file: what is the file's rather than the store's own.
///
/// Each store says which [`FileStore`] it keeps and gets the rest from here, the way a C#
/// interface with default methods would hand them out.
pub trait Stored {
    /// What the file holds.
    type Doc: Document;

    /// The file.
    fn file(&self) -> &FileStore<Self::Doc>;

    /// Where the file is.
    fn path(&self) -> &Path {
        self.file().path()
    }

    /// What went wrong opening the file, for the app to tell the user.
    fn problem(&self) -> Option<&str> {
        self.file().problem()
    }

    /// Whether there was no file when the store opened.
    fn was_missing(&self) -> bool {
        self.file().was_missing()
    }

    /// Bumped by every saved change, so the app can tell when to look again.
    fn revision(&self) -> u64 {
        self.file().revision()
    }

    /// Takes what another program has saved in the file since this store last looked; whether
    /// that changed anything. It reads a file, so it stays off a window's thread.
    fn refresh(&self) -> bool {
        self.file().refresh()
    }

    /// Writes the document as it is, so a first launch leaves a file to look at and edit.
    fn create_file(&self) -> Result<(), StoreError> {
        self.file().create_file()
    }
}

/// Marks `document` as written by this build.
fn stamp<D: Document>(document: &mut D) {
    if let Some(version) = document.version_mut() {
        *version = D::VERSION;
    }
}

fn save<D: Document>(path: &Path, document: &D) -> Result<(), StoreError> {
    let text = D::Format::render(document).map_err(StoreError::Serialize)?;
    write_atomically(path, &text, D::OWNER_ONLY).map_err(StoreError::Write)
}

/// Write-then-rename, so a failure never leaves a half-written file behind. Every file the app
/// saves goes through here. An `owner_only` file is its owner's alone on Unix
/// ([`Document::OWNER_ONLY`]).
fn write_atomically(path: &Path, text: &str, owner_only: bool) -> Result<(), WriteError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|source| WriteError::CreateDir {
        dir: dir.to_path_buf(),
        source,
    })?;

    // `settings.json.tmp`, `cameras.json.tmp`: the whole name plus `.tmp`.
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

/// Writes `text` to `path`, readable by its owner alone on Unix when `owner_only`. The mode is set
/// while the file is still empty, so its text is never readable by anyone else, even in a
/// temporary file a failed save left behind with a wider mode.
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

/// Writes `text` to `path`. Windows has no owner-only mode ([`Document::OWNER_ONLY`]).
#[cfg(not(unix))]
fn write_file(path: &Path, text: &str, _owner_only: bool) -> io::Result<()> {
    fs::write(path, text)
}

/// Renames an unreadable file to `<name>.broken-<seconds>`, keeping it for the user to look at.
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

    /// A small document of its own, so these tests are about the store and nothing else.
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

        /// Blank items are what a hand edit gets wrong here.
        fn repair(&mut self) -> bool {
            let before = self.items.len();
            self.items.retain(|item| !item.trim().is_empty());
            self.items.len() != before
        }
    }

    /// The same with no layout number and nothing to repair: what `settings.json` is.
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Plain {
        items: Vec<String>,
    }

    impl Document for Plain {
        type Format = Json;
    }

    /// The same, for its owner's eyes only.
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

        // The store still works: the next change starts a new file.
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
        // A folder can't be read as text, which stands in for a locked or forbidden file.
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
        // The server's copy is from before that: its change must start from the file.
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
        // A store tells writes apart by their time and size, and two writes in one tick of the
        // file system's clock share a time, so the other program's file must differ in size.
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

        // Once it reads again, the store carries on from what it holds.
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

        // A temporary file a failed save left behind, readable by everyone, is narrowed before
        // anything is written to it.
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
