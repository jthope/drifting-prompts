//! The system's folder picker: Windows' own Open dialog, choosing a folder.
//!
//! nsh 0.5.1 picks files but not folders, so this shows `IFileOpenDialog` itself. It runs on a
//! thread of its own, in the single-threaded COM apartment the dialog needs, and the app's window
//! owns it, so the window waits behind it as behind any modal dialog while the app keeps drawing.

use std::{
    path::{Path, PathBuf},
    thread,
};

use nsh_platform_windows::windows::{
    Win32::{
        Foundation::{ERROR_CANCELLED, HWND, LPARAM},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
            },
            Threading::GetCurrentProcessId,
        },
        UI::{
            Shell::{
                FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog,
                IFileOpenDialog, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
            },
            WindowsAndMessaging::{
                EnumWindows, GW_OWNER, GWL_STYLE, GetWindow, GetWindowLongPtrW,
                GetWindowThreadProcessId, WS_CAPTION,
            },
        },
    },
    core::{BOOL, HRESULT, HSTRING, Result as WinResult},
};

/// Asks for a folder, titled `title` and opening at `start` when it exists. `None` when the user
/// cancels. Blocks until the dialog closes, so call it off the engine's thread.
pub fn pick(title: &str, start: &Path) -> Result<Option<PathBuf>, String> {
    let title = title.to_owned();
    let start = start.to_owned();
    // A window handle can't cross threads as itself, so it goes as its number.
    let owner = main_window().map(|window| window.0 as isize);
    thread::spawn(move || show(owner, &title, &start))
        .join()
        .map_err(|_| "The folder dialog closed unexpectedly.".to_string())?
}

fn show(owner: Option<isize>, title: &str, start: &Path) -> Result<Option<PathBuf>, String> {
    // SAFETY: this thread is the dialog's alone, so it can take the apartment the dialog needs,
    // and gives it back below whatever happened in between.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
        .ok()
        .map_err(|error| format!("The folder dialog couldn't open: {}", error.message()))?;
    let picked = ask(owner, title, start);
    // SAFETY: balances the `CoInitializeEx` above, on the same thread, after the dialog is gone.
    unsafe { CoUninitialize() };
    picked.map_err(|error| format!("The folder dialog couldn't open: {}", error.message()))
}

fn ask(owner: Option<isize>, title: &str, start: &Path) -> WinResult<Option<PathBuf>> {
    // SAFETY: plain COM calls on a dialog this thread made, in the apartment it set up.
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        dialog.SetOptions(
            dialog.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST,
        )?;
        dialog.SetTitle(&HSTRING::from(title))?;
        // Opening somewhere else is only a convenience, so a folder that can't be opened is skipped.
        if start.is_dir()
            && let Ok(folder) = SHCreateItemFromParsingName::<_, _, IShellItem>(
                &HSTRING::from(start.as_os_str()),
                None,
            )
        {
            let _ = dialog.SetFolder(&folder);
        }

        if let Err(error) = dialog.Show(owner.map(|raw| HWND(raw as _))) {
            return if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
                Ok(None)
            } else {
                Err(error)
            };
        }

        let name = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
        let path = name.to_string();
        CoTaskMemFree(Some(name.0 as _));
        Ok(path.ok().map(PathBuf::from))
    }
}

/// This process's main window, to own the dialog: the top-level window with a title bar that
/// nothing owns.
///
/// That rules out the hidden helper windows Windows makes for the process, such as the IME's.
/// nsh takes the frame off in full screen, so the dialog has no owner there and opens on its own.
fn main_window() -> Option<HWND> {
    unsafe extern "system" fn visit(window: HWND, found: LPARAM) -> BOOL {
        let mut owner_process = 0;
        // SAFETY: plain calls on the handle Windows passed in. `found` is the `Option<HWND>` below,
        // which outlives `EnumWindows`, and this runs on the thread that called it.
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut owner_process));
            let unowned = GetWindow(window, GW_OWNER).is_err();
            if owner_process == GetCurrentProcessId() && unowned && has_frame(window) {
                *(found.0 as *mut Option<HWND>) = Some(window);
                return false.into();
            }
        }
        true.into()
    }

    let mut found: Option<HWND> = None;
    // SAFETY: `visit` only writes through the pointer while this call runs. `EnumWindows` reports
    // an error when the callback stops it early, which is the success case here.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&raw mut found as isize)) };
    found
}

/// Whether `window` has a title bar.
fn has_frame(window: HWND) -> bool {
    // SAFETY: a plain read of the window's style.
    let style = unsafe { GetWindowLongPtrW(window, GWL_STYLE) } as u32;
    style & WS_CAPTION.0 == WS_CAPTION.0
}
