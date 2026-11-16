//! Every change to the cameras, groups and views, as one value: [`Change`], applied by
//! [`Library::apply`].
//!
//! The app's buttons and menus describe what they want as a `Change`, and whoever holds the files
//! applies it. On the desktop that is the app itself. In the web build it is the server: the
//! browser sends the change over HTTP, the server applies the same value to its own library, and
//! the browser takes a fresh copy. So each rule is written once, in [`Library`], and every client
//! is refused in the same words. In C# terms, a `Change` is a command object, and `apply` its one
//! handler.
//!
//! Ids are made where the files are, never by the client: a change that adds something returns
//! the new id in its [`Applied`].
//!
//! A camera is checked here ([`CameraConfig::validate`], in the settings form's words), not in its
//! store: `CameraStore` and `Library::add_camera` and `update_cameras` save what they are given,
//! so a hand-edited `cameras.json` is read as it is and stays diagnosable, never quietly corrected.
//! `apply` is the only way the app and the server change cameras; anything that changes them
//! another way would need the check moved into the store.
//!
//! A client that is never told the cameras' passwords (the web build) sends an update with
//! [`CameraUpdate::keep_password`] set, and the saved password stays.

use serde::{Deserialize, Serialize};

use crate::{
    StoreError,
    camera_config::CameraConfig,
    camera_view::{ArrangedTile, Fit, Layout, TileStream, ViewTile},
    ids::{CameraId, GroupId, ViewId},
    library::Library,
    records::not_found,
};

/// One change to the cameras, groups or views. As JSON, its kind is in `change`:
/// `{ "change": "rename_group", "id": "…", "name": "Outside" }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum Change {
    /// Adds a camera at the end of the list. Its id is made anew.
    AddCamera {
        /// Its settings.
        camera: CameraConfig,
    },
    /// Gives cameras that exist new settings, all in one save.
    UpdateCameras {
        /// Each camera and its new settings.
        cameras: Vec<CameraUpdate>,
    },
    /// Deletes a camera from the list, every group and every view.
    DeleteCamera {
        /// The camera.
        id: CameraId,
    },
    /// Moves a camera among the cameras: the order All cameras shows. Groups keep their own.
    MoveCamera {
        /// The camera.
        id: CameraId,
        /// Where it goes, from 0; past the end is the end.
        index: usize,
    },
    /// Adds a group at the end, with these cameras in it.
    AddGroup {
        /// Its name.
        name: String,
        /// Its cameras, in order.
        cameras: Vec<CameraId>,
    },
    /// Renames a group.
    RenameGroup {
        /// The group.
        id: GroupId,
        /// Its new name.
        name: String,
    },
    /// Gives a group a name and cameras, in one save: what its sheet saves.
    SaveGroup {
        /// The group.
        id: GroupId,
        /// Its name.
        name: String,
        /// Its cameras, in order.
        cameras: Vec<CameraId>,
    },
    /// Removes groups. Their cameras stay.
    RemoveGroups {
        /// The groups.
        ids: Vec<GroupId>,
    },
    /// Moves a group among the groups.
    MoveGroup {
        /// The group.
        id: GroupId,
        /// Where it goes, from 0; past the end is the end.
        index: usize,
    },
    /// Puts a camera at the end of a group.
    AddToGroup {
        /// The group.
        group: GroupId,
        /// The camera.
        camera: CameraId,
    },
    /// Takes a camera out of one group.
    RemoveFromGroup {
        /// The group.
        group: GroupId,
        /// The camera.
        camera: CameraId,
    },
    /// Adds a view with empty tiles, at the end.
    AddView {
        /// Its name.
        name: String,
        /// How it divides the stage.
        layout: Layout,
    },
    /// Renames a view.
    RenameView {
        /// The view.
        id: ViewId,
        /// Its new name.
        name: String,
    },
    /// Removes a view. Its cameras are not touched.
    RemoveView {
        /// The view.
        id: ViewId,
    },
    /// Divides a view as one of the layouts, each tile keeping its camera.
    SetLayout {
        /// The view.
        id: ViewId,
        /// Its new layout.
        layout: Layout,
    },
    /// Moves, resizes, adds and removes a view's tiles at once, each keeping the camera of the
    /// tile it was.
    ArrangeTiles {
        /// The view.
        id: ViewId,
        /// Every tile it has from now on, in order.
        tiles: Vec<ArrangedTile>,
    },
    /// Sets how a view's pictures fill the tiles that pick no fit of their own.
    SetFit {
        /// The view.
        id: ViewId,
        /// The new fit.
        fit: Fit,
    },
    /// Sets how one tile's picture fills it. The tile keeps it as its camera or stream changes.
    SetTileFit {
        /// The view.
        id: ViewId,
        /// The tile, from 0. It must show a camera.
        index: usize,
        /// The new fit.
        fit: Fit,
    },
    /// Shows a camera in a tile, on one of its streams, or empties it.
    SetTile {
        /// The view.
        id: ViewId,
        /// The tile, from 0.
        index: usize,
        /// The camera, or `None` for an empty tile.
        camera: Option<CameraId>,
        /// The stream it picks; `None`, and left out of the JSON, for Auto, which plays the one
        /// the size the tile is drawn at calls for.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stream: Option<TileStream>,
    },
}

/// One camera's new settings in [`Change::UpdateCameras`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraUpdate {
    /// The camera. It keeps this id whatever `camera` says.
    pub id: CameraId,
    /// Its new settings.
    pub camera: CameraConfig,
    /// Keeps the saved password, whatever `camera` says: what a client sends when it was never
    /// told the password and the user left it alone.
    #[serde(default)]
    pub keep_password: bool,
}

/// What a change did that its sender needs to know.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applied {
    /// What it added, by id; `None` when it added nothing.
    pub added: Option<Added>,
}

/// What a change added, by the id made for it. As JSON, `{ "camera": "…" }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Added {
    /// A camera.
    Camera(CameraId),
    /// A group.
    Group(GroupId),
    /// A view.
    View(ViewId),
}

impl Added {
    /// The new id's text, whichever kind it is.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Camera(id) => id.as_str(),
            Self::Group(id) => id.as_str(),
            Self::View(id) => id.as_str(),
        }
    }
}

impl Applied {
    fn added(added: Added) -> Self {
        Self { added: Some(added) }
    }

    /// The text of the id it added, or `""` when it added nothing: what the app's TSX is told.
    pub fn added_id(&self) -> String {
        self.added
            .as_ref()
            .map(|added| added.as_str().to_string())
            .unwrap_or_default()
    }
}

impl Library {
    /// Makes `change`, with every rule the library keeps, and says what it added. A refused
    /// change changes nothing, and its error is a sentence for the user.
    pub fn apply(&self, change: &Change) -> Result<Applied, StoreError> {
        let nothing_added = |done: Result<(), StoreError>| done.map(|()| Applied::default());
        match change {
            Change::AddCamera { camera } => {
                saveable(camera)?;
                self.add_camera(camera.clone(), None)
                    .map(|id| Applied::added(Added::Camera(id)))
            }
            Change::UpdateCameras { cameras } => {
                let mut settings = Vec::with_capacity(cameras.len());
                for update in cameras {
                    let mut camera = update.camera.clone();
                    if update.keep_password {
                        camera.password = self
                            .camera(&update.id)
                            .ok_or_else(|| not_found(&update.id))?
                            .password;
                    }
                    saveable(&camera)?;
                    settings.push((update.id.clone(), camera));
                }
                nothing_added(self.update_cameras(settings))
            }
            Change::DeleteCamera { id } => nothing_added(self.delete_camera(id)),
            Change::MoveCamera { id, index } => nothing_added(self.move_camera(id, *index)),
            Change::AddGroup { name, cameras } => self
                .add_group(name, cameras.clone())
                .map(|id| Applied::added(Added::Group(id))),
            Change::RenameGroup { id, name } => nothing_added(self.rename_group(id, name)),
            Change::SaveGroup { id, name, cameras } => {
                nothing_added(self.update_group(id, name, cameras.clone()))
            }
            Change::RemoveGroups { ids } => nothing_added(self.remove_groups(ids)),
            Change::MoveGroup { id, index } => nothing_added(self.move_group(id, *index)),
            Change::AddToGroup { group, camera } => nothing_added(self.add_to_group(group, camera)),
            Change::RemoveFromGroup { group, camera } => {
                nothing_added(self.remove_from_group(group, camera))
            }
            Change::AddView { name, layout } => self
                .add_view(name, *layout)
                .map(|id| Applied::added(Added::View(id))),
            Change::RenameView { id, name } => nothing_added(self.rename_view(id, name)),
            Change::RemoveView { id } => nothing_added(self.remove_view(id)),
            Change::SetLayout { id, layout } => nothing_added(self.set_layout(id, *layout)),
            Change::ArrangeTiles { id, tiles } => nothing_added(self.arrange_tiles(id, tiles)),
            Change::SetFit { id, fit } => nothing_added(self.set_fit(id, *fit)),
            Change::SetTileFit { id, index, fit } => {
                nothing_added(self.set_tile_fit(id, *index, *fit))
            }
            Change::SetTile {
                id,
                index,
                camera,
                stream,
            } => {
                // No fit: the tile keeps the one it had.
                let tile = camera.clone().map(|camera| ViewTile {
                    camera,
                    stream: stream.clone(),
                    fit: None,
                });
                nothing_added(self.show_in_tile(id, *index, tile.as_ref()))
            }
        }
    }
}

/// Refuses a camera that can't be saved, in the settings form's words.
fn saveable(camera: &CameraConfig) -> Result<(), StoreError> {
    camera
        .validate()
        .map_err(|problem| StoreError::Invalid(problem.to_string()))
}

#[cfg(test)]
mod tests;
