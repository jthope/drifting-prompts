//! Where snapshots and recordings are saved, as Settings changes it. A new folder takes effect at
//! once; files already saved stay where they are.

use std::path::PathBuf;

use ipcamera_storage::space::{check_writable, space_under};

use super::{HUB, explorer, off_main};
use crate::bridge::{DriveSpace, StorageAbilities, StorageFolder};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Snapshots,
    Videos,
}

impl Kind {
    fn named(name: &str) -> Result<Self, String> {
        match name {
            "snapshots" => Ok(Self::Snapshots),
            "videos" => Ok(Self::Videos),
            other => Err(format!("no folder for `{other}`")),
        }
    }

    fn current(self) -> PathBuf {
        match self {
            Self::Snapshots => HUB.config.snapshots_dir(),
            Self::Videos => HUB.config.videos_dir(),
        }
    }

    fn default_folder(self) -> PathBuf {
        match self {
            Self::Snapshots => HUB.config.default_snapshots_dir(),
            Self::Videos => HUB.config.default_videos_dir(),
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Snapshots => "Choose where snapshots are saved",
            Self::Videos => "Choose where videos are saved",
        }
    }

    /// `""` means the default folder.
    fn save(self, folder: String) -> Result<(), String> {
        HUB.config
            .update(|config| match self {
                Self::Snapshots => config.storage.snapshots = folder,
                Self::Videos => config.storage.videos = folder,
            })
            .map_err(|error| error.to_string())
    }

    /// A recording keeps writing where it started, so the videos' folder can't move while one runs.
    fn may_move(self) -> Result<(), String> {
        let recording = HUB
            .all()
            .iter()
            .any(|camera| camera.recording_file().is_some());
        if self == Self::Videos && recording {
            return Err("Stop recording before changing where videos are saved.".into());
        }
        Ok(())
    }

    fn info(self) -> StorageFolder {
        let current = self.current();
        StorageFolder {
            is_default: current == self.default_folder(),
            path: current.display().to_string(),
        }
    }
}

/// What the desktop can do with the folders.
pub fn storage_abilities() -> StorageAbilities {
    StorageAbilities {
        reveal_label: "Show in Explorer".to_string(),
        can_open_folder: true,
        can_choose_folder: true,
    }
}

/// Where `kind` (`"snapshots"` or `"videos"`) are saved now.
pub fn storage_folder(kind: &str) -> Result<StorageFolder, String> {
    Kind::named(kind).map(Kind::info)
}

/// Picks a folder with the system's dialog and saves `kind` there; canceling changes nothing.
pub async fn choose_storage(kind: String) -> Result<StorageFolder, String> {
    let kind = Kind::named(&kind)?;
    kind.may_move()?;
    let start = kind.current();
    let picked = dp_ui::folders::pick(kind.title(), &start).await?;
    let Some(folder) = picked else {
        return Ok(kind.info());
    };
    check_writable(&folder).map_err(|error| error.to_string())?;
    let saved = if folder == kind.default_folder() {
        String::new()
    } else {
        folder.display().to_string()
    };
    kind.save(saved)?;
    Ok(kind.info())
}

/// Saves `kind` in their default folder again.
pub fn reset_storage(kind: &str) -> Result<StorageFolder, String> {
    let kind = Kind::named(kind)?;
    kind.may_move()?;
    kind.save(String::new())?;
    Ok(kind.info())
}

/// Opens where `kind` are saved in Explorer, making the folder if it is missing.
pub async fn open_storage(kind: &str) -> Result<(), String> {
    explorer::open_folder(&Kind::named(kind)?.current()).await
}

/// How full the drive under `kind`'s folder is, off the main thread since a drive can be slow.
pub async fn storage_space(kind: String) -> Result<DriveSpace, String> {
    let folder = Kind::named(&kind)?.current();
    let space = off_main(move || space_under(&folder))
        .await?
        .map_err(|error| error.to_string())?;
    Ok(DriveSpace {
        drive: space.drive,
        free: space.free as f64,
        total: space.total as f64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_two_folders_have_names() {
        assert_eq!(Kind::named("snapshots"), Ok(Kind::Snapshots));
        assert_eq!(Kind::named("videos"), Ok(Kind::Videos));
        assert!(Kind::named("settings").is_err());
    }
}
