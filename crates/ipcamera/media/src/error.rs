//! Why a session, a pipeline or a still picture failed: [`MediaError`].
//!
//! A rejected login and an unplayable stream are final: a session never retries them.

use std::{fmt, io};

use ipcamera_core::InvalidCamera;

/// Why the media stack could not do what was asked.
#[derive(Debug)]
pub enum MediaError {
    /// The camera's settings lack something a stream needs, such as its address.
    InvalidCamera(InvalidCamera),
    /// The camera refused the login; retrying can lock the camera's account.
    LoginRejected,
    /// The camera sends something this machine cannot play; a retry fails the same way.
    CannotPlay(String),
    /// Anything else, as a sentence; a retry may succeed.
    Failed(String),
}

impl MediaError {
    /// Whether a retry would only repeat it.
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

impl From<InvalidCamera> for MediaError {
    fn from(problem: InvalidCamera) -> Self {
        Self::InvalidCamera(problem)
    }
}

impl From<io::Error> for MediaError {
    fn from(error: io::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

pub(crate) type Result<T, E = MediaError> = std::result::Result<T, E>;

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
