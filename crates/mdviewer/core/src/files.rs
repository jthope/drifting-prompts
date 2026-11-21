//! The Markdown files in a project's folder, reading one, and reading a picture a page shows. A file
//! is named by its path in the project with `/` between parts (`docs/setup.md`), and a name never
//! leaves the project's folder.

use std::{
    fmt, fs, io,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

use base64::{Engine, engine::general_purpose::STANDARD};

const EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd", "mkdn", "mdx"];

/// Hidden folders are skipped by their dot.
const SKIPPED: &[&str] = &["node_modules", "target"];

/// The most files a listing holds.
pub const MOST_FILES: usize = 5_000;

/// The largest file read, in bytes: a larger one would hold the window up.
pub const MOST_BYTES: u64 = 8 << 20;

/// The largest picture read, in bytes: the page gets it whole, as a `data:` URL.
pub const MOST_PICTURE_BYTES: u64 = 16 << 20;

/// The pictures a page shows, by extension, and their media types.
const PICTURES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("avif", "image/avif"),
    ("bmp", "image/bmp"),
    ("ico", "image/x-icon"),
    ("svg", "image/svg+xml"),
];

/// A Markdown file in a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownFile {
    /// Its path in the project, with `/` between its parts: `docs/setup.md`.
    pub path: String,
    /// Its size, in bytes.
    pub size: u64,
    /// When it last changed, in Unix milliseconds; 0 when unknown.
    pub modified_ms: u64,
}

impl MarkdownFile {
    /// Its name without its folders.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Its folder in the project, or `""` at the top.
    pub fn folder(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(folder, _)| folder)
    }
}

/// The Markdown files in a project's folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    /// The files, in the order the files panel shows them.
    pub files: Vec<MarkdownFile>,
    /// Whether the walk stopped at [`MOST_FILES`], leaving some out.
    pub truncated: bool,
}

/// What couldn't be read.
#[derive(Debug)]
pub enum FilesError {
    /// The project's folder isn't there.
    MissingFolder(PathBuf),
    /// The file isn't there.
    MissingFile(PathBuf),
    /// The name leaves the project's folder.
    OutsideProject(String),
    /// The file is larger than [`MOST_BYTES`].
    TooLarge {
        /// The file.
        path: PathBuf,
        /// Its size, in bytes.
        size: u64,
    },
    /// The file isn't a picture a page shows.
    NotAPicture(String),
    /// The picture is larger than [`MOST_PICTURE_BYTES`].
    PictureTooLarge {
        /// The picture.
        path: PathBuf,
        /// Its size, in bytes.
        size: u64,
    },
    /// The system refused.
    Unreadable {
        /// The file or folder.
        path: PathBuf,
        /// Why.
        source: io::Error,
    },
}

impl fmt::Display for FilesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFolder(path) => write!(
                f,
                "{} isn't there any more. Remove the project, or put the folder back.",
                path.display()
            ),
            Self::MissingFile(path) => write!(f, "{} isn't there any more.", path.display()),
            Self::OutsideProject(name) => {
                write!(f, "{name} isn't in the project's folder.")
            }
            Self::TooLarge { path, size } => write!(
                f,
                "{} is {} MB, too large to show. Markdown Viewer shows files up to {} MB.",
                path.display(),
                size >> 20,
                MOST_BYTES >> 20
            ),
            Self::NotAPicture(name) => write!(
                f,
                "{name} isn't a picture Markdown Viewer shows: PNG, JPEG, GIF, WebP, AVIF, BMP, ICO or SVG."
            ),
            Self::PictureTooLarge { path, size } => write!(
                f,
                "{} is {} MB, too large to show. Markdown Viewer shows pictures up to {} MB.",
                path.display(),
                size >> 20,
                MOST_PICTURE_BYTES >> 20
            ),
            Self::Unreadable { path, source } => {
                write!(f, "{} couldn't be read: {source}.", path.display())
            }
        }
    }
}

impl std::error::Error for FilesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unreadable { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// The Markdown files in `folder` and under it, each folder's files before its folders.
pub fn list(folder: &Path) -> Result<Listing, FilesError> {
    if !folder.is_dir() {
        return Err(FilesError::MissingFolder(folder.to_path_buf()));
    }
    let mut listing = Listing::default();
    walk(folder, "", &mut listing).map_err(|source| FilesError::Unreadable {
        path: folder.to_path_buf(),
        source,
    })?;
    Ok(listing)
}

/// A folder under the top that can't be read is left out rather than failing the listing.
fn walk(dir: &Path, prefix: &str, listing: &mut Listing) -> io::Result<()> {
    let mut files = Vec::new();
    let mut folders = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        // A link can point back up the tree, so none is followed.
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if !name.starts_with('.') && !SKIPPED.contains(&name.as_str()) {
                folders.push(name);
            }
        } else if kind.is_file() && is_markdown(&name) {
            let metadata = entry.metadata().ok();
            files.push((name, metadata));
        }
    }
    files.sort_by_key(|(name, _)| name.to_lowercase());
    folders.sort_by_key(|name| name.to_lowercase());

    for (name, metadata) in files {
        if listing.files.len() >= MOST_FILES {
            listing.truncated = true;
            return Ok(());
        }
        listing.files.push(MarkdownFile {
            path: join(prefix, &name),
            size: metadata.as_ref().map_or(0, fs::Metadata::len),
            modified_ms: metadata
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |since| since.as_millis() as u64),
        });
    }
    for name in folders {
        if listing.truncated {
            return Ok(());
        }
        let _ = walk(&dir.join(&name), &join(prefix, &name), listing);
    }
    Ok(())
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}/{name}")
    }
}

/// Whether `name` has a Markdown extension, in any case.
pub fn is_markdown(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty()
            && EXTENSIONS
                .iter()
                .any(|each| each.eq_ignore_ascii_case(extension))
    })
}

/// Where file `name` of the project in `folder` is on disk; refuses an absolute name or `..`.
pub fn locate(folder: &Path, name: &str) -> Result<PathBuf, FilesError> {
    let relative = Path::new(name);
    let inside = !name.is_empty()
        && relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    if !inside {
        return Err(FilesError::OutsideProject(name.to_owned()));
    }
    Ok(folder.join(relative))
}

/// The text of file `name` of the project in `folder`, without a byte order mark, lossily UTF-8.
pub fn read(folder: &Path, name: &str) -> Result<String, FilesError> {
    let path = locate(folder, name)?;
    let metadata = fs::metadata(&path).map_err(|source| missing_or(&path, source))?;
    if metadata.len() > MOST_BYTES {
        return Err(FilesError::TooLarge {
            size: metadata.len(),
            path,
        });
    }
    let bytes = fs::read(&path).map_err(|source| missing_or(&path, source))?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

/// Picture `name` of the project in `folder` as a `data:` URL, for the page.
pub fn picture(folder: &Path, name: &str) -> Result<String, FilesError> {
    let path = locate(folder, name)?;
    let kind = name
        .rsplit_once('.')
        .and_then(|(_, extension)| {
            PICTURES
                .iter()
                .find(|(each, _)| each.eq_ignore_ascii_case(extension))
        })
        .map(|(_, kind)| *kind)
        .ok_or_else(|| FilesError::NotAPicture(name.to_owned()))?;
    let metadata = fs::metadata(&path).map_err(|source| missing_or(&path, source))?;
    if metadata.len() > MOST_PICTURE_BYTES {
        return Err(FilesError::PictureTooLarge {
            size: metadata.len(),
            path,
        });
    }
    let bytes = fs::read(&path).map_err(|source| missing_or(&path, source))?;
    Ok(format!("data:{kind};base64,{}", STANDARD.encode(bytes)))
}

/// The size and time of file `name`, to tell whether it changed; `None` when unreadable.
pub fn stamp(folder: &Path, name: &str) -> Option<(u64, u64)> {
    let metadata = fs::metadata(locate(folder, name).ok()?).ok()?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis() as u64);
    Some((metadata.len(), modified))
}

fn missing_or(path: &Path, source: io::Error) -> FilesError {
    if source.kind() == io::ErrorKind::NotFound {
        FilesError::MissingFile(path.to_path_buf())
    } else {
        FilesError::Unreadable {
            path: path.to_path_buf(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn project(files: &[&str]) -> TempDir {
        let dir = TempDir::new().expect("a temporary folder");
        for file in files {
            let path = dir.path().join(file);
            fs::create_dir_all(path.parent().expect("a folder")).expect("its folder");
            fs::write(&path, format!("# {file}\n")).expect("the file");
        }
        dir
    }

    fn paths(listing: &Listing) -> Vec<&str> {
        listing
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect()
    }

    #[test]
    fn files_come_before_folders_each_by_name() {
        let dir = project(&[
            "zeta.md",
            "Alpha.md",
            "docs/setup.md",
            "docs/api/intro.md",
            "docs/README.md",
            "blog/post.markdown",
        ]);
        let listing = list(dir.path()).expect("a listing");
        assert_eq!(
            paths(&listing),
            [
                "Alpha.md",
                "zeta.md",
                "blog/post.markdown",
                "docs/README.md",
                "docs/setup.md",
                "docs/api/intro.md",
            ]
        );
        assert!(!listing.truncated);
    }

    #[test]
    fn only_markdown_is_listed_and_build_folders_are_skipped() {
        let dir = project(&[
            "notes.md",
            "SHOUT.MD",
            "page.mdx",
            "image.png",
            "readme.txt",
            ".md",
            ".git/info.md",
            "node_modules/pkg/README.md",
            "target/doc/out.md",
            ".hidden/secret.md",
        ]);
        let listing = list(dir.path()).expect("a listing");
        let mut found = paths(&listing);
        found.sort_unstable();
        assert_eq!(found, ["SHOUT.MD", "notes.md", "page.mdx"]);
    }

    #[test]
    fn a_file_knows_its_name_and_folder() {
        let dir = project(&["docs/api/intro.md", "top.md"]);
        let listing = list(dir.path()).expect("a listing");
        let intro = &listing.files[1];
        assert_eq!(intro.name(), "intro.md");
        assert_eq!(intro.folder(), "docs/api");
        assert_eq!(listing.files[0].folder(), "");
        assert!(intro.size > 0);
        assert!(intro.modified_ms > 0);
    }

    #[test]
    fn a_missing_folder_says_so() {
        let dir = project(&[]);
        let gone = dir.path().join("gone");
        let error = list(&gone).expect_err("no folder");
        assert!(matches!(error, FilesError::MissingFolder(_)));
        assert!(error.to_string().contains("isn't there any more"));
    }

    #[test]
    fn a_file_is_read_without_its_byte_order_mark() {
        let dir = project(&[]);
        fs::write(dir.path().join("bom.md"), b"\xEF\xBB\xBF# Title\n").expect("a file");
        assert_eq!(read(dir.path(), "bom.md").expect("its text"), "# Title\n");
    }

    #[test]
    fn a_name_that_leaves_the_project_is_refused() {
        let dir = project(&["inside.md"]);
        for name in ["../outside.md", "docs/../../outside.md", "", "/etc/passwd"] {
            assert!(
                matches!(read(dir.path(), name), Err(FilesError::OutsideProject(_))),
                "{name}"
            );
        }
        assert!(read(dir.path(), "inside.md").is_ok());
    }

    #[test]
    fn a_missing_file_says_so() {
        let dir = project(&[]);
        assert!(matches!(
            read(dir.path(), "gone.md"),
            Err(FilesError::MissingFile(_))
        ));
        assert_eq!(stamp(dir.path(), "gone.md"), None);
    }

    #[test]
    fn a_picture_comes_as_a_data_url() {
        let dir = TempDir::new().expect("a temporary folder");
        fs::create_dir(dir.path().join("shots")).expect("a folder");
        fs::write(dir.path().join("shots/dot.PNG"), b"\x89PNG").expect("a picture");

        assert_eq!(
            picture(dir.path(), "shots/dot.PNG").expect("read"),
            "data:image/png;base64,iVBORw=="
        );
        assert!(matches!(
            picture(dir.path(), "../dot.png"),
            Err(FilesError::OutsideProject(_))
        ));
        assert!(matches!(
            picture(dir.path(), "notes.md"),
            Err(FilesError::NotAPicture(_))
        ));
        assert!(matches!(
            picture(dir.path(), "gone.png"),
            Err(FilesError::MissingFile(_))
        ));
    }

    #[test]
    fn a_changed_file_has_a_new_stamp() {
        let dir = project(&["note.md"]);
        let before = stamp(dir.path(), "note.md").expect("a stamp");
        fs::write(dir.path().join("note.md"), "# A longer title now\n").expect("changed");
        assert_ne!(stamp(dir.path(), "note.md"), Some(before));
    }

    #[test]
    fn markdown_is_told_by_its_extension() {
        assert!(is_markdown("README.md"));
        assert!(is_markdown("post.Markdown"));
        assert!(!is_markdown(".md"));
        assert!(!is_markdown("notes.txt"));
        assert!(!is_markdown("md"));
    }
}
