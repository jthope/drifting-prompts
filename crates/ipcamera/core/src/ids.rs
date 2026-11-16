//! The ids that tie groups and views to cameras.
//!
//! Each is a GUID in a type of its own, the way a C# app might wrap `Guid` in `CameraId`,
//! `GroupId` and `ViewId` records: the compiler then refuses a group id where a camera id belongs.
//! In the JSON files they are plain strings, because of `#[serde(transparent)]`.
//!
//! A name is never an id. Renaming a camera, a group or a view changes nothing that refers to it.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::records::RecordId;

/// A GUID's text, random, so it is unlike any other: `Guid.NewGuid()`.
#[cfg(not(target_arch = "wasm32"))]
fn new_guid() -> String {
    Uuid::new_v4().to_string()
}

/// A GUID's text, unlike any other this page has made.
///
/// In a browser, randomness comes only through JavaScript, which the web build has no glue for.
/// The web preview saves nothing, so its ids only need to differ while the page is open, and a
/// count does that.
#[cfg(target_arch = "wasm32")]
fn new_guid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static MADE: AtomicU64 = AtomicU64::new(0);
    let count = MADE.fetch_add(1, Ordering::Relaxed) + 1;
    Uuid::from_u128(u128::from(count)).to_string()
}

/// Writes out one id type, and what a message calls the thing it names. The three are identical
/// apart from their names, and a macro keeps them that way: it is expanded at compile time, like a
/// source generator, so each is an ordinary type.
macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident, $noun:literal) => {
        $(#[$doc])*
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// A new id, unlike any other. The equivalent of `Guid.NewGuid()`.
            pub fn generate() -> Self {
                Self(new_guid())
            }

            /// Whether this is the empty id of something that has not been saved yet.
            pub fn is_empty(&self) -> bool {
                self.0.trim().is_empty()
            }

            /// The id as it is saved: the GUID's text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.to_string())
            }
        }

        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl RecordId for $name {
            const NOUN: &'static str = $noun;

            fn generate() -> Self {
                // The inherent `generate` above, which wins over this trait's own.
                Self::generate()
            }

            fn is_empty(&self) -> bool {
                Self::is_empty(self)
            }
        }
    };
}

id_type!(
    /// A camera's id. The default is empty: a camera that has not been saved yet.
    CameraId,
    "camera"
);

id_type!(
    /// A group's id. The default is empty: a group that has not been saved yet.
    GroupId,
    "group"
);

id_type!(
    /// A view's id. The default is empty: a view that has not been saved yet.
    ViewId,
    "view"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_new_id_is_different() {
        assert_ne!(CameraId::generate(), CameraId::generate());
        assert_ne!(GroupId::generate(), GroupId::generate());
        assert_ne!(ViewId::generate(), ViewId::generate());
        assert!(!CameraId::generate().is_empty());
    }

    #[test]
    fn an_id_is_a_plain_string_in_json() {
        let id = CameraId::from("8f0c");

        assert_eq!(serde_json::to_string(&id).unwrap(), r#""8f0c""#);
        assert_eq!(serde_json::from_str::<CameraId>(r#""8f0c""#).unwrap(), id);
    }

    #[test]
    fn blank_is_empty() {
        assert!(CameraId::default().is_empty());
        assert!(GroupId::from("  ").is_empty());
    }
}
