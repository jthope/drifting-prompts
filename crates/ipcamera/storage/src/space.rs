//! How full the drive under a folder is, and whether a folder takes files at all.
//!
//! A folder is made only by the first file saved in it, so until then the nearest folder above it
//! that exists stands in: it is on the same drive. Asking a drive can wait on it (a network drive, a
//! disk spinning up), so callers ask off any thread that must stay quick.

use std::{fs, path::Path};

use crate::error::{Result, StorageError};

/// How full the drive under a folder is.
#[derive(Clone, Debug, PartialEq)]
pub struct Space {
    /// The drive, as Explorer names it (`D:`), or the folder's root where there are no drive
    /// letters (`/`).
    pub drive: String,
    /// What is left to fill, in bytes.
    pub free: u64,
    /// How much the drive holds, in bytes.
    pub total: u64,
}

/// How full the drive under `folder` is, asked of the nearest folder at or above it that exists.
pub fn space_under(folder: &Path) -> Result<Space> {
    let there = folder
        .ancestors()
        .find(|each| each.is_dir())
        .ok_or_else(|| StorageError::NoDrive(folder.to_path_buf()))?;
    let stats = fs4::statvfs(there).map_err(StorageError::io(format!(
        "The free space on the drive under {} could not be read",
        folder.display()
    )))?;
    Ok(Space {
        drive: drive_of(folder),
        free: stats.available_space(),
        total: stats.total_space(),
    })
}

/// The drive `folder` is on as Explorer names it, `D:`, from the start of its path; its root
/// (`\\server\share\`, `/`) where it has no drive letter.
pub fn drive_of(folder: &Path) -> String {
    let root = folder
        .ancestors()
        .last()
        .unwrap_or(folder)
        .display()
        .to_string();
    match root.strip_suffix('\\') {
        Some(letter) if letter.len() == 2 && letter.ends_with(':') => letter.to_string(),
        _ => root,
    }
}

/// Whether files can be saved in `folder`, which is made if it isn't there: a folder that can't
/// take a file is refused when chosen, not when the next snapshot fails.
pub fn check_writable(folder: &Path) -> Result<()> {
    let refused = |error: std::io::Error| {
        StorageError::io(format!("The app can't save in {}", folder.display()))(error)
    };
    fs::create_dir_all(folder).map_err(refused)?;
    let probe = folder.join(".ipcamera-write-check");
    fs::write(&probe, b"").map_err(refused)?;
    fs::remove_file(&probe).map_err(refused)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn a_folder_the_app_can_write_to_is_taken_and_left_clean() {
        let temp = TempDir::new().unwrap();
        let folder = temp.path().join("new");

        check_writable(&folder).expect("a new folder is made and written to");

        assert!(folder.is_dir());
        assert_eq!(
            fs::read_dir(&folder).unwrap().count(),
            0,
            "the check leaves nothing behind"
        );
    }

    #[test]
    fn a_folder_the_app_cannot_write_to_is_refused_with_why() {
        let temp = TempDir::new().unwrap();
        // A file where the folder should be stands in for a folder the app may not write to.
        let file = temp.path().join("taken");
        fs::write(&file, b"x").unwrap();

        let error = check_writable(&file.join("inside"))
            .unwrap_err()
            .to_string();

        assert!(error.starts_with("The app can't save in "), "{error}");
    }

    #[test]
    fn a_folder_not_made_yet_reads_the_drive_it_will_be_on() {
        let temp = TempDir::new().unwrap();
        let folder = temp.path().join("not").join("made");

        let space = space_under(&folder).expect("the temp folder's drive answers");

        assert!(space.total > 0, "{space:?}");
        assert!(space.free <= space.total, "{space:?}");
        assert!(!folder.exists(), "asking makes no folder");
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_is_named_as_explorer_names_it() {
        assert_eq!(
            drive_of(Path::new(r"D:\Projects\Cameras\data\videos")),
            "D:"
        );
        assert_eq!(drive_of(Path::new(r"C:\")), "C:");
    }
}
