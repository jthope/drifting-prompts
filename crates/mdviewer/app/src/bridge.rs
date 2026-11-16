//! What TSX sees of the app: the types and functions it imports from `mdviewer/native`.
//!
//! What the app saves is `mdviewer_core::Saved`, opened the first time anything asks:
//! `settings.json` and `projects.json` beside the executable. Whatever reads a disk runs off the
//! main thread (`blocking`): the projects' folders, a file's text, a search, a save. Reading what
//! is already in memory stays a plain call.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use dp_ui::{SavedAppearance, WindowPlace, listen, listen_later};
use mdviewer_core::{
    ConfigStore, Finder, Listing, MOST_MATCHES, Project, ProjectStore, Saved, Tab, files, marks,
    summarize,
};
use nsh_core::prelude::*;

use crate::dropped;

/// How often the projects' list is looked at. It changes only as the app changes it, so this is
/// how soon every region sees a change one of them made.
const PROJECTS_CHECK_EVERY: Duration = Duration::from_millis(250);

/// How often a project's folder is listed again, off the main thread: soon enough that a file
/// added in another program shows by itself, rare enough that a large folder costs little.
const FILES_CHECK_EVERY: Duration = Duration::from_secs(2);

/// How often the open file's size and time are looked at, to show it again once it changed: a
/// file saved in an editor shows here within a second.
const FILE_CHECK_EVERY: Duration = Duration::from_secs(1);

/// How often a search lists the project's folder again, off the main thread, to search it again
/// once a file changed: as often as the files panel lists it.
const SEARCH_CHECK_EVERY: Duration = FILES_CHECK_EVERY;

/// What the app saves: `settings.json` and `projects.json` beside the executable. For `nsh test`'s
/// headless runner, which sits beside the development host, files of its own in a throwaway
/// folder, so tests start the same way on every machine and never change what the app saved; for
/// the development host, a folder beside it that `nsh run` never takes for the app's code
/// (`dp_ui::development_folder`).
static SAVED: LazyLock<Saved> = LazyLock::new(|| {
    if dp_ui::is_test_runner() {
        let folder = std::env::temp_dir().join("mdviewer-nsh-tests");
        let _ = std::fs::remove_dir_all(&folder);
        Saved::open(&folder)
    } else if let Some(folder) = dp_ui::development_folder() {
        Saved::open(&folder)
    } else {
        Saved::beside_the_executable()
    }
});

/// The app's settings: `settings.json`.
pub(crate) fn settings() -> &'static ConfigStore {
    &SAVED.settings
}

/// The app's projects: `projects.json`.
fn projects() -> &'static ProjectStore {
    &SAVED.projects
}

/// Runs `work` on a thread of its own, because a TSX call must not wait on a disk or a dialog.
async fn off_main<R: Send + 'static>(
    work: impl FnOnce() -> R + Send + 'static,
) -> Result<R, String> {
    blocking(work).await.map_err(|error| error.to_string())
}

/// A project, as the sidebar lists it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct ProjectInfo {
    /// Its folder, which is also how the app names the project.
    pub path: String,
    /// Its folder's name.
    pub name: String,
    /// The file it had in front last, in the project (`docs/setup.md`); empty for none.
    pub last_file: String,
    /// The files it had open in tabs, in their order.
    pub tabs: Vec<TabInfo>,
}

/// A file open in a tab of a project.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct TabInfo {
    /// The file, in the project (`docs/setup.md`).
    pub file: String,
    /// Pinned: kept first, and left open by closing the others.
    pub pinned: bool,
    /// A preview: the tab the next file opened with a single press takes over, until it is kept.
    pub preview: bool,
    /// How it shows.
    pub mode: ViewMode,
}

impl From<&Tab> for TabInfo {
    fn from(tab: &Tab) -> Self {
        Self {
            file: tab.file.clone(),
            pinned: tab.pinned,
            preview: tab.preview,
            mode: tab.mode.into(),
        }
    }
}

impl From<TabInfo> for Tab {
    fn from(tab: TabInfo) -> Self {
        Self {
            file: tab.file,
            pinned: tab.pinned,
            preview: tab.preview,
            mode: tab.mode.into(),
        }
    }
}

/// The projects, and which one is open.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct ProjectList {
    /// The projects, newest first.
    pub projects: Vec<ProjectInfo>,
    /// The open project's folder; empty while New shows.
    pub selected: String,
}

fn project_info(project: &Project) -> ProjectInfo {
    ProjectInfo {
        path: project.path.display().to_string(),
        name: project.name(),
        last_file: project.last_file.clone().unwrap_or_default(),
        tabs: project.tabs.iter().map(TabInfo::from).collect(),
    }
}

/// The projects: the folders of Markdown files the user opened.
#[module]
pub struct Projects;

#[module]
impl Projects {
    /// The projects, newest first, and which one is open.
    pub fn list() -> ProjectList {
        let saved = projects().current();
        ProjectList {
            projects: saved.projects.iter().map(project_info).collect(),
            selected: saved
                .selected
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        }
    }

    /// Calls `listener` with the projects now and after every change.
    pub fn on_change(#[args(projects)] listener: Callback<ProjectList>, cx: CallCx<'_>) -> Stop {
        listen(cx, listener, PROJECTS_CHECK_EVERY, Self::list)
    }

    /// What went wrong reading `settings.json` or `projects.json` as the app started, to say once;
    /// empty when nothing did.
    pub fn problem() -> String {
        SAVED.problem().unwrap_or_default().to_owned()
    }

    /// Asks for a folder with the system's folder picker and opens it as a project: resolves
    /// with whether one was chosen.
    pub async fn choose() -> Result<bool, String> {
        let start = projects()
            .current()
            .projects
            .first()
            .and_then(|project| project.path.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let Some(folder) = off_main(move || pick_folder(&start)).await?? else {
            return Ok(false);
        };
        Self::open(folder.display().to_string()).await?;
        Ok(true)
    }

    /// Opens folder `path` as a project and selects it, first in the list.
    pub async fn open(path: String) -> Result<(), String> {
        off_main(move || projects().open_project(Path::new(&path)))
            .await?
            .map_err(|error| error.to_string())
    }

    /// Opens the first folder of what was dropped on the window (`file:` URLs, as `onFileDrop`
    /// gives them) as a project; refuses a drop of files alone, saying what to drop instead.
    pub async fn open_dropped(uris: Vec<String>) -> Result<(), String> {
        let paths: Vec<PathBuf> = uris
            .iter()
            .filter_map(|uri| dropped::path_of(uri))
            .collect();
        let folder = off_main(move || paths.into_iter().find(|path| path.is_dir())).await?;
        match folder {
            Some(folder) => Self::open(folder.display().to_string()).await,
            None => Err("Drop a folder of Markdown files to open it as a project.".to_owned()),
        }
    }

    /// Opens the project of folder `path`, or New when `path` is empty.
    pub async fn select(path: String) -> Result<(), String> {
        off_main(move || {
            let folder = (!path.is_empty()).then(|| PathBuf::from(&path));
            projects().select_project(folder.as_deref())
        })
        .await?
        .map_err(|error| error.to_string())
    }

    /// Takes the project of folder `path` off the list. The folder and its files stay as they
    /// are.
    pub async fn remove(path: String) -> Result<(), String> {
        off_main(move || projects().remove_project(Path::new(&path)))
            .await?
            .map_err(|error| error.to_string())
    }

    /// Remembers the tabs the project of `path` has open, in their order, and `front`, the file
    /// in front of them (empty for none), to open again with it. A failure is silent: it is a
    /// convenience.
    pub async fn remember_tabs(path: String, tabs: Vec<TabInfo>, front: String) {
        let _ = off_main(move || {
            projects().set_tabs(
                Path::new(&path),
                tabs.into_iter().map(Tab::from).collect(),
                (!front.is_empty()).then_some(front),
            )
        })
        .await;
    }

    /// Opens Explorer on the project of folder `path`, with `file` (in the project) selected, or
    /// on the folder itself when `file` is empty.
    pub async fn show_in_explorer(path: String, file: String) -> Result<(), String> {
        off_main(move || show_in_explorer(Path::new(&path), &file)).await?
    }
}

#[cfg(windows)]
fn pick_folder(start: &Path) -> Result<Option<PathBuf>, String> {
    dp_ui::folder_dialog::pick("Open a folder of Markdown files", start)
}

#[cfg(not(windows))]
fn pick_folder(_start: &Path) -> Result<Option<PathBuf>, String> {
    Err(
        "This system has no folder picker here yet: drop a folder on the window instead."
            .to_owned(),
    )
}

#[cfg(windows)]
fn show_in_explorer(folder: &Path, file: &str) -> Result<(), String> {
    if file.is_empty() {
        return dp_ui::explorer::open_folder(folder);
    }
    let path = files::locate(folder, file).map_err(|error| error.to_string())?;
    dp_ui::explorer::show_file(&path)
}

#[cfg(not(windows))]
fn show_in_explorer(_folder: &Path, _file: &str) -> Result<(), String> {
    Err("This build can't show files in a file manager.".to_owned())
}

/// A Markdown file in a project, as the files panel lists it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct FileEntry {
    /// Its path in the project, with `/` between its parts: `docs/setup.md`. The app names the
    /// file by it.
    pub path: String,
    /// Its name: `setup.md`.
    pub name: String,
    /// The folder it is in, in the project: `docs`, or empty at the project's top.
    pub folder: String,
}

/// The Markdown files of a project.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct FileList {
    /// The files, in each folder its files first, then its folders, each by name.
    pub files: Vec<FileEntry>,
    /// Whether the folder held more than the panel lists, which it stopped at.
    pub truncated: bool,
    /// Why the folder couldn't be listed (moved, renamed or deleted since); empty when it could.
    pub problem: String,
}

/// Lists the project in `folder`, as the files panel shows it.
fn file_list(folder: &Path) -> FileList {
    match files::list(folder) {
        Ok(listing) => FileList {
            files: listing
                .files
                .iter()
                .map(|file| FileEntry {
                    path: file.path.clone(),
                    name: file.name().to_owned(),
                    folder: file.folder().to_owned(),
                })
                .collect(),
            truncated: listing.truncated,
            problem: String::new(),
        },
        Err(error) => FileList {
            files: Vec::new(),
            truncated: false,
            problem: error.to_string(),
        },
    }
}

/// The Markdown files in the projects.
#[module]
pub struct Files;

#[module]
impl Files {
    /// Calls `listener` with the Markdown files of the project of folder `project` now and
    /// whenever they change: a file added, renamed or deleted, here or in another program. The
    /// folder is listed off the main thread.
    pub fn on_change(
        project: String,
        #[args(files)] listener: Callback<FileList>,
        cx: CallCx<'_>,
    ) -> Stop {
        let folder = PathBuf::from(project);
        listen_later(cx, listener, FILES_CHECK_EVERY, move || {
            let folder = folder.clone();
            async move { off_main(move || file_list(&folder)).await.ok() }
        })
    }
}

/// A heading of a file, as the outline lists it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct HeadingInfo {
    /// Its level: 1 for `#`, up to 6.
    pub level: f64,
    /// Its text, without the Markdown around it.
    pub title: String,
    /// The line it starts on, from 0: where the raw view goes to.
    pub line: f64,
}

/// What a part of a file's text is, as the raw view colors it.
#[derive(Clone, Copy, Debug, PartialEq, PropEnum)]
pub enum MarkKind {
    /// Markdown's own punctuation: a heading's `#`s, `**`, backticks, a link's brackets, a fence, a
    /// table's pipes.
    Punctuation,
    /// A heading's text.
    Heading,
    /// Strong text, between its `**`.
    Strong,
    /// Emphasized text, between its `*` or `_`.
    Emphasis,
    /// Struck-through text, between its `~~`.
    Strikethrough,
    /// Code in text, or a formula, between its backticks.
    Code,
    /// A link's text, or a picture's.
    Link,
    /// Where a link or a picture goes.
    Url,
    /// A list item's marker, a task's box, a quote's `>`, a thematic break, a footnote's label.
    Marker,
    /// A quote's text, after its `>`.
    Quote,
    /// A code block's lines, whole: a band behind them.
    CodeBlock,
    /// The language a fence names.
    Language,
    /// The front matter, the file's data at its top.
    FrontMatter,
}

impl From<mdviewer_core::MarkKind> for MarkKind {
    fn from(kind: mdviewer_core::MarkKind) -> Self {
        use mdviewer_core::MarkKind as Core;
        match kind {
            Core::Punctuation => Self::Punctuation,
            Core::Heading => Self::Heading,
            Core::Strong => Self::Strong,
            Core::Emphasis => Self::Emphasis,
            Core::Strikethrough => Self::Strikethrough,
            Core::Code => Self::Code,
            Core::Link => Self::Link,
            Core::Url => Self::Url,
            Core::Marker => Self::Marker,
            Core::Quote => Self::Quote,
            Core::CodeBlock => Self::CodeBlock,
            Core::Language => Self::Language,
            Core::FrontMatter => Self::FrontMatter,
        }
    }
}

/// A part of a file's text and what it is (`mdviewer_core::marks`), in the raw view's terms: lines
/// from 0, and columns in UTF-16 code units, as the editor counts them.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct MarkInfo {
    /// What it is.
    pub kind: MarkKind,
    /// The line it starts on.
    pub start_line: f64,
    /// Where on that line it starts.
    pub start_column: f64,
    /// The line it ends on.
    pub end_line: f64,
    /// Where on that line it ends, not included.
    pub end_column: f64,
}

/// A file, as the window shows it.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct OpenFile {
    /// Its text.
    pub text: String,
    /// What each part of its text is, for the raw view to color, in the order they start.
    pub marks: Vec<MarkInfo>,
    /// Its top-level headings, in order: the outline, and the sections of the rendered view.
    pub headings: Vec<HeadingInfo>,
    /// How many words it has, code left out.
    pub words: f64,
    /// Why it couldn't be read (gone, too large); empty when it could.
    pub problem: String,
}

/// Reads file `name` of the project in `folder`, with its outline.
fn open_file(folder: &Path, name: &str) -> OpenFile {
    match files::read(folder, name) {
        Ok(text) => {
            let summary = summarize(&text);
            OpenFile {
                marks: marks(&text)
                    .into_iter()
                    .map(|mark| MarkInfo {
                        kind: mark.kind.into(),
                        start_line: f64::from(mark.start.line),
                        start_column: f64::from(mark.start.column),
                        end_line: f64::from(mark.end.line),
                        end_column: f64::from(mark.end.column),
                    })
                    .collect(),
                headings: summary
                    .headings
                    .into_iter()
                    .map(|heading| HeadingInfo {
                        level: f64::from(heading.level),
                        title: heading.title,
                        line: f64::from(heading.line),
                    })
                    .collect(),
                words: summary.words as f64,
                text,
                problem: String::new(),
            }
        }
        Err(error) => OpenFile {
            text: String::new(),
            marks: Vec::new(),
            headings: Vec::new(),
            words: 0.0,
            problem: error.to_string(),
        },
    }
}

/// The open file.
#[module]
pub struct Documents;

#[module]
impl Documents {
    /// Calls `listener` with file `file` of the project of folder `project` (its text and
    /// outline) now and whenever it changes on disk, as when it is saved in an editor. Its size
    /// and time are looked at every second, off the main thread, and it is read again only when
    /// they changed.
    pub fn on_change(
        project: String,
        file: String,
        #[args(file)] listener: Callback<OpenFile>,
        cx: CallCx<'_>,
    ) -> Stop {
        let listener = Rc::new(RefCell::new(Some(listener)));
        let held = listener.clone();
        let folder = PathBuf::from(project);
        cx.start(async move {
            // The size and time of the file as last shown; `Some(None)` once it was found gone.
            let mut shown: Option<Option<(u64, u64)>> = None;
            loop {
                if held.borrow().is_none() {
                    return;
                }
                let (where_, name, before) = (folder.clone(), file.clone(), shown);
                let read = off_main(move || {
                    let now = files::stamp(&where_, &name);
                    (before != Some(now)).then(|| (now, open_file(&where_, &name)))
                })
                .await;
                let Some(listener) = held.borrow().clone() else {
                    return;
                };
                if let Ok(Some((now, opened))) = read {
                    shown = Some(now);
                    listener.post(opened);
                }
                timer(FILE_CHECK_EVERY).await;
            }
        });
        Stop::new(move || drop(listener.take()))
    }
}

/// What a search finds, and how.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct SearchQuery {
    /// The text to find, or a regular expression with `regex`. Empty finds nothing.
    pub text: String,
    /// Only text of the same case; otherwise any case.
    pub case_sensitive: bool,
    /// Only whole words.
    pub whole_word: bool,
    /// `text` is a regular expression, in the syntax the raw view's search reads (Rust's `regex`).
    pub regex: bool,
}

impl From<SearchQuery> for mdviewer_core::SearchQuery {
    fn from(query: SearchQuery) -> Self {
        Self {
            text: query.text,
            case_sensitive: query.case_sensitive,
            whole_word: query.whole_word,
            regex: query.regex,
        }
    }
}

/// A match in a file, in the raw view's terms: lines from 0, and columns in UTF-16 code units, as
/// the editor counts them.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct MatchInfo {
    /// The line it starts on.
    pub start_line: f64,
    /// Where on that line it starts.
    pub start_column: f64,
    /// The line it ends on.
    pub end_line: f64,
    /// Where on that line it ends, not included.
    pub end_column: f64,
    /// The line it starts on, as a list of results shows it: its indent left out, shortened around
    /// the match with `…` where words were left out.
    pub excerpt: String,
    /// Where the match starts in `excerpt`, in UTF-16 code units.
    pub excerpt_start: f64,
    /// Where it ends in `excerpt`, not included, in UTF-16 code units.
    pub excerpt_end: f64,
}

/// The matches in one file.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct FileMatches {
    /// The project the file is in, by its folder, as the search was given it.
    pub project: String,
    /// The file, by its path in the project (`docs/setup.md`).
    pub path: String,
    /// Its matches, in order.
    pub matches: Vec<MatchInfo>,
}

/// What a search found.
#[derive(Clone, Debug, Default, PartialEq, JsObject)]
pub struct SearchResults {
    /// The files with matches, the projects in the order they were given, each project's files in
    /// the files panel's order; a file with none is left out.
    pub files: Vec<FileMatches>,
    /// How many matches, in all the files.
    pub total: f64,
    /// Whether the search stopped at the most it counts (10,000), leaving later matches out.
    pub truncated: bool,
    /// Why the query finds nothing (it is meant as a regular expression and isn't one), as a
    /// sentence; empty when it was searched for.
    pub problem: String,
}

/// Where a search looks: some or all of a project's files.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct SearchPlace {
    /// The project, by its folder.
    pub project: String,
    /// Every Markdown file of the project, rather than only `files`.
    pub every_file: bool,
    /// The files to search when not every one, by their paths in the project (`docs/setup.md`).
    pub files: Vec<String>,
}

impl SearchResults {
    /// Adds what a search of `project` found to these results.
    fn add(&mut self, project: &str, found: mdviewer_core::SearchResults) {
        self.total += found.total as f64;
        self.truncated |= found.truncated;
        self.files.extend(found.files.into_iter().map(|file| {
            FileMatches {
                project: project.to_owned(),
                path: file.path,
                matches: file
                    .matches
                    .into_iter()
                    .map(|found| MatchInfo {
                        start_line: f64::from(found.start.line),
                        start_column: f64::from(found.start.column),
                        end_line: f64::from(found.end.line),
                        end_column: f64::from(found.end.column),
                        excerpt: found.excerpt.text,
                        excerpt_start: f64::from(found.excerpt.start),
                        excerpt_end: f64::from(found.excerpt.end),
                    })
                    .collect(),
            }
        }));
    }
}

/// The files of each of `places`, as their folders list them now: a folder that can't be listed
/// has none (the files panel says why).
fn listings(places: &[SearchPlace]) -> Vec<Listing> {
    places
        .iter()
        .map(|place| {
            let mut listing = files::list(Path::new(&place.project)).unwrap_or_default();
            if !place.every_file {
                listing
                    .files
                    .retain(|file| place.files.contains(&file.path));
            }
            listing
        })
        .collect()
}

/// What `finder` finds in `places`, at most `MOST_MATCHES` in all, unless their files are those of
/// `searched` still, or `stop` was set first: with the listings it searched, to tell next time.
fn search_again(
    places: &[SearchPlace],
    finder: &Finder,
    searched: Option<&[Listing]>,
    stop: &AtomicBool,
) -> Option<(Vec<Listing>, SearchResults)> {
    let listed = listings(places);
    if searched == Some(listed.as_slice()) {
        return None;
    }
    let mut results = SearchResults::default();
    for (place, listing) in places.iter().zip(&listed) {
        let room = MOST_MATCHES.saturating_sub(results.total as usize);
        let found = mdviewer_core::search(
            Path::new(&place.project),
            &listing.files,
            finder,
            room,
            stop,
        )?;
        results.add(&place.project, found);
        if results.truncated {
            break;
        }
    }
    Some((listed, results))
}

/// Finding text in the projects.
#[module]
pub struct Search;

#[module]
impl Search {
    /// Calls `listener` with what `query` finds in `places` (a file, the files open in tabs, a
    /// project, every project): once they are searched, off the main thread, and again whenever a
    /// file of them changes, here or in another program. Stopping it calls off a search on its
    /// way, so a newer query never waits for this one. A query that isn't a regular expression it
    /// says it is gets its `problem` at once.
    pub fn on_change(
        places: Vec<SearchPlace>,
        query: SearchQuery,
        #[args(results)] listener: Callback<SearchResults>,
        cx: CallCx<'_>,
    ) -> Stop {
        let finder = match Finder::new(&query.into()) {
            Ok(Some(finder)) => Arc::new(finder),
            Ok(None) => {
                listener.post(SearchResults::default());
                return Stop::new(|| {});
            }
            Err(error) => {
                listener.post(SearchResults {
                    problem: error.to_string(),
                    ..SearchResults::default()
                });
                return Stop::new(|| {});
            }
        };
        let listener = Rc::new(RefCell::new(Some(listener)));
        let held = listener.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let places = Arc::new(places);
        cx.start(async move {
            // The places' files as they were when they were searched last.
            let mut searched: Option<Vec<Listing>> = None;
            loop {
                if held.borrow().is_none() {
                    return;
                }
                let (places, finder, stop, before) = (
                    places.clone(),
                    finder.clone(),
                    stop.clone(),
                    searched.clone(),
                );
                let found =
                    off_main(move || search_again(&places, &finder, before.as_deref(), &stop))
                        .await;
                let Some(listener) = held.borrow().clone() else {
                    return;
                };
                if let Ok(Some((listed, results))) = found {
                    searched = Some(listed);
                    listener.post(results);
                }
                timer(SEARCH_CHECK_EVERY).await;
            }
        });
        Stop::new(move || {
            stopping.store(true, Ordering::Relaxed);
            drop(listener.take());
        })
    }

    /// The lines of file `file` of the project in `project` (its folder) around each of `lines`
    /// (from 0), `around` before and after each, in blocks that never overlap: what the results
    /// tab shows of each match, in its place. Read off the main thread.
    pub async fn context(
        project: String,
        file: String,
        lines: Vec<f64>,
        around: f64,
    ) -> Result<Vec<ContextInfo>, String> {
        off_main(move || {
            let text =
                files::read(Path::new(&project), &file).map_err(|error| error.to_string())?;
            let lines: Vec<u32> = lines.iter().map(|line| *line as u32).collect();
            Ok(mdviewer_core::context(&text, &lines, around as u32)
                .into_iter()
                .map(|block| ContextInfo {
                    first: f64::from(block.first),
                    lines: block.lines,
                })
                .collect())
        })
        .await?
    }
}

/// Lines of a file around its matches (`Search.context`).
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct ContextInfo {
    /// The first line's number, from 0.
    pub first: f64,
    /// The lines, from `first` on, as the raw view has them; one longer than 400 characters is
    /// shortened, ending with `…`.
    pub lines: Vec<String>,
}

/// How a file shows: its text, the page it makes, or both.
#[derive(Clone, Copy, Debug, PartialEq, PropEnum)]
pub enum ViewMode {
    /// The Markdown as written, highlighted as a code editor highlights it.
    Raw,
    /// The text on the left and the page on the right.
    Split,
    /// The page the Markdown makes.
    Rendered,
}

impl From<mdviewer_core::ViewMode> for ViewMode {
    fn from(mode: mdviewer_core::ViewMode) -> Self {
        match mode {
            mdviewer_core::ViewMode::Raw => Self::Raw,
            mdviewer_core::ViewMode::Split => Self::Split,
            mdviewer_core::ViewMode::Rendered => Self::Rendered,
        }
    }
}

impl From<ViewMode> for mdviewer_core::ViewMode {
    fn from(mode: ViewMode) -> Self {
        match mode {
            ViewMode::Raw => Self::Raw,
            ViewMode::Split => Self::Split,
            ViewMode::Rendered => Self::Rendered,
        }
    }
}

/// How the files panel lists a project's files.
#[derive(Clone, Copy, Debug, PartialEq, PropEnum)]
pub enum FilesView {
    /// The folders nested as they are on disk, each opening and closing.
    Tree,
    /// One heading for each folder that holds files, its whole path, with its files under it.
    Folder,
    /// Every file in one list, each with its folder beside it.
    List,
}

impl From<mdviewer_core::FilesView> for FilesView {
    fn from(view: mdviewer_core::FilesView) -> Self {
        match view {
            mdviewer_core::FilesView::Tree => Self::Tree,
            mdviewer_core::FilesView::Folder => Self::Folder,
            mdviewer_core::FilesView::List => Self::List,
        }
    }
}

impl From<FilesView> for mdviewer_core::FilesView {
    fn from(view: FilesView) -> Self {
        match view {
            FilesView::Tree => Self::Tree,
            FilesView::Folder => Self::Folder,
            FilesView::List => Self::List,
        }
    }
}

/// What the tab strip does with more tabs than fit in one row.
#[derive(Clone, Copy, Debug, PartialEq, PropEnum)]
pub enum TabOverflow {
    /// One row, which scrolls sideways.
    Scroll,
    /// As many rows as the tabs take.
    Wrap,
}

impl From<mdviewer_core::TabOverflow> for TabOverflow {
    fn from(overflow: mdviewer_core::TabOverflow) -> Self {
        match overflow {
            mdviewer_core::TabOverflow::Scroll => Self::Scroll,
            mdviewer_core::TabOverflow::Wrap => Self::Wrap,
        }
    }
}

impl From<TabOverflow> for mdviewer_core::TabOverflow {
    fn from(overflow: TabOverflow) -> Self {
        match overflow {
            TabOverflow::Scroll => Self::Scroll,
            TabOverflow::Wrap => Self::Wrap,
        }
    }
}

/// The window's preferences: its appearance, its panels, how a file shows and how its tabs fit.
#[derive(Clone, Debug, PartialEq, JsObject)]
pub struct UiPreferences {
    /// How the window looks: its mode, themes, texture and elevation.
    pub appearance: SavedAppearance,
    /// The sidebar's width, in points.
    pub sidebar_width: f64,
    /// The files panel's width, in points.
    pub files_width: f64,
    /// Whether the files panel shows.
    pub files_open: bool,
    /// Whether the outline shows beside the file.
    pub outline_open: bool,
    /// Whether a map of the whole file shows beside each view of it.
    pub map_open: bool,
    /// How a file opened in a new tab shows; each tab keeps its own after that.
    pub mode: ViewMode,
    /// How the files panel lists the files.
    pub files_view: FilesView,
    /// What the tab strip does with more tabs than fit in one row.
    pub tab_overflow: TabOverflow,
}

/// The window's preferences, saved in `settings.json`.
#[module]
pub struct Preferences;

#[module]
impl Preferences {
    /// The saved preferences.
    pub fn load() -> UiPreferences {
        let ui = settings().current().ui;
        UiPreferences {
            appearance: ui.appearance.into(),
            sidebar_width: f64::from(ui.sidebar_width),
            files_width: f64::from(ui.files_width),
            files_open: ui.files_open,
            outline_open: ui.outline_open,
            map_open: ui.map_open,
            mode: ui.mode.into(),
            files_view: ui.files_view.into(),
            tab_overflow: ui.tab_overflow.into(),
        }
    }

    /// Saves the preferences, off the main thread. A failure is silent, because a preference
    /// must never interrupt what the user is doing.
    pub async fn save(preferences: UiPreferences) {
        let _ = off_main(move || {
            settings().update(|config| {
                let ui = &mut config.ui;
                ui.appearance = preferences.appearance.into();
                ui.sidebar_width = preferences.sidebar_width as f32;
                ui.files_width = preferences.files_width as f32;
                ui.files_open = preferences.files_open;
                ui.outline_open = preferences.outline_open;
                ui.map_open = preferences.map_open;
                ui.mode = preferences.mode.into();
                ui.files_view = preferences.files_view.into();
                ui.tab_overflow = preferences.tab_overflow.into();
            })
        })
        .await;
    }
}

/// Where the window was left, kept in `settings.json` so it reopens there: `keepWindowPlace` (in
/// `dp/ui`) hands it to nsh, and saves each new place the window settles in.
#[module]
pub struct AppWindow;

#[module]
impl AppWindow {
    /// Where the window was left; a width of 0 when nothing was saved yet.
    pub fn place() -> WindowPlace {
        settings().current().window.into()
    }

    /// Saves where the window settled, off the main thread. A failure is silent, as with every
    /// preference.
    pub async fn save_place(place: WindowPlace) {
        let _ = off_main(move || settings().update(|config| config.window = place.into())).await;
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn a_file_comes_with_its_outline() {
        let dir = TempDir::new().expect("a folder");
        fs::write(
            dir.path().join("README.md"),
            "# Title\n\nSome words here.\n\n## Part\n",
        )
        .expect("a file");

        let opened = open_file(dir.path(), "README.md");
        assert_eq!(opened.problem, "");
        assert_eq!(
            opened
                .headings
                .iter()
                .map(|heading| (heading.level, heading.title.as_str(), heading.line))
                .collect::<Vec<_>>(),
            [(1.0, "Title", 0.0), (2.0, "Part", 4.0)]
        );
        assert_eq!(opened.words, 5.0);
    }

    #[test]
    fn a_missing_file_says_why() {
        let dir = TempDir::new().expect("a folder");
        let opened = open_file(dir.path(), "gone.md");
        assert_eq!(opened.text, "");
        assert!(opened.problem.contains("isn't there any more"));
    }

    #[test]
    fn a_listing_names_each_file_and_its_folder() {
        let dir = TempDir::new().expect("a folder");
        fs::create_dir(dir.path().join("docs")).expect("docs");
        fs::write(dir.path().join("docs/setup.md"), "# Setup").expect("a file");
        fs::write(dir.path().join("README.md"), "# Hi").expect("a file");

        let list = file_list(dir.path());
        assert_eq!(list.problem, "");
        let names: Vec<(&str, &str, &str)> = list
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.name.as_str(), file.folder.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("README.md", "README.md", ""),
                ("docs/setup.md", "setup.md", "docs")
            ]
        );
    }

    /// A finder of `text`, as typed.
    fn finder(text: &str) -> Finder {
        let query = mdviewer_core::SearchQuery {
            text: text.to_owned(),
            ..Default::default()
        };
        Finder::new(&query).expect("a query").expect("not empty")
    }

    /// Every file of the project in `dir`.
    fn whole(dir: &TempDir) -> SearchPlace {
        SearchPlace {
            project: dir.path().display().to_string(),
            every_file: true,
            files: Vec::new(),
        }
    }

    #[test]
    fn a_search_finds_again_only_once_a_file_changed() {
        let dir = TempDir::new().expect("a folder");
        fs::write(dir.path().join("README.md"), "# Notes\nThe toolchain.\n").expect("a file");
        let finder = finder("toolchain");
        let running = AtomicBool::new(false);
        let places = [whole(&dir)];

        let (listed, found) = search_again(&places, &finder, None, &running).expect("searched");
        assert_eq!(found.total, 1.0);
        assert_eq!(found.files[0].path, "README.md");
        assert_eq!(found.files[0].project, places[0].project);
        assert_eq!(
            found.files[0].matches[0],
            MatchInfo {
                start_line: 1.0,
                start_column: 4.0,
                end_line: 1.0,
                end_column: 13.0,
                excerpt: "The toolchain.".to_owned(),
                excerpt_start: 4.0,
                excerpt_end: 13.0,
            }
        );
        // The same files are not searched again.
        assert!(search_again(&places, &finder, Some(&listed), &running).is_none());

        fs::write(dir.path().join("README.md"), "toolchain, toolchain\n").expect("changed");
        let (_, again) =
            search_again(&places, &finder, Some(&listed), &running).expect("searched again");
        assert_eq!(again.total, 2.0);
    }

    #[test]
    fn a_search_looks_only_where_it_is_told_in_every_project_given() {
        let one = TempDir::new().expect("a folder");
        let two = TempDir::new().expect("a folder");
        fs::write(one.path().join("a.md"), "tool").expect("a file");
        fs::write(one.path().join("b.md"), "tool tool").expect("a file");
        fs::write(two.path().join("c.md"), "tool").expect("a file");
        let only_b = SearchPlace {
            every_file: false,
            files: vec!["b.md".to_owned()],
            ..whole(&one)
        };
        let places = [only_b, whole(&two)];

        let (_, found) = search_again(&places, &finder("tool"), None, &AtomicBool::new(false))
            .expect("searched");
        let files: Vec<(&str, usize)> = found
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.matches.len()))
            .collect();
        assert_eq!(files, [("b.md", 2), ("c.md", 1)]);
        assert_eq!(found.files[1].project, places[1].project);
        assert_eq!(found.total, 3.0);
    }

    #[test]
    fn a_missing_folder_lists_nothing_and_says_why() {
        let dir = TempDir::new().expect("a folder");
        let list = file_list(&dir.path().join("gone"));
        assert!(list.files.is_empty());
        assert!(list.problem.contains("isn't there any more"));
    }
}
