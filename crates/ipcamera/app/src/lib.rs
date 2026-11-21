//! The app's Rust code: what `App.tsx` imports from `ipcamera/native`.

mod bridge;
#[cfg(windows)]
mod camera;
mod form;
mod form_check;
mod library;
#[cfg(windows)]
mod video;
// Built in tests on Windows too, where the bridge doesn't call it.
#[cfg(any(not(windows), test))]
#[cfg_attr(all(windows, test), allow(dead_code))]
mod web_backend;

pub use dp_ui::Themes;
use nsh_core::prelude::*;

pub use crate::bridge::{
    AppWindow, Automation, Cameras, Events, Groups, Library, Preferences, Server, Snapshots,
    Storage, Videos, Views,
};

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

/// Bytes of decoded `<Image>` pictures kept off screen; nsh's larger default filled up.
#[cfg(windows)]
const PICTURE_MEMORY: usize = 64 << 20;

/// Runs before the app's first frame, so the picture budget is set here.
#[native_components(windows)]
fn components(components: &nsh_platform_windows::Components) {
    nsh_core::image::budget().set_limit(PICTURE_MEMORY);
    video::register(components);
}
