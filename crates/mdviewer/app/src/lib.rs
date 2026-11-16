//! The app's Rust code: what `App.tsx` imports from `mdviewer/native`. Their TypeScript
//! declarations are generated from these types into `.nsh/types/mdviewer/native.d.ts`.
//!
//! The bridge (`bridge.rs`) is thin: the projects, the files, the outline and the search are
//! `mdviewer_core`'s, and the themes, the window's place, the folder picker and Explorer are the
//! suite's (`dp_ui`).

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
