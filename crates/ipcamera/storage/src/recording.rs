//! Recordings: naming each file as it starts, its thumbnail, listing them, and retagging an
//! event's once it closes.

use std::{
    fs::{self, OpenOptions},
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use ipcamera_media::FrameSlot;

use crate::{
    error::{Result, StorageError},
    saved_file::{self, VIDEO},
    thumbnail,
};

const PICTURE_WITHIN: Duration = Duration::from_secs(5);

/// A recording in the folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    /// The file's name, which names it to every other call.
    pub name: String,
    /// Where it is, absolute.
    pub path: PathBuf,
    /// Its thumbnail, when it has one.
    pub thumbnail: Option<PathBuf>,
    /// The id of the camera that made it; `None` when the name doesn't say.
    pub camera: Option<String>,
    /// What the camera saw, for one an event started; empty for one the user started.
    pub tags: Vec<String>,
    /// When it started, in ms since 1970: from its name, else when the file was made.
    pub started_ms: u64,
    /// When the file was last written, in ms since 1970: where a recording ends.
    pub written_ms: u64,
    /// Its size on disk.
    pub bytes: u64,
}

impl Saved {
    /// How long it runs, in milliseconds.
    pub fn length_ms(&self) -> u64 {
        self.written_ms.saturating_sub(self.started_ms)
    }
}

/// A new, empty file for a recording, made in one step so the app and the server recording into
/// one folder never take the same name; a second already taken moves on to the next.
pub fn file_for(folder: &Path, stamp: u64, camera: &str, tags: &[&str]) -> PathBuf {
    // The folder must exist for the name to be taken in it.
    let _ = fs::create_dir_all(folder);
    let mut stamp = stamp;
    loop {
        let path = folder.join(VIDEO.name(stamp, camera, tags));
        let taken = OpenOptions::new().write(true).create_new(true).open(&path);
        match taken {
            Err(error) if error.kind() == ErrorKind::AlreadyExists => stamp += 1,
            _ => return path,
        }
    }
}

/// Writes the recording's thumbnail from the next picture in `frames`, on a thread of its own.
pub fn write_thumbnail_soon(video: PathBuf, frames: FrameSlot) {
    let produced = frames.produced();
    let _ = std::thread::Builder::new()
        .name("ipcamera-thumbnail".into())
        .spawn(move || {
            // The picture the recording starts on is still being decoded.
            let deadline = Instant::now() + PICTURE_WITHIN;
            while frames.produced() == produced && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if let Some(frame) = frames.latest() {
                let _ = thumbnail::write(&video, &frame);
            }
        });
}

/// Every recording in `folder`, newest first.
pub fn list(folder: &Path) -> Vec<Saved> {
    let mut found: Vec<Saved> = VIDEO
        .entries(folder)
        .into_iter()
        .map(|entry| {
            let written_ms = saved_file::unix_ms(entry.metadata.modified());
            Saved {
                thumbnail: thumbnail::existing(&entry.path),
                bytes: entry.metadata.len(),
                started_ms: entry.stamp_ms.unwrap_or_else(|| {
                    saved_file::unix_ms(entry.metadata.created()).min(written_ms)
                }),
                written_ms,
                name: entry.name,
                path: entry.path,
                camera: entry.camera,
                tags: entry.tags,
            }
        })
        .collect();
    saved_file::sort_newest_first(
        &mut found,
        |saved| saved.started_ms,
        |saved| saved.name.as_str(),
    );
    found
}

/// A number that changes whenever a recording or a thumbnail is added, removed or written to.
pub fn revision(folder: &Path) -> u64 {
    saved_file::revision(
        &[folder.to_path_buf(), folder.join(thumbnail::FOLDER)],
        |_| true,
    )
}

/// Renames the recording to carry `tags` in place of its own, its thumbnail with it.
pub fn retag(video: &Path, tags: &[&str]) -> Result<PathBuf> {
    let name = video
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parsed = VIDEO
        .parse(&name)
        .ok_or_else(|| StorageError::NotThisKind {
            name: name.clone(),
            noun: "recording",
        })?;
    let renamed = VIDEO.name(parsed.stamp, parsed.camera, tags);
    if renamed == name {
        return Ok(video.to_path_buf());
    }
    let target = video.with_file_name(&renamed);
    if target.exists() {
        return Err(StorageError::Missing(format!(
            "{renamed} is already in the videos folder."
        )));
    }
    fs::rename(video, &target).map_err(StorageError::io(format!("Could not rename {name}")))?;
    if let (Some(from), Some(to)) = (thumbnail::existing(video), thumbnail::path_of(&target)) {
        let _ = fs::rename(from, to);
    }
    Ok(target)
}

/// Deletes the recording for good, and its thumbnail.
pub fn delete(folder: &Path, name: &str) -> Result<()> {
    let path = VIDEO.delete(folder, name)?;
    if let Some(thumbnail) = thumbnail::path_of(&path) {
        let _ = std::fs::remove_file(thumbnail);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use ipcamera_media::DecodedFrame;
    use tempfile::TempDir;

    use super::*;

    fn frame(width: u32, height: u32) -> DecodedFrame {
        DecodedFrame {
            width,
            height,
            bgra: [10u8, 20, 30, 255].repeat((width * height) as usize),
        }
    }

    #[test]
    fn a_recording_is_named_for_its_camera_and_never_written_over() {
        let folder = TempDir::new().unwrap();
        let first = file_for(folder.path(), 100, "3f2a-9c", &[]);
        assert_eq!(first, folder.path().join("video-100-3f2a-9c.mp4"));

        assert_eq!(
            std::fs::metadata(&first).unwrap().len(),
            0,
            "the name is taken with an empty file, for the recorder to open"
        );
        assert_eq!(
            file_for(folder.path(), 100, "3f2a-9c", &[]),
            folder.path().join("video-101-3f2a-9c.mp4"),
            "the same second again takes the next one"
        );
        assert_eq!(
            file_for(&folder.path().join("not").join("there yet"), 5, "a", &[]),
            folder
                .path()
                .join("not")
                .join("there yet")
                .join("video-5-a.mp4"),
            "the folder is made for it"
        );
        assert_eq!(
            file_for(folder.path(), 7, "..\\..\\evil", &[]),
            folder.path().join("video-7-evil.mp4")
        );
        assert_eq!(
            file_for(folder.path(), 7, "", &[]),
            folder.path().join("video-7.mp4")
        );
        assert_eq!(
            file_for(folder.path(), 8, "a", &["person", "vehicle"]),
            folder.path().join("video-8-a.person.vehicle.mp4"),
            "an event's recording is tagged with what the camera saw"
        );
        let listed = list(folder.path());
        let tagged = listed
            .iter()
            .find(|saved| saved.name == "video-8-a.person.vehicle.mp4")
            .unwrap();
        assert_eq!(tagged.tags, ["person", "vehicle"]);
        assert_eq!(tagged.camera.as_deref(), Some("a"));
    }

    #[test]
    fn the_list_is_newest_first_with_each_camera_length_and_thumbnail() {
        let folder = TempDir::new().unwrap();
        let older = folder.path().join("video-100-older.mp4");
        let newer = folder.path().join("video-300-newer.mp4");
        std::fs::write(&older, "older").unwrap();
        std::fs::write(&newer, "newer!").unwrap();
        std::fs::write(folder.path().join("notes.txt"), "not a video").unwrap();
        thumbnail::write(&newer, &frame(4, 2)).unwrap();

        let found = list(folder.path());

        let names: Vec<_> = found.iter().map(|saved| saved.name.as_str()).collect();
        assert_eq!(names, ["video-300-newer.mp4", "video-100-older.mp4"]);
        assert_eq!(found[0].camera.as_deref(), Some("newer"));
        assert_eq!(found[0].started_ms, 300_000);
        assert_eq!(found[0].bytes, 6);
        assert!(found[0].path.is_absolute());
        assert!(
            found[0]
                .thumbnail
                .as_ref()
                .is_some_and(|path| path.is_file())
        );
        assert_eq!(found[1].thumbnail, None, "a recording can lack one");
        assert!(found[0].written_ms > found[0].started_ms);
    }

    #[test]
    fn the_revision_follows_writing_and_deleting() {
        let folder = TempDir::new().unwrap();
        assert_eq!(revision(folder.path()), 0);
        let video = folder.path().join("video-1-a.mp4");
        std::fs::write(&video, "one").unwrap();
        let one = revision(folder.path());
        assert_eq!(revision(folder.path()), one, "nothing changed");

        std::fs::write(&video, "one and more").unwrap();
        let grown = revision(folder.path());
        assert_ne!(grown, one);

        thumbnail::write(&video, &frame(2, 2)).unwrap();
        let with_thumbnail = revision(folder.path());
        assert_ne!(with_thumbnail, grown, "a thumbnail arriving shows too");

        delete(folder.path(), "video-1-a.mp4").unwrap();
        assert_eq!(revision(folder.path()), 0, "the thumbnail went with it");
        assert!(list(folder.path()).is_empty());
    }

    #[test]
    fn only_a_recording_in_the_folder_can_be_deleted() {
        let folder = TempDir::new().unwrap();
        std::fs::write(folder.path().join("video-1-a.mp4"), "video").unwrap();
        std::fs::write(folder.path().join("notes.txt"), "keep me").unwrap();

        assert!(delete(folder.path(), "notes.txt").is_err());
        assert!(delete(folder.path(), "..\\video-1-a.mp4").is_err());
        assert!(delete(folder.path(), "video-9-a.mp4").is_err(), "not there");
        assert!(folder.path().join("notes.txt").is_file());
        assert!(folder.path().join("video-1-a.mp4").is_file());
    }

    #[test]
    fn a_closed_video_takes_the_tags_of_all_its_camera_saw_with_its_thumbnail() {
        let folder = TempDir::new().unwrap();
        let video = file_for(folder.path(), 100, "a", &["animal"]);
        std::fs::write(&video, "video").unwrap();
        thumbnail::write(&video, &frame(2, 2)).unwrap();

        let renamed = retag(&video, &["vehicle", "animal"]).unwrap();
        assert_eq!(
            renamed,
            folder.path().join("video-100-a.vehicle.animal.mp4")
        );
        assert!(!video.exists());
        assert_eq!(std::fs::read(&renamed).unwrap(), b"video");
        assert!(
            thumbnail::existing(&renamed).is_some(),
            "the thumbnail went with it"
        );
        assert!(thumbnail::existing(&video).is_none());

        assert_eq!(retag(&renamed, &["vehicle", "animal"]).unwrap(), renamed);
        let other = folder.path().join("notes.txt");
        std::fs::write(&other, "keep").unwrap();
        assert!(retag(&other, &["person"]).is_err());
        std::fs::write(folder.path().join("video-100-a.person.mp4"), "taken").unwrap();
        assert!(retag(&renamed, &["person"]).is_err());
        assert!(renamed.exists());
    }
}
