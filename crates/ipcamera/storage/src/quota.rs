//! What the snapshots and videos hold together, and deleting the oldest to keep them under a limit.
//!
//! A listing's size is stale until a file closes, so a file a camera is writing is sized from the
//! file itself, and never deleted.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    recording,
    saved_file::{self, FileKind, SNAPSHOT, VIDEO},
    snapshot,
};

/// A snapshot or a recording, for adding up and making room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaFile {
    /// Where it is.
    pub path: PathBuf,
    /// Its name in its folder.
    pub name: String,
    /// Whether it is a recording, rather than a snapshot.
    pub video: bool,
    /// Its size, in bytes.
    pub bytes: u64,
    /// When it was made, in ms since 1970: as its name says, else when it was written.
    pub made_ms: u64,
}

/// Every snapshot and recording, oldest first; the `busy` files cameras are writing.
pub fn media_files(snapshots: &Path, videos: &Path, busy: &[PathBuf]) -> Vec<MediaFile> {
    let mut files = listed(&SNAPSHOT, snapshots, false, busy);
    files.extend(listed(&VIDEO, videos, true, busy));
    files.sort_by(|a, b| a.made_ms.cmp(&b.made_ms).then_with(|| a.name.cmp(&b.name)));
    files
}

fn listed(kind: &FileKind, folder: &Path, video: bool, busy: &[PathBuf]) -> Vec<MediaFile> {
    let Ok(listing) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let folder = saved_file::absolute(folder);
    listing
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let path = folder.join(&name);
            if !kind.matches(&path) {
                return None;
            }
            let metadata = if busy.contains(&path) {
                fs::metadata(&path).ok()?
            } else {
                entry.metadata().ok()?
            };
            if !metadata.is_file() {
                return None;
            }
            let made_ms = kind
                .parse(&name)
                .map(|parsed| parsed.stamp.saturating_mul(1000))
                .unwrap_or_else(|| saved_file::unix_ms(metadata.modified()));
            Some(MediaFile {
                path,
                name,
                video,
                bytes: metadata.len(),
                made_ms,
            })
        })
        .collect()
}

/// What `files` add up to, in bytes.
pub fn total(files: &[MediaFile]) -> u64 {
    files.iter().map(|file| file.bytes).sum()
}

/// Deletes the oldest, never one in `busy`, until the rest are under `limit`; returns what is left.
pub fn make_room(files: &[MediaFile], limit: u64, busy: &[PathBuf]) -> u64 {
    let mut left = total(files);
    for file in files {
        if left < limit {
            break;
        }
        if busy.contains(&file.path) {
            continue;
        }
        let Some(folder) = file.path.parent() else {
            continue;
        };
        let deleted = if file.video {
            recording::delete(folder, &file.name)
        } else {
            snapshot::delete(folder, &file.name)
        };
        if deleted.is_ok() {
            left = left.saturating_sub(file.bytes);
        }
    }
    left
}

/// `bytes` as a message says it (`1.5 GB`), 1,024 to the next unit.
pub fn size_label(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        return format!("{bytes} bytes");
    }
    let rounded = (size * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0} {}", UNITS[unit])
    } else {
        format!("{rounded:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::thumbnail;

    fn write(folder: &Path, name: &str, bytes: usize) -> PathBuf {
        fs::create_dir_all(folder).unwrap();
        let path = folder.join(name);
        fs::write(&path, vec![0u8; bytes]).unwrap();
        path
    }

    #[test]
    fn the_media_add_up_oldest_first_whatever_their_kind() {
        let root = TempDir::new().unwrap();
        let (snapshots, videos) = (root.path().join("snapshots"), root.path().join("videos"));
        write(&snapshots, "snapshot-300-cam.png", 30);
        write(&videos, "video-100-cam.mp4", 100);
        write(&videos, "video-200-cam.mp4", 200);
        write(&videos, "notes.txt", 1_000);

        let files = media_files(&snapshots, &videos, &[]);
        let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "video-100-cam.mp4",
                "video-200-cam.mp4",
                "snapshot-300-cam.png"
            ]
        );
        assert_eq!(total(&files), 330);
        assert!(media_files(&root.path().join("none"), &videos, &[]).len() == 2);
    }

    #[test]
    fn making_room_deletes_the_oldest_but_never_what_a_camera_writes() {
        let root = TempDir::new().unwrap();
        let (snapshots, videos) = (root.path().join("snapshots"), root.path().join("videos"));
        let writing = write(&videos, "video-100-cam.mp4", 100);
        let oldest_snapshot = write(&snapshots, "snapshot-150-cam.png", 50);
        let thumbnail = write(
            &snapshots.join(thumbnail::FOLDER),
            "snapshot-150-cam.png",
            5,
        );
        let older_video = write(&videos, "video-200-cam.mp4", 200);
        let newest = write(&snapshots, "snapshot-300-cam.png", 30);
        let busy = vec![saved_file::absolute(&writing)];

        let files = media_files(&snapshots, &videos, &busy);
        assert_eq!(total(&files), 380);
        let left = make_room(&files, 150, &busy);
        assert_eq!(left, 130);
        assert!(writing.exists());
        assert!(!oldest_snapshot.exists());
        assert!(!thumbnail.exists(), "with its thumbnail");
        assert!(!older_video.exists());
        assert!(newest.exists());

        let files = media_files(&snapshots, &videos, &busy);
        assert_eq!(make_room(&files, 50, &busy), 100);
        assert!(writing.exists());
    }

    #[test]
    fn a_size_reads_as_windows_shows_it() {
        assert_eq!(size_label(512), "512 bytes");
        assert_eq!(size_label(250 * 1024 * 1024), "250 MB");
        assert_eq!(size_label(5 * 1024 * 1024 * 1024), "5 GB");
        assert_eq!(size_label(1536 * 1024 * 1024), "1.5 GB");
    }
}
