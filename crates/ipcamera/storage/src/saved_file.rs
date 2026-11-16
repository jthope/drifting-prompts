//! What snapshots and recordings share: how a saved file is named, found and listed.
//!
//! Both name a file for when it was made and which camera made it,
//! `<prefix><seconds>-<camera id>.<extension>`: `snapshot-1762560000-<id>.png` and
//! `video-1762560000-<id>.mp4`. The id, not the camera's name, so renaming a camera keeps its
//! files. Any other file of the kind put in the folder by hand lists with no camera.
//!
//! A file an event saved carries tags, what the camera saw as the file started, each after a dot
//! before the extension: `snapshot-1762560000-<id>.vehicle.png`,
//! `video-1762560000-<id>.person.vehicle.mp4`. A name keeps only a GUID's characters from the id
//! and from each tag, so a dot always starts a tag and the two never run together. A file taken by
//! hand has none. Tags in the name rather than beside it mean the listing is all a page reads to
//! filter by them, on the desktop and from the server alike, and that Explorer shows them too.
//!
//! The two kinds differ only in those names and in the words a message uses, so each is a
//! [`FileKind`] value rather than a type of its own: [`SNAPSHOT`] and [`VIDEO`]. What only one
//! kind does, writing a PNG or a thumbnail, stays in `snapshot` and `recording`.
//!
//! Windows updates a folder listing's size and time for a file only once the file is closed, so
//! [`FileKind::entries`] asks each file itself, which is up to date even while a camera writes it.

use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::error::{Result, StorageError};

/// One kind of file the app saves.
pub struct FileKind {
    /// What every name of this kind starts with.
    prefix: &'static str,
    /// Without the dot, as `Path::extension` gives it.
    extension: &'static str,
    /// One of them, as a message names it.
    noun: &'static str,
    /// Their folder, as a message names it.
    folder: &'static str,
}

/// Snapshots: `snapshot-<seconds>-<camera>.png`.
pub const SNAPSHOT: FileKind = FileKind {
    prefix: "snapshot-",
    extension: "png",
    noun: "snapshot",
    folder: "snapshots",
};

/// Recordings: `video-<seconds>-<camera>.mp4`.
pub const VIDEO: FileKind = FileKind {
    prefix: "video-",
    extension: "mp4",
    noun: "recording",
    folder: "videos",
};

/// A file of one kind, as [`FileKind::entries`] found it.
pub struct Entry {
    /// The file's name, which names it to every other call.
    pub name: String,
    /// Absolute, so `<Image src>` and `<Video src>` read it as a file rather than one of the app's
    /// own assets.
    pub path: PathBuf,
    /// Asked of the file itself, not the folder listing.
    pub metadata: fs::Metadata,
    /// When its name says it was made, in milliseconds since 1970; `None` when the name doesn't say.
    pub stamp_ms: Option<u64>,
    /// The id of the camera its name holds; `None` when it holds none.
    pub camera: Option<String>,
    /// What its name tags it with, as an event saved it (`vehicle`, `person`); empty for a file
    /// taken by hand.
    pub tags: Vec<String>,
}

/// What a name says, as [`FileKind::parse`] reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed<'a> {
    /// When the file was made, in seconds since 1970.
    pub stamp: u64,
    /// The id of the camera that made it.
    pub camera: &'a str,
    /// What it is tagged with, in the name's order; none for a file taken by hand.
    pub tags: Vec<&'a str>,
}

/// Only the characters a GUID uses, so no id or tag can make a name Windows refuses, reach outside
/// the folder, or run into the next part of the name.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

impl FileKind {
    /// The name of a file `camera` makes at `stamp`, in seconds, tagged with `tags`: what the
    /// camera saw, for a file an event saved; none for one taken by hand. Only the characters a
    /// GUID uses are kept from the id and from each tag.
    pub fn name(&self, stamp: u64, camera: &str, tags: &[&str]) -> String {
        let (prefix, extension) = (self.prefix, self.extension);
        let mut name = format!("{prefix}{stamp}");
        let camera = plain(camera);
        if !camera.is_empty() {
            name.push('-');
            name.push_str(&camera);
        }
        for tag in tags.iter().map(|tag| plain(tag)) {
            if !tag.is_empty() {
                name.push('.');
                name.push_str(&tag);
            }
        }
        format!("{name}.{extension}")
    }

    /// What a name says, if it is one of this kind's names: when, which camera and what tags.
    pub fn parse<'a>(&self, name: &'a str) -> Option<Parsed<'a>> {
        let rest = name
            .strip_prefix(self.prefix)?
            .strip_suffix(self.extension)?
            .strip_suffix('.')?;
        let (made, tags) = match rest.split_once('.') {
            Some((made, tags)) => (
                made,
                tags.split('.').filter(|tag| !tag.is_empty()).collect(),
            ),
            None => (rest, Vec::new()),
        };
        let (stamp, camera) = made.split_once('-')?;
        if camera.is_empty() {
            return None;
        }
        Some(Parsed {
            stamp: stamp.parse().ok()?,
            camera,
            tags,
        })
    }

    /// Whether `path` has this kind's extension, in any case.
    pub fn matches(&self, path: &Path) -> bool {
        path.extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(self.extension))
    }

    /// The file called `name` in `folder`. Only a file of this kind directly in the folder, so a
    /// name can never reach another file.
    pub fn locate(&self, folder: &Path, name: &str) -> Result<PathBuf> {
        let plain = Path::new(name).file_name().is_some_and(|file| file == name);
        let path = folder.join(name);
        if !plain || !self.matches(&path) {
            return Err(StorageError::NotThisKind {
                name: name.to_string(),
                noun: self.noun,
            });
        }
        if !path.is_file() {
            return Err(StorageError::Missing(format!(
                "{name} is no longer in the {} folder.",
                self.folder
            )));
        }
        Ok(path)
    }

    /// Deletes the file called `name` in `folder` for good, returning where it was.
    pub fn delete(&self, folder: &Path, name: &str) -> Result<PathBuf> {
        let path = self.locate(folder, name)?;
        fs::remove_file(&path).map_err(StorageError::io(format!("Could not delete {name}")))?;
        Ok(path)
    }

    /// Every file of this kind in `folder`, in no order. A folder that isn't there yet has none.
    pub fn entries(&self, folder: &Path) -> Vec<Entry> {
        let Ok(listing) = fs::read_dir(folder) else {
            return Vec::new();
        };
        let folder = absolute(folder);
        listing
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .filter(|entry| self.matches(&entry.path()))
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let path = folder.join(&name);
                let metadata = fs::metadata(&path).ok()?;
                let (stamp_ms, camera, tags) = match self.parse(&name) {
                    Some(parsed) => (
                        Some(parsed.stamp.saturating_mul(1000)),
                        Some(parsed.camera.to_string()),
                        parsed.tags.into_iter().map(str::to_string).collect(),
                    ),
                    None => (None, None, Vec::new()),
                };
                Some(Entry {
                    name,
                    path,
                    metadata,
                    stamp_ms,
                    camera,
                    tags,
                })
            })
            .collect()
    }
}

/// Sorts `files` newest first by `made_ms`. The name breaks ties, so two made in the same second
/// keep one order.
pub fn sort_newest_first<T>(
    files: &mut [T],
    made_ms: impl Fn(&T) -> u64,
    name: impl Fn(&T) -> &str,
) {
    files.sort_by(|a, b| {
        made_ms(b)
            .cmp(&made_ms(a))
            .then_with(|| name(b).cmp(name(a)))
    });
}

/// A number that changes whenever a file `keep` accepts, in any of `folders`, is added, removed or
/// written to; 0 when there is none. It reads only the folder listings, never the files, so it is
/// cheap enough to check every second.
pub fn revision(folders: &[PathBuf], keep: impl Fn(&Path) -> bool) -> u64 {
    let mut seen: Vec<(String, u64, u128)> = Vec::new();
    for folder in folders {
        let Ok(listing) = fs::read_dir(folder) else {
            continue;
        };
        seen.extend(listing.filter_map(Result::ok).filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() || !keep(&entry.path()) {
                return None;
            }
            let written = metadata
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_nanos();
            Some((
                entry.file_name().into_string().ok()?,
                metadata.len(),
                written,
            ))
        }));
    }
    if seen.is_empty() {
        return 0;
    }
    seen.sort();
    let mut hasher = DefaultHasher::new();
    seen.hash(&mut hasher);
    hasher.finish()
}

/// `time` in milliseconds since 1970, or 0 when the system could not give it.
pub fn unix_ms(time: io::Result<SystemTime>) -> u64 {
    time.ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

/// Now, in milliseconds since 1970.
pub fn now_ms() -> u64 {
    unix_ms(Ok(SystemTime::now()))
}

/// Now, in whole seconds since 1970, which is what snapshot and recording names carry.
pub fn now_seconds() -> u64 {
    now_ms() / 1000
}

/// `folder` made absolute, or as it is when it can't be.
pub fn absolute(folder: &Path) -> PathBuf {
    std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn parsed<'a>(stamp: u64, camera: &'a str, tags: &[&'a str]) -> Option<Parsed<'a>> {
        Some(Parsed {
            stamp,
            camera,
            tags: tags.to_vec(),
        })
    }

    #[test]
    fn a_camera_id_cannot_leave_the_folder_or_break_the_name() {
        assert_eq!(SNAPSHOT.name(7, "..\\..\\evil", &[]), "snapshot-7-evil.png");
        assert_eq!(SNAPSHOT.name(7, "", &[]), "snapshot-7.png");
        assert_eq!(VIDEO.name(7, "/:*?", &[]), "video-7.mp4");
        assert_eq!(
            SNAPSHOT.name(7, "a.b", &["c.d", "", "../e"]),
            "snapshot-7-ab.cd.e.png",
            "neither the id nor a tag holds a dot"
        );
    }

    #[test]
    fn names_read_back_with_their_camera() {
        assert_eq!(
            SNAPSHOT.parse("snapshot-12345-3f2a-9c.png"),
            parsed(12345, "3f2a-9c", &[])
        );
        assert_eq!(VIDEO.parse("video-12345.mp4"), None, "no camera");
        assert_eq!(SNAPSHOT.parse("snapshot-soon.png"), None);
        assert_eq!(VIDEO.parse("holiday.mp4"), None);
        assert_eq!(
            VIDEO.parse("snapshot-1.png"),
            None,
            "each reads only its own"
        );
        assert_eq!(
            SNAPSHOT.parse("snapshot-1png"),
            None,
            "the dot is part of it"
        );
    }

    #[test]
    fn a_name_carries_what_an_event_saw_as_its_tags() {
        let name = SNAPSHOT.name(17, "3f2a-9c", &["person", "vehicle"]);
        assert_eq!(name, "snapshot-17-3f2a-9c.person.vehicle.png");
        assert_eq!(
            SNAPSHOT.parse(&name),
            parsed(17, "3f2a-9c", &["person", "vehicle"])
        );
    }

    #[test]
    fn a_listing_gives_each_files_tags() {
        let folder = TempDir::new().unwrap();
        fs::write(folder.path().join("video-1-a.vehicle.mp4"), "video").unwrap();
        fs::write(folder.path().join("video-2-a.mp4"), "video").unwrap();
        let mut found: Vec<(String, Vec<String>)> = VIDEO
            .entries(folder.path())
            .into_iter()
            .map(|entry| (entry.name, entry.tags))
            .collect();
        found.sort();
        assert_eq!(
            found,
            [
                (
                    "video-1-a.vehicle.mp4".to_string(),
                    vec!["vehicle".to_string()]
                ),
                ("video-2-a.mp4".to_string(), Vec::new())
            ]
        );
    }

    #[test]
    fn only_a_file_of_the_kind_in_the_folder_is_found() {
        let folder = TempDir::new().unwrap();
        fs::write(folder.path().join("video-1-a.mp4"), "video").unwrap();
        fs::write(folder.path().join("notes.txt"), "keep me").unwrap();

        assert!(VIDEO.locate(folder.path(), "video-1-a.mp4").is_ok());
        assert_eq!(
            VIDEO
                .locate(folder.path(), "notes.txt")
                .unwrap_err()
                .to_string(),
            "`notes.txt` is not a recording"
        );
        assert!(VIDEO.locate(folder.path(), "..\\video-1-a.mp4").is_err());
        assert!(VIDEO.locate(folder.path(), "../video-1-a.mp4").is_err());
        assert_eq!(
            SNAPSHOT
                .locate(folder.path(), "snapshot-9.png")
                .unwrap_err()
                .to_string(),
            "snapshot-9.png is no longer in the snapshots folder."
        );
    }

    #[test]
    fn the_newest_comes_first_and_the_name_breaks_a_tie() {
        let mut files = vec![(100, "b"), (300, "a"), (100, "c")];
        sort_newest_first(&mut files, |file| file.0, |file| file.1);
        assert_eq!(files, [(300, "a"), (100, "c"), (100, "b")]);
    }

    #[test]
    fn an_empty_or_missing_folder_has_revision_zero() {
        let folder = TempDir::new().unwrap();
        assert_eq!(revision(&[folder.path().to_path_buf()], |_| true), 0);
        assert_eq!(revision(&[folder.path().join("missing")], |_| true), 0);
    }
}
