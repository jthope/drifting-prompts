//! Snapshots: a frame, or a camera's own JPEG, written as a PNG with its thumbnail, and listed.

use std::{
    fs::{File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    thread,
};

use image::RgbaImage;
use ipcamera_media::DecodedFrame;

use crate::{
    error::{Result, StorageError},
    saved_file::{self, SNAPSHOT},
    thumbnail,
};

static FILLING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// A snapshot in the folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    /// The file's name, which names it to every other call.
    pub name: String,
    /// Where it is, absolute.
    pub path: PathBuf,
    /// Its thumbnail, when it has one yet.
    pub thumbnail: Option<PathBuf>,
    /// The id of the camera that took it; `None` when the name doesn't say.
    pub camera: Option<String>,
    /// What the camera saw, for one an event took; empty for one taken by hand.
    pub tags: Vec<String>,
    /// When it was taken, in ms since 1970: from its name, else when the file was written.
    pub taken_ms: u64,
    /// Its size on disk.
    pub bytes: u64,
    /// Its width and height in pixels; `None` when its header can't be read.
    pub size: Option<(u32, u32)>,
}

/// Writes one frame from `camera` into `folder` as a PNG, with its thumbnail.
pub fn write_snapshot(
    folder: &Path,
    frame: &DecodedFrame,
    stamp: u64,
    camera: &str,
) -> Result<PathBuf> {
    write_tagged(folder, frame, stamp, camera, &[])
}

/// Writes one frame as [`write_snapshot`] does, its name tagged with `tags`.
pub fn write_tagged(
    folder: &Path,
    frame: &DecodedFrame,
    stamp: u64,
    camera: &str,
    tags: &[&str],
) -> Result<PathBuf> {
    if frame.width == 0 || frame.height == 0 {
        return Err(StorageError::BadPicture(
            "the last frame is empty".to_string(),
        ));
    }
    write_picture(folder, &thumbnail::rgba_of(frame)?, stamp, camera, tags)
}

/// Writes the camera's own JPEG as a snapshot at its full size, its name tagged with `tags`.
pub fn write_from_jpeg(
    folder: &Path,
    jpeg: &[u8],
    stamp: u64,
    camera: &str,
    tags: &[&str],
) -> Result<PathBuf> {
    let picture = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
        .map_err(|error| StorageError::BadPicture(format!("the camera's picture: {error}")))?
        .to_rgba8();
    write_picture(folder, &picture, stamp, camera, tags)
}

fn write_picture(
    folder: &Path,
    picture: &RgbaImage,
    stamp: u64,
    camera: &str,
    tags: &[&str],
) -> Result<PathBuf> {
    std::fs::create_dir_all(folder).map_err(StorageError::io(folder.display().to_string()))?;
    let (path, file) = create(folder, stamp, camera, tags)?;

    if let Err(error) = write_png(file, picture.as_raw(), picture.width(), picture.height()) {
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    // A missing thumbnail is made from the picture later.
    let _ = thumbnail::save(&path, picture);
    Ok(path)
}

/// A taken second moves on to the next, not to a `-2` suffix, which would read back as the camera;
/// creating the file claims the name.
fn create(folder: &Path, stamp: u64, camera: &str, tags: &[&str]) -> Result<(PathBuf, File)> {
    let mut stamp = stamp;
    loop {
        let path = folder.join(SNAPSHOT.name(stamp, camera, tags));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => stamp += 1,
            Err(error) => return Err(StorageError::io(path.display().to_string())(error)),
        }
    }
}

fn write_png(file: File, rgba: &[u8], width: u32, height: u32) -> Result<()> {
    let mut out = BufWriter::new(file);
    image::write_buffer_with_format(
        &mut out,
        rgba,
        width,
        height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|error| StorageError::BadPicture(error.to_string()))?;
    out.flush()
        .map_err(StorageError::io("the snapshot could not be written"))
}

/// Every PNG in `folder`, newest first.
pub fn list(folder: &Path) -> Vec<Saved> {
    let mut found: Vec<Saved> = SNAPSHOT
        .entries(folder)
        .into_iter()
        .map(|entry| Saved {
            size: image::image_dimensions(&entry.path).ok(),
            thumbnail: thumbnail::existing(&entry.path),
            bytes: entry.metadata.len(),
            taken_ms: entry
                .stamp_ms
                .unwrap_or_else(|| saved_file::unix_ms(entry.metadata.modified())),
            name: entry.name,
            path: entry.path,
            camera: entry.camera,
            tags: entry.tags,
        })
        .collect();
    saved_file::sort_newest_first(
        &mut found,
        |saved| saved.taken_ms,
        |saved| saved.name.as_str(),
    );
    found
}

/// Changes whenever a PNG in `folder`, or its thumbnail, is added, removed or written.
pub fn revision(folder: &Path) -> u64 {
    saved_file::revision(
        &[folder.to_path_buf(), folder.join(thumbnail::FOLDER)],
        |path| SNAPSHOT.matches(path),
    )
}

/// Deletes the snapshot called `name` in `folder` for good, and its thumbnail.
pub fn delete(folder: &Path, name: &str) -> Result<()> {
    let path = SNAPSHOT.delete(folder, name)?;
    if let Some(thumbnail) = thumbnail::path_of(&path) {
        let _ = std::fs::remove_file(thumbnail);
    }
    Ok(())
}

/// Makes the thumbnail of each snapshot in `folder` that has none; returns how many it made.
pub fn fill_thumbnails(folder: &Path) -> usize {
    let mut made = 0;
    for entry in SNAPSHOT.entries(folder) {
        if thumbnail::existing(&entry.path).is_none()
            && thumbnail::write_from_picture(&entry.path).is_ok()
        {
            made += 1;
        }
    }
    made
}

/// Runs [`fill_thumbnails`] on a thread of its own, unless one already is for `folder`.
pub fn fill_thumbnails_soon(folder: PathBuf) {
    let done = |folder: &PathBuf| {
        FILLING
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|each| each != folder);
    };
    {
        let mut filling = FILLING.lock().unwrap_or_else(PoisonError::into_inner);
        if filling.contains(&folder) {
            return;
        }
        filling.push(folder.clone());
    }
    let working = folder.clone();
    let started = thread::Builder::new()
        .name("ipcamera-thumbnails".into())
        .spawn(move || {
            fill_thumbnails(&working);
            done(&working);
        });
    if started.is_err() {
        done(&folder);
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn frame() -> DecodedFrame {
        DecodedFrame {
            width: 3,
            height: 2,
            bgra: [0u8, 1, 2, 255].repeat(6),
        }
    }

    #[test]
    fn a_snapshot_is_written_into_its_folder_as_a_png_named_for_its_camera() {
        let temp = TempDir::new().unwrap();
        let folder = temp.path().join("snapshots");
        let path = write_snapshot(&folder, &frame(), 12345, "3f2a-9c").expect("it should write");

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("snapshot-12345-3f2a-9c.png")
        );
        assert_eq!(path.parent(), Some(folder.as_path()));

        let bytes = std::fs::read(&path).expect("the snapshot should be readable");
        assert_eq!(
            &bytes[..8],
            b"\x89PNG\r\n\x1a\n",
            "the file must be a real PNG, not just a correctly named file"
        );
    }

    #[test]
    fn an_events_snapshot_is_the_cameras_own_picture_at_full_size_and_tagged() {
        let folder = TempDir::new().unwrap();
        let mut jpeg = Vec::new();
        image::RgbImage::from_pixel(1280, 720, image::Rgb([200, 100, 50]))
            .write_to(&mut io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .unwrap();

        let path = write_from_jpeg(folder.path(), &jpeg, 17, "cam", &["vehicle"])
            .expect("it should write");
        assert_eq!(path, folder.path().join("snapshot-17-cam.vehicle.png"));
        assert_eq!(image::image_dimensions(&path).unwrap(), (1280, 720));
        let small = thumbnail::existing(&path).expect("with its thumbnail");
        assert_eq!(image::image_dimensions(&small).unwrap(), (480, 270));

        let from_screen = write_tagged(folder.path(), &frame(), 18, "cam", &["motion"]).unwrap();
        assert_eq!(
            from_screen,
            folder.path().join("snapshot-18-cam.motion.png")
        );

        let found = list(folder.path());
        assert_eq!(found[0].tags, ["motion"]);
        assert_eq!(found[1].tags, ["vehicle"]);
        assert!(write_from_jpeg(folder.path(), b"not a jpeg", 19, "cam", &[]).is_err());
    }

    #[test]
    fn a_second_snapshot_in_the_same_second_takes_the_next_one() {
        let folder = TempDir::new().unwrap();

        let first = write_snapshot(folder.path(), &frame(), 5, "a").unwrap();
        let second = write_snapshot(folder.path(), &frame(), 5, "a").unwrap();

        assert_eq!(first, folder.path().join("snapshot-5-a.png"));
        assert_eq!(second, folder.path().join("snapshot-6-a.png"));
        let found = list(folder.path());
        assert_eq!(found.len(), 2, "neither was written over");
        assert!(
            found
                .iter()
                .all(|saved| saved.camera.as_deref() == Some("a"))
        );
        assert_eq!(
            write_snapshot(folder.path(), &frame(), 5, "b").unwrap(),
            folder.path().join("snapshot-5-b.png"),
            "another camera's snapshot is no clash"
        );
    }

    #[test]
    fn snapshots_taken_at_once_never_share_a_file() {
        const AT_ONCE: usize = 4;
        let folder = TempDir::new().unwrap();

        let written: Vec<PathBuf> = std::thread::scope(|scope| {
            let writers: Vec<_> = (0..AT_ONCE)
                .map(|_| scope.spawn(|| write_snapshot(folder.path(), &frame(), 9, "a").unwrap()))
                .collect();
            writers
                .into_iter()
                .map(|writer| writer.join().unwrap())
                .collect()
        });

        let mut names: Vec<_> = written.iter().filter_map(|path| path.file_name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), AT_ONCE, "each got a name of its own");
        for path in &written {
            assert_eq!(
                image::image_dimensions(path).unwrap(),
                (3, 2),
                "{} is a whole PNG",
                path.display()
            );
        }
    }

    #[test]
    fn an_empty_frame_is_refused() {
        let frame = DecodedFrame {
            width: 0,
            height: 0,
            bgra: Vec::new(),
        };

        let error = write_snapshot(&std::env::temp_dir(), &frame, 1, "a").unwrap_err();

        assert_eq!(error.to_string(), "the last frame is empty");
    }

    #[test]
    fn the_list_is_newest_first_with_each_camera_and_size() {
        let folder = TempDir::new().unwrap();
        write_snapshot(folder.path(), &frame(), 100, "older").unwrap();
        write_snapshot(folder.path(), &frame(), 300, "newer").unwrap();
        std::fs::write(folder.path().join("notes.txt"), "not a picture").unwrap();

        let found = list(folder.path());

        let names: Vec<_> = found.iter().map(|saved| saved.name.as_str()).collect();
        assert_eq!(names, ["snapshot-300-newer.png", "snapshot-100-older.png"]);
        assert_eq!(found[0].camera.as_deref(), Some("newer"));
        assert_eq!(found[1].camera.as_deref(), Some("older"));
        assert_eq!(found[0].taken_ms, 300_000);
        assert_eq!(found[0].size, Some((3, 2)));
        assert!(found[0].bytes > 0);
        assert!(found[0].path.is_absolute());
    }

    #[test]
    fn a_png_put_there_by_hand_lists_with_no_camera() {
        let folder = TempDir::new().unwrap();
        let written = write_snapshot(folder.path(), &frame(), 5, "cam").unwrap();
        std::fs::rename(&written, folder.path().join("Holiday.PNG")).unwrap();

        let found = list(folder.path());

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].camera, None);
        assert!(found[0].taken_ms > 0, "it is dated by when it was written");
    }

    #[test]
    fn a_missing_folder_has_no_snapshots() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("missing");
        assert!(list(&missing).is_empty());
        assert_eq!(revision(&missing), 0);
    }

    #[test]
    fn the_revision_follows_adding_and_deleting() {
        let folder = TempDir::new().unwrap();
        write_snapshot(folder.path(), &frame(), 1, "a").unwrap();
        let one = revision(folder.path());
        assert_eq!(revision(folder.path()), one, "nothing changed");

        write_snapshot(folder.path(), &frame(), 2, "a").unwrap();
        let two = revision(folder.path());
        assert_ne!(two, one);

        delete(folder.path(), "snapshot-2-a.png").unwrap();
        assert_ne!(revision(folder.path()), two);
        assert_eq!(list(folder.path()).len(), 1);
    }

    #[test]
    fn only_a_png_in_the_folder_can_be_deleted() {
        let folder = TempDir::new().unwrap();
        write_snapshot(folder.path(), &frame(), 1, "a").unwrap();
        std::fs::write(folder.path().join("notes.txt"), "keep me").unwrap();

        assert!(delete(folder.path(), "notes.txt").is_err());
        assert!(delete(folder.path(), "..\\snapshot-1-a.png").is_err());
        assert!(
            delete(folder.path(), "snapshot-9-a.png").is_err(),
            "not there"
        );
        assert!(folder.path().join("notes.txt").is_file());

        delete(folder.path(), "snapshot-1-a.png").unwrap();
        assert!(list(folder.path()).is_empty());
    }

    #[test]
    fn a_snapshot_comes_with_its_thumbnail_and_goes_with_it() {
        let folder = TempDir::new().unwrap();
        let path = write_snapshot(folder.path(), &frame(), 1, "a").unwrap();
        let thumbnail = folder.path().join("thumbnails").join("snapshot-1-a.png");

        assert!(thumbnail.is_file());
        assert_eq!(list(folder.path())[0].thumbnail, Some(thumbnail.clone()));
        assert_eq!(list(folder.path()).len(), 1, "a thumbnail is no snapshot");

        delete(folder.path(), "snapshot-1-a.png").unwrap();
        assert!(!path.exists());
        assert!(!thumbnail.exists());
    }

    #[test]
    fn the_thumbnails_a_snapshot_lacks_are_made_from_the_picture() {
        let folder = TempDir::new().unwrap();
        let written = write_snapshot(folder.path(), &frame(), 1, "a").unwrap();
        let thumbnails = folder.path().join("thumbnails");
        std::fs::remove_file(thumbnails.join("snapshot-1-a.png")).unwrap();
        std::fs::copy(&written, folder.path().join("Holiday.PNG")).unwrap();
        std::fs::write(folder.path().join("snapshot-2-a.png"), "not a picture").unwrap();
        let before = revision(folder.path());

        assert_eq!(fill_thumbnails(folder.path()), 2);

        assert_eq!(
            image::image_dimensions(thumbnails.join("snapshot-1-a.png")).unwrap(),
            (3, 2)
        );
        assert!(thumbnails.join("Holiday.png").is_file());
        assert!(
            !thumbnails.join("snapshot-2-a.png").exists(),
            "a file that isn't a picture gets none"
        );
        assert_ne!(revision(folder.path()), before, "the gallery hears of them");
        assert_eq!(fill_thumbnails(folder.path()), 0, "nothing left to make");
    }
}
