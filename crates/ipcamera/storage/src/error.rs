//! Why a file on disk could not be found, written or deleted: [`StorageError`].

use std::{fmt, io, path::PathBuf};

/// Why the files on disk could not do what was asked.
#[derive(Debug)]
pub enum StorageError {
    /// The name is not one of that kind's files: a path, or another kind.
    NotThisKind {
        /// The name asked for.
        name: String,
        /// What it was asked for as: "snapshot", "recording", "thumbnail".
        noun: &'static str,
    },
    /// The file is not there, or not yet; the sentence says which.
    Missing(String),
    /// No folder above this one exists.
    NoDrive(PathBuf),
    /// The picture is empty or could not be made a PNG; the sentence says why.
    BadPicture(String),
    /// The disk refused.
    Io {
        /// What was being done, or the path: the sentence's start.
        what: String,
        /// What the system reported.
        source: io::Error,
    },
}

impl StorageError {
    pub(crate) fn io(what: impl Into<String>) -> impl FnOnce(io::Error) -> Self {
        let what = what.into();
        move |source| Self::Io { what, source }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotThisKind { name, noun } => write!(f, "`{name}` is not a {noun}"),
            Self::Missing(sentence) | Self::BadPicture(sentence) => f.write_str(sentence),
            Self::NoDrive(folder) => {
                write!(f, "{} is on no drive this PC has.", folder.display())
            }
            Self::Io { what, source } => write!(f, "{what}: {source}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::NotThisKind { .. }
            | Self::Missing(_)
            | Self::NoDrive(_)
            | Self::BadPicture(_) => None,
        }
    }
}

pub(crate) type Result<T, E = StorageError> = std::result::Result<T, E>;
