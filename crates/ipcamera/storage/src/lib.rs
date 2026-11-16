//! Snapshots and recordings on disk: how they are named, written, found, listed and deleted, and how
//! full the drive under their folder is.
//!
//! The desktop app and the server both save what the cameras give them into folders, and both
//! follow these rules, so a folder either one wrote reads the same to the other. Where the folders
//! are is `core`'s `ConfigStore` (`settings.json`'s `storage`); what goes in them is here:
//!
//! - [`saved_file`]: what snapshots and recordings share, a name that says when, which camera and,
//!   for one an event saved, what the camera saw (`snapshot-1762560000-<id>.png`,
//!   `video-1762560000-<id>.vehicle.mp4`), and how a folder of them is listed and watched for
//!   changes.
//! - [`thumbnail`]: the small PNG of each snapshot and recording that the pages show, in a
//!   `thumbnails` folder beside them.
//! - [`snapshot`]: writing a decoded picture, or a camera's own JPEG, as a PNG, the snapshots in a
//!   folder, and making the thumbnails any of them lack.
//! - [`recording`]: naming each recording as it starts, its first picture's thumbnail, and the
//!   recordings in a folder.
//! - [`space`]: how full the drive under a folder is, and whether a folder takes files at all.
//! - [`quota`]: what the snapshots and videos hold together, and deleting the oldest to keep them
//!   under the limit the app's Automation settings set.
//!
//! What an event saves (`core`'s `EventLog`) is a snapshot and recordings like any others, tagged,
//! so there is nothing here of its own for events.
//!
//! Showing a file in the system's file manager is the app's own business, so it isn't here.

mod error;
pub mod quota;
pub mod recording;
pub mod saved_file;
pub mod snapshot;
pub mod space;
pub mod thumbnail;

pub use error::StorageError;
