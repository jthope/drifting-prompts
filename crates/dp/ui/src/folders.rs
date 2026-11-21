//! The system's folder picker and file manager, through nsh's own calls. The file manager needs
//! absolute paths.

use std::path::{Path, PathBuf};

use nsh_core::{file_dialog::OpenDialog, prelude::*};

/// Asks for a folder, opening at `start` when it is one; `None` when the user cancels.
pub async fn pick(title: &str, start: &Path) -> Result<Option<PathBuf>, String> {
    let mut dialog = OpenDialog::folders().title(title);
    if !start.as_os_str().is_empty() {
        dialog = dialog.directory(start);
    }
    let picked = pick_paths(dialog)
        .await
        .map_err(|error| format!("The folder dialog couldn't open: {error}"))?;
    Ok(picked.and_then(|paths| paths.into_iter().next()))
}

/// Opens `folder` in the system's file manager.
pub async fn open(folder: &Path) -> Result<(), String> {
    let folder = absolute(folder);
    open_path(folder.clone())
        .await
        .map_err(|error| format!("Couldn't open {}: {error}", folder.display()))
}

/// Opens the system's file manager on the folder `file` is in, with `file` selected.
pub async fn show_file(file: &Path) -> Result<(), String> {
    let file = absolute(file);
    reveal_path(file.clone())
        .await
        .map_err(|error| format!("Couldn't show {}: {error}", file.display()))
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
