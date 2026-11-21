//! A named group of cameras, which holds camera ids, never camera details.

use serde::{Deserialize, Serialize};

use crate::ids::{CameraId, GroupId};

/// One group: its name and its cameras, in order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraGroup {
    /// What the group is found by; the name can change and repeat.
    pub id: GroupId,
    /// The group's name.
    pub name: String,
    /// The cameras in the group, by id, in order.
    pub cameras: Vec<CameraId>,
}

impl CameraGroup {
    /// Whether `camera` is in this group.
    pub fn contains(&self, camera: &CameraId) -> bool {
        self.cameras.contains(camera)
    }
}
