//! The camera's own model: what the settings files say, what the live camera connection needs,
//! and the protocols in front of it.
//!
//! Nothing here draws or plays anything. That is deliberate: this crate has no UI and no media
//! stack, so it compiles anywhere, and `cargo test -p ipcamera-core` runs in milliseconds without
//! a display or a decoder.
//!
//! The cameras, their groups and the views are independent stores, each with its own file in
//! `data\settings`: `camera_store` keeps every camera once, while `group_store` and `view_store`
//! only refer to cameras by id. The stores are private: the app reaches all three through
//! [`Library`], so how they are saved can change without the app knowing.
//!
//! The types everything is built from are at the crate's root, whichever file holds them:
//! `ipcamera_core::CameraConfig`, `ipcamera_core::Library`. In C# terms, they share the root
//! namespace, and a `pub mod` below is a namespace of its own, for a part with a subject of its
//! own: [`api`], [`config`], `dahua_http`, `onvif`, [`ptz`], `reolink`, [`rtsp_url`] and [`sync`].
//!
//! What a camera does beyond its stream (motion, AI detections, a spotlight) is in brand-neutral
//! types here ([`CameraFeatures`], [`CameraActivity`], [`Spotlight`]), and each brand's protocol
//! fills them in: `reolink`; `dahua_http` for a Dahua's light, Dahua's HTTP API; or `onvif` for a
//! camera that has it turned on. `CameraWatch` polls a camera for them on a thread of its own,
//! `detect` tells the camera form which brand a camera is, and `find_cameras` which cameras are on
//! the network: none of the three is in the web build, whose cameras are the server's, nor are the
//! protocols. What a watch sees becomes [`CameraEvent`]s ([`EventTracker`]), which an
//! [`EventLog`] keeps in `data\events`, and [`EventVideo`] says how the videos an event saves are
//! made.

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
// HTTPS is out of the web build's reach: a browser makes those requests, not the program.
#[cfg(not(target_arch = "wasm32"))]
pub mod reolink;
pub mod rtsp_url;
mod saved_password;
// The suite's lock helper, still reached here as `ipcamera_core::sync::LockExt`.
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
