//! Snapshots and recordings on disk, by one set of rules for the app and the server: their names,
//! writing and listing them, thumbnails, the storage limit and a drive's free space.

mod error;
pub mod quota;
pub mod recording;
pub mod saved_file;
pub mod snapshot;
pub mod space;
pub mod thumbnail;

pub use error::StorageError;
