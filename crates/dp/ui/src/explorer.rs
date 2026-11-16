//! Showing a file or a folder in Explorer: a file selected in its folder, or a folder opened.
//!
//! nsh 0.5.1 has no call for either, so Explorer is started with the path. It is started and
//! left: its exit code means nothing (it is 1 even when the window opened), so nothing waits on
//! it.

use std::{os::windows::process::CommandExt, path::Path, process::Command};

/// Opens Explorer on the folder `file` is in, with `file` selected.
pub fn show_file(file: &Path) -> Result<(), String> {
    // Explorer reads `/select,` and the quoted path as one argument, which `arg` would quote as a
    // whole.
    explorer(|command| {
        command.raw_arg(format!("/select,\"{}\"", file.display()));
    })
}

/// Opens Explorer on `folder`.
pub fn open_folder(folder: &Path) -> Result<(), String> {
    explorer(|command| {
        command.arg(folder);
    })
}

fn explorer(arguments: impl FnOnce(&mut Command)) -> Result<(), String> {
    let mut command = Command::new("explorer.exe");
    arguments(&mut command);
    command
        .spawn()
        .map(drop)
        .map_err(|error| format!("Explorer couldn't open: {error}"))
}
