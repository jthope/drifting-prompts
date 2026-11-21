//! The camera's own model: the settings files, the cameras, groups and views, and the camera
//! protocols. No UI and no media stack; the stores are private and reached through [`Library`].

pub mod api;
mod camera_config;
#[cfg(not(target_arch = "wasm32"))]
mod camera_detect;
mod camera_event;
mod camera_features;
mod camera_group;
#[cfg(not(target_arch = "wasm32"))]
mod camera_search;
mod camera_store;
mod camera_view;
#[cfg(not(target_arch = "wasm32"))]
mod camera_watch;
mod change;
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod dahua_http;
mod event_log;
mod event_video;
mod group_store;
mod ids;
mod library;
#[cfg(not(target_arch = "wasm32"))]
pub mod onvif;
pub mod ptz;
mod records;
#[cfg(not(target_arch = "wasm32"))]
pub mod reolink;
pub mod rtsp_url;
mod saved_password;
pub use dp_store::sync;
mod view_store;

pub use camera_config::{Brand, CameraConfig, InvalidCamera, Stream};
#[cfg(not(target_arch = "wasm32"))]
pub use camera_detect::{Detected, detect};
pub use camera_event::{
    CameraEvent, EventRules, EventStep, EventTracker, MOTION_TAG, QUIET_FOR_MS, merged_tags,
};
pub use camera_features::{
    CameraActivity, CameraFeatures, CameraStream, Detection, DeviceInfo, FLV_PREFIX, FoundCamera,
    Lens, Spotlight, StreamPath,
};
pub use camera_group::CameraGroup;
#[cfg(not(target_arch = "wasm32"))]
pub use camera_search::find_cameras;
pub use camera_view::{
    ArrangedTile, CameraView, Cell, Fit, GRID, Layout, SMALLEST_SPAN, TileStream, ViewTile,
    check_arrangement,
};
#[cfg(not(target_arch = "wasm32"))]
pub use camera_watch::{CameraWatch, SeenEvent, Watched, watches};
pub use change::{Added, Applied, CameraUpdate, Change};
pub use dp_store::{StoreError, WriteError};
pub use event_log::{EventFile, EventLog, MOST_EVENTS};
pub use event_video::{
    AFTER_SECONDS, BEFORE_SECONDS, EventVideo, LONGEST_MINUTES, MOTION_HOLDS_MS,
};
pub use ids::{CameraId, GroupId, ViewId};
pub use library::{ClientCopy, Library, Snapshot};
