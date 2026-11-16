//! A named group of cameras, such as "Outside" or "Garage".
//!
//! A group holds camera ids, never camera details. One camera can be in any number of groups, and
//! its settings stay in one place: the cameras the [`Library`](crate::library::Library) keeps.

use serde::{Deserialize, Serialize};

use crate::ids::{CameraId, GroupId};

/// One group: its name and the cameras in it, in the order they are shown.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraGroup {
    /// What the group is found by; the name can change and repeat.
    pub id: GroupId,
    /// The name the sidebar shows.
    pub name: String,
    /// The cameras in the group, by id, in the order the sidebar lists them.
    pub cameras: Vec<CameraId>,
}

impl CameraGroup {
    /// Whether `camera` is in this group.
    pub fn contains(&self, camera: &CameraId) -> bool {
        self.cameras.contains(camera)
    }
}
