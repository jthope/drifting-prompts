//! The app's Rust code: the `mdviewer/native` modules, over `mdviewer_core` and `dp_ui`.

mod bridge;
mod dropped;

pub use dp_ui::Themes;
use nsh_core::prelude::*;

pub use crate::bridge::{AppWindow, Documents, Files, Preferences, Projects, Search};

/// The crate's UI set (`nsh.json` `"rust"`).
#[native_module("mdviewer/native")]
fn native(registry: &mut Registry) {
    registry
        .module::<Projects>()
        .module::<Files>()
        .module::<Documents>()
        .module::<Search>()
        .module::<Preferences>()
        .module::<Themes>()
        .module::<AppWindow>();
}
