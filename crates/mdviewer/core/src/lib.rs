//! Markdown Viewer's model: projects and settings, the Markdown files in a project, a file's
//! outline, syntax marks and rendered page, and search. No UI and no nsh.

mod config;
pub mod files;
mod outline;
mod projects;
mod render;
mod saved;
mod search;
mod syntax;

pub use config::{Config, ConfigStore, FilesView, TabOverflow, Ui, ViewMode, WindowState};
pub use dp_store::StoreError;
pub use files::{FilesError, Listing, MarkdownFile};
pub use outline::{Heading, Summary, summarize};
pub use projects::{Project, ProjectStore, SavedProjects, Tab};
pub use render::render;
pub use saved::Saved;
pub use search::{
    CONTEXT_LINE_MOST, ContextBlock, EXCERPT_AFTER, EXCERPT_BEFORE, EXCERPT_MATCH, Excerpt,
    FileMatches, Finder, MOST_MATCHES, SearchError, SearchMatch, SearchQuery, SearchResults,
    context, search,
};
pub use syntax::{Mark, MarkKind, Place, marks};
