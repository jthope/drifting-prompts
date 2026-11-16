//! Why a session, a pipeline or a still picture failed: [`MediaError`].
//!
//! Most failures clear up by themselves, and a session reconnects after them. Two never do, and
//! are kept apart so the compiler makes every caller tell them apart: a rejected login, where a
//! retry does harm, and a stream this machine cannot play. In C# terms, one exception type per
//! case, as an enum: the caller matches on it rather than asking an exception what type it is.

use std::{fmt, io};

use ipcamera_core::InvalidCamera;

/// Why the media stack could not do what was asked.
#[derive(Debug)]
pub enum MediaError {
    /// The camera's settings lack something a stream needs, such as its address.
    InvalidCamera(InvalidCamera),
    /// The camera refused the username or password.
    ///
    /// This is kept apart from every other failure because it is the one a retry cannot fix, and
    /// the one where retrying does harm: repeated failed logins can lock the camera's account.
    LoginRejected,
    /// The camera sends something this machine cannot play: video with no decoder here, or a
    /// format the pipeline cannot agree on. A retry fails the same way, so the session stops at
    /// once and shows why, instead of counting reconnect attempts with the reason out of sight.
    CannotPlay(String),
    /// Anything else, as a sentence: a camera that did not answer, a stream that broke off. A
    /// retry may well succeed.
    Failed(String),
}

impl MediaError {
    /// Whether a retry would only repeat it: the camera rejected the login, or sends a stream this
    /// machine can't play.
    pub fn is_final(&self) -> bool {
        matches!(self, Self::LoginRejected | Self::CannotPlay(_))
    }
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCamera(problem) => write!(f, "{problem}"),
            Self::LoginRejected => f.write_str("the camera rejected the username or password"),
            Self::CannotPlay(reason) | Self::Failed(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for MediaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidCamera(problem) => Some(problem),
            Self::LoginRejected | Self::CannotPlay(_) | Self::Failed(_) => None,
        }
    }
}

/// Lets `?` turn a refused camera into a [`MediaError`].
impl From<InvalidCamera> for MediaError {
    fn from(problem: InvalidCamera) -> Self {
        Self::InvalidCamera(problem)
    }
}

/// Lets `?` turn a thread that could not start into a [`MediaError`].
impl From<io::Error> for MediaError {
    fn from(error: io::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

/// What the media stack's own functions return.
pub(crate) type Result<T, E = MediaError> = std::result::Result<T, E>;

/// A [`MediaError::Failed`] from a format string, as `format!` takes it.
macro_rules! failed {
    ($($argument:tt)*) => {
        $crate::error::MediaError::Failed(format!($($argument)*))
    };
}
pub(crate) use failed;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_rejected_login_and_an_unplayable_stream_are_final() {
        assert!(MediaError::LoginRejected.is_final());
        assert!(MediaError::CannotPlay("no H.264".into()).is_final());
        assert!(!failed!("the camera did not answer within {} s", 10).is_final());
        assert!(!MediaError::from(InvalidCamera::NoAddress).is_final());
    }

    #[test]
    fn an_error_reads_as_its_sentence() {
        assert_eq!(
            failed!("port {}", 554).to_string(),
            "port 554",
            "a failure is its own sentence"
        );
        assert_eq!(
            MediaError::from(InvalidCamera::NoAddress).to_string(),
            InvalidCamera::NoAddress.message()
        );
    }
}
