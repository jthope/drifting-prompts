//! Thumbnails: a small PNG of each snapshot and recording, so a page can show a folder of them
//! without decoding a full picture or any video.
//!
//! A thumbnail sits in the `thumbnails` folder inside its file's folder, under the file's name
//! with a `.png` extension (`snapshots\thumbnails\snapshot-1762560000-<id>.png`,
//! `videos\thumbnails\video-1762560000-<id>.png`), and is 480 pixels wide at most; the pages show
//! them about half that wide. A PNG can't be decoded any smaller than it is, so a gallery of 1080p
//! snapshots shown as they are decodes 8 MB for each tile.
//!
//! Each is written to a file of its own beside it first and renamed into place, so a page never
//! reads half of one, and two writing the same one at once don't mix.

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use image::RgbaImage;
use ipcamera_media::DecodedFrame;

use crate::error::{Result, StorageError};

/// The thumbnails' folder, inside the folder of the files they show.
pub const FOLDER: &str = "thumbnails";

/// How wide a thumbnail is at most, in pixels.
const WIDTH: u32 = 480;

/// Numbers the files thumbnails are written to before they are renamed into place.
static NEXT_PART: AtomicU64 = AtomicU64::new(0);

/// Where the thumbnail of `file` goes.
pub fn path_of(file: &Path) -> Option<PathBuf> {
    let folder = file.parent()?.join(FOLDER);
    Some(folder.join(Path::new(file.file_name()?).with_extension("png")))
}

/// The thumbnail of `file`, when it has one.
pub fn existing(file: &Path) -> Option<PathBuf> {
    path_of(file).filter(|path| path.is_file())
}

/// The thumbnail called `name` in the thumbnails folder of `folder`. Only a PNG directly in it, so
/// a name can never reach another file.
pub fn locate(folder: &Path, name: &str) -> Result<PathBuf> {
    let plain = Path::new(name).file_name().is_some_and(|file| file == name);
    let path = folder.join(FOLDER).join(name);
    let png = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
    if !plain || !png {
        return Err(StorageError::NotThisKind {
            name: name.to_string(),
            noun: "thumbnail",
        });
    }
    if !path.is_file() {
        return Err(StorageError::Missing(format!(
            "{name} has no thumbnail yet."
        )));
    }
    Ok(path)
}

/// Writes `frame`, made smaller, as the thumbnail of `file`.
pub fn write(file: &Path, frame: &DecodedFrame) -> Result<PathBuf> {
    save(file, &rgba_of(frame)?)
}

/// `frame` as a picture `image` can work on.
pub(crate) fn rgba_of(frame: &DecodedFrame) -> Result<RgbaImage> {
    if frame.width == 0 || frame.height == 0 {
        return Err(bad("the frame is empty"));
    }
    RgbaImage::from_raw(frame.width, frame.height, frame.to_rgba())
        .ok_or_else(|| bad("the frame does not match its size"))
}

/// Writes the thumbnail of `file`, a picture, from the picture itself.
pub fn write_from_picture(file: &Path) -> Result<PathBuf> {
    let picture = image::open(file)
        .map_err(|error| bad(format!("{}: {error}", file.display())))?
        .to_rgba8();
    save(file, &picture)
}

/// A picture that can't be made into a thumbnail, and why.
pub(crate) fn bad(why: impl Into<String>) -> StorageError {
    StorageError::BadPicture(why.into())
}

/// Writes `picture`, made smaller, as the thumbnail of `file`.
pub(crate) fn save(file: &Path, picture: &RgbaImage) -> Result<PathBuf> {
    let path = path_of(file).ok_or_else(|| bad("the file has no folder"))?;
    save_at(path, picture)
}

/// Writes `picture`, made as small as a thumbnail, at `path`.
pub(crate) fn save_at(path: PathBuf, picture: &RgbaImage) -> Result<PathBuf> {
    if picture.width() == 0 || picture.height() == 0 {
        return Err(bad("the picture is empty"));
    }
    let width = WIDTH.min(picture.width());
    let height =
        (u64::from(picture.height()) * u64::from(width) / u64::from(picture.width())).max(1) as u32;
    let small = image::imageops::thumbnail(picture, width, height);

    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(StorageError::io(folder.display().to_string()))?;
    }
    let part = path.with_extension(format!(
        "png.{}.part",
        NEXT_PART.fetch_add(1, Ordering::Relaxed)
    ));
    let written = small
        .save_with_format(&part, image::ImageFormat::Png)
        .map_err(|error| bad(format!("{}: {error}", path.display())))
        .and_then(|()| {
            std::fs::rename(&part, &path).map_err(StorageError::io(path.display().to_string()))
        });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&part);
        return Err(error);
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
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
    fn a_thumbnail_is_small_and_sits_in_its_own_folder() {
        let folder = TempDir::new().unwrap();
        let video = folder.path().join("video-5-cam.mp4");

        let path = write(&video, &frame(1920, 1080)).expect("it should write");

        assert_eq!(
            path,
            folder.path().join("thumbnails").join("video-5-cam.png")
        );
        assert_eq!(image::image_dimensions(&path).unwrap(), (480, 270));
        assert_eq!(existing(&video), Some(path.clone()));
        assert_eq!(locate(folder.path(), "video-5-cam.png").unwrap(), path);
        assert!(locate(folder.path(), "..\\video-5-cam.png").is_err());
        assert!(locate(folder.path(), "video-6-cam.png").is_err());
        assert!(locate(folder.path(), "video-5-cam.mp4").is_err());
        let left: Vec<_> = std::fs::read_dir(folder.path().join("thumbnails"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(left, ["video-5-cam.png"], "nothing half written is left");
    }

    #[test]
    fn a_picture_smaller_than_a_thumbnail_keeps_its_size() {
        let folder = TempDir::new().unwrap();
        let snapshot = folder.path().join("snapshot-5-cam.png");

        let path = write(&snapshot, &frame(4, 2)).unwrap();

        assert_eq!(
            path,
            folder.path().join("thumbnails").join("snapshot-5-cam.png")
        );
        assert_eq!(image::image_dimensions(&path).unwrap(), (4, 2));
    }

    #[test]
    fn a_thumbnail_is_made_from_a_picture_file() {
        let folder = TempDir::new().unwrap();
        let picture = folder.path().join("Holiday.PNG");
        image::RgbaImage::from_pixel(960, 540, image::Rgba([1, 2, 3, 255]))
            .save_with_format(&picture, image::ImageFormat::Png)
            .unwrap();

        let path = write_from_picture(&picture).unwrap();

        assert_eq!(path, folder.path().join("thumbnails").join("Holiday.png"));
        assert_eq!(image::image_dimensions(&path).unwrap(), (480, 270));
        assert!(
            write_from_picture(&folder.path().join("missing.png")).is_err(),
            "no picture, no thumbnail"
        );
    }

    #[test]
    fn an_empty_frame_has_no_thumbnail() {
        let folder = TempDir::new().unwrap();
        assert_eq!(
            write(&folder.path().join("video-1-a.mp4"), &frame(0, 0))
                .unwrap_err()
                .to_string(),
            "the frame is empty"
        );
    }
}
