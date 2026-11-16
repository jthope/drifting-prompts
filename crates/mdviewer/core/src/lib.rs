//! Markdown Viewer's own model: its projects and settings, the Markdown files in a project, and
//! what a file's outline is.
//!
//! A project is a folder the user opened (New), listed in the sidebar by its folder's name. Its
//! Markdown files are whatever is in that folder and the folders under it, read again as they
//! change ([`files`]); nothing about them is saved but the tabs they are open in and which was in
//! front. A file shows raw or rendered, with its headings beside it ([`outline`]). A search finds
//! text in all of them, where the raw view would ([`search()`]).
//!
//! What the app saves is two files beside the executable, opened together as [`Saved`]:
//! `settings.json` for the window's preferences ([`ConfigStore`]) and `projects.json` for the
//! projects ([`ProjectStore`]).
//!
//! Nothing here draws anything: this crate has no UI and no nsh, so it compiles anywhere, and
//! `cargo test -p mdviewer-core` runs in milliseconds. The app (`crates/mdviewer/app`) reaches it
//! through its bridge.
//!
//! The types everything is built from are at the crate's root (`mdviewer_core::Config`,
//! `mdviewer_core::Heading`). [`files`] is a namespace of its own, for reading folders and files.

mod config;
pub mod files;
mod outline;
mod projects;
mod saved;
mod search;
mod syntax;

pub use config::{Config, ConfigStore, FilesView, TabOverflow, Ui, ViewMode, WindowState};
pub use dp_store::StoreError;
pub use files::{FilesError, Listing, MarkdownFile};
pub use outline::{Heading, Summary, summarize};
pub use projects::{Project, ProjectStore, SavedProjects, Tab};
pub use saved::Saved;
pub use search::{
    CONTEXT_LINE_MOST, ContextBlock, EXCERPT_AFTER, EXCERPT_BEFORE, EXCERPT_MATCH, Excerpt,
    FileMatches, Finder, MOST_MATCHES, SearchError, SearchMatch, SearchQuery, SearchResults,
    context, search,
};
pub use syntax::{Mark, MarkKind, Place, marks};
