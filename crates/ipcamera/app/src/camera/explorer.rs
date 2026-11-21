//! Showing the app's files and folders in Explorer.

use std::{fs, path::Path};

use ipcamera_storage::saved_file::FileKind;

/// Opens Explorer on `folder` with the file of `kind` called `name` selected.
pub async fn reveal(kind: &FileKind, folder: &Path, name: &str) -> Result<(), String> {
    let path = kind
        .locate(folder, name)
        .map_err(|error| error.to_string())?;
    dp_ui::folders::show_file(&path).await
}

/// Opens Explorer on `folder`, making it first if it is missing.
pub async fn open_folder(folder: &Path) -> Result<(), String> {
    fs::create_dir_all(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    dp_ui::folders::open(folder).await
}
