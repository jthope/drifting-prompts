//! Showing the app's files in Explorer: a snapshot or a recording selected in its folder, or a
//! folder opened.
//!
//! Which file a name means is `ipcamera_storage`'s rule ([`FileKind::locate`]), the same the server
//! keeps; opening Explorer is the desktop's alone, and the suite's (`dp_ui::explorer`).

use std::{fs, path::Path};

use ipcamera_storage::saved_file::{FileKind, absolute};

/// Opens Explorer on `folder` with the file of `kind` called `name` selected.
pub fn reveal(kind: &FileKind, folder: &Path, name: &str) -> Result<(), String> {
    let path = kind
        .locate(folder, name)
        .map_err(|error| error.to_string())?;
    dp_ui::explorer::show_file(&path)
}

/// Opens Explorer on `folder`, making it first if nothing has been saved there yet.
pub fn open_folder(folder: &Path) -> Result<(), String> {
    fs::create_dir_all(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    dp_ui::explorer::open_folder(&absolute(folder))
}
