//! The app's Rust code: what `App.tsx` imports from `ipcamera/native`. Their
//! TypeScript declarations are generated from these types into
//! `.nsh/types/ipcamera/native.d.ts`.

mod bridge;
#[cfg(windows)]
mod camera;
mod form;
mod form_check;
mod library;
// Built for tests on Windows too, so `cargo test` there covers the web backend's own logic. The
// bridge doesn't call it there, hence the allowance.
#[cfg(windows)]
mod video;
#[cfg(any(not(windows), test))]
#[cfg_attr(all(windows, test), allow(dead_code))]
mod web_backend;

pub use dp_ui::Themes;
use nsh_core::prelude::*;

pub use crate::bridge::{
    AppWindow, Automation, Cameras, Events, Groups, Library, Preferences, Server, Snapshots,
    Storage, Videos, Views,
};

/// The crate's UI set (`nsh.json` `"rust"`).
#[native_module("ipcamera/native")]
fn native(registry: &mut Registry) {
    registry
        .module::<Cameras>()
        .module::<Groups>()
        .module::<Views>()
        .module::<Library>()
        .module::<Snapshots>()
        .module::<Videos>()
        .module::<Events>()
        .module::<Automation>()
        .module::<Storage>()
        .module::<Server>()
        .module::<Preferences>()
        .module::<Themes>()
        .module::<AppWindow>();
}

/// What the decoded pictures `<Image>` shows may keep in memory once off screen, in bytes. nsh
/// keeps up to a sixteenth of the machine's memory by default, 256 MiB, and a few visits to the
/// Snapshots and Videos pages filled it; the galleries' thumbnails are small, so this holds a few
/// pages of them and the pictures last opened. Those on screen are kept whatever this says.
#[cfg(windows)]
const PICTURE_MEMORY: usize = 64 << 20;

/// The components the host app registers: on Windows, the live picture.
///
/// nsh calls this as it makes the view, before the app's first frame, so this is also where the
/// decoded pictures' memory is limited.
#[native_components(windows)]
fn components(components: &nsh_platform_windows::Components) {
    nsh_core::image::budget().set_limit(PICTURE_MEMORY);
    video::register(components);
}
