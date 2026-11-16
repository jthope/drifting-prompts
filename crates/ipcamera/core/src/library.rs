//! The cameras, their groups and the views together: the only way the app reaches them.
//!
//! Each is kept by a store of its own, private to this crate: `CameraStore`, `GroupStore` and
//! `ViewStore`, each saving its own file in `data\settings`. The app never names them. Every read
//! and change goes through [`Library`], so how they are saved can change without the app knowing.
//! In C# terms, `Library` is a facade over three repositories.
//!
//! The stores know nothing of each other. A few things need more than one, and they live here:
//!
//! - deleting a camera, which also takes it out of every group and every view;
//! - putting cameras in a group or a view's tile, which checks the cameras exist;
//! - the groups as the sidebar shows them, listing only cameras that exist;
//! - the first view, made when there is none, which shows the first cameras.
//!
//! The files can disagree: a crash between the writes of a delete, or a hand edit, can leave a
//! group or a tile with a camera that is gone. `cameras.json` is what counts. An id for a missing
//! camera is skipped by [`Library::tree`], [`Library::shown_tiles`] and [`Library::hidden_tiles`],
//! and cleared out the next
//! time the library opens, so nothing worse than a stale id can happen.

use std::path::PathBuf;

use dp_store::Stored;
use serde::{Deserialize, Serialize};

use crate::{
    StoreError,
    camera_config::CameraConfig,
    camera_group::CameraGroup,
    camera_store::CameraStore,
    camera_view::{ArrangedTile, CameraView, Fit, Layout, ViewTile},
    config::ConfigStore,
    group_store::GroupStore,
    ids::{CameraId, GroupId, ViewId},
    records::not_found,
    view_store::ViewStore,
};

/// What the first launch calls the view it makes.
const FIRST_VIEW: &str = "Default";

/// The cameras, the groups and the views, whole: what the server sends a client that keeps a copy
/// ([`Library::snapshot`], [`Library::from_snapshot`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    /// The library's revision when it was taken. A client that holds an older one is out of date.
    pub revision: u64,
    /// Every camera, in order.
    pub cameras: Vec<CameraConfig>,
    /// Every group as saved, in order, cameras that are gone included.
    pub groups: Vec<CameraGroup>,
    /// Every view, in order.
    pub views: Vec<CameraView>,
}

impl Snapshot {
    /// This snapshot with every camera's password taken out, and which cameras had one: what the
    /// server sends a browser, which is never told a password.
    pub fn for_client(mut self) -> ClientCopy {
        let mut with_password = Vec::new();
        for camera in &mut self.cameras {
            if !camera.password.is_empty() {
                with_password.push(camera.id.clone());
                camera.password.clear();
            }
        }
        ClientCopy {
            snapshot: self,
            with_password,
        }
    }
}

/// A [`Snapshot`] with no passwords in it, as a browser gets it ([`Snapshot::for_client`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientCopy {
    /// The snapshot, with every camera's password empty.
    pub snapshot: Snapshot,
    /// The cameras that have a password saved.
    pub with_password: Vec<CameraId>,
}

/// The cameras, the groups and the views, each in its own store.
pub struct Library {
    cameras: CameraStore,
    groups: GroupStore,
    views: ViewStore,
    /// What went wrong while opening, beyond what each store reports itself.
    problems: Vec<String>,
}

impl Library {
    /// Opens `cameras.json`, `groups.json` and `views.json` in `dir`.
    ///
    /// A first launch starts with no cameras: an empty `cameras.json` and an empty `groups.json`
    /// are created, ready to edit. When there is no `views.json`, a 2 × 2 view called "Default"
    /// shows the first cameras, and is empty when there are none.
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let mut library = Self {
            cameras: CameraStore::open(dir.join(CameraStore::FILE)),
            groups: GroupStore::open(dir.join(GroupStore::FILE)),
            views: ViewStore::open(dir.join(ViewStore::FILE)),
            problems: Vec::new(),
        };

        // Only a convenience, for both: the first camera added, or the first change to a group,
        // writes the file anyway.
        if library.cameras.was_missing() {
            let _ = library.cameras.create_file();
        }
        if library.groups.was_missing() {
            let _ = library.groups.create_file();
        }
        if library.views.was_missing() {
            library.add_first_view();
        }

        // Groups and views may list cameras deleted since, or never there after a hand edit. Only
        // cleared out when `cameras.json` was there and read as it is: after a broken one every id
        // would look unknown, and the groups would lose cameras the user can still bring back from
        // the moved-aside file. `tree` and `shown_tiles` skip unknown ids either way.
        if !library.cameras.was_missing() && library.cameras.problem().is_none() {
            let known = library.cameras.ids();
            let _ = library.groups.prune(&known);
            let _ = library.views.prune(&known);
        }
        library
    }

    /// A library with no files: `cameras`, no groups, and the first view showing them, as a first
    /// launch has. Its changes are kept in memory only, and gone when the app closes. It is the
    /// web preview's, which has no disk to save to, and keeps every rule `open`'s does.
    pub fn in_memory(cameras: Vec<CameraConfig>) -> Self {
        let mut library = Self {
            cameras: CameraStore::in_memory(),
            groups: GroupStore::in_memory(),
            views: ViewStore::in_memory(),
            problems: Vec::new(),
        };
        for camera in cameras {
            let _ = library.cameras.add(camera);
        }
        library.add_first_view();
        library
    }

    /// The cameras, groups and views as they are now, whole, for a client that keeps a copy.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            revision: self.revision(),
            cameras: self.cameras(),
            groups: self.groups.list(),
            views: self.views(),
        }
    }

    /// A copy of `snapshot` with no files: the same cameras, groups and views, ids and all, with
    /// every rule `open`'s library keeps. Its changes are kept in memory only. It is how the web
    /// build holds the server's library: reads come from the copy at once, and changes go to the
    /// server, which sends a new snapshot back. Unlike [`Library::in_memory`], no first view is
    /// made: the snapshot's views are the ones there are.
    pub fn from_snapshot(snapshot: Snapshot) -> Self {
        Self {
            cameras: CameraStore::holding(snapshot.cameras),
            groups: GroupStore::holding(snapshot.groups),
            views: ViewStore::holding(snapshot.views),
            problems: Vec::new(),
        }
    }

    /// Saves the first view: 2 × 2, called "Default", showing the first cameras.
    fn add_first_view(&mut self) {
        let layout = Layout::Four;
        let tiles = self
            .cameras
            .list()
            .into_iter()
            .take(layout.tile_count())
            .map(|camera| Some(ViewTile::from(camera.id)))
            .collect();
        if let Err(error) = self.views.add_with(FIRST_VIEW, layout, tiles) {
            self.problems
                .push(format!("The first view could not be saved: {error}"));
        }
    }

    /// The library beside `settings`: its `data\settings`.
    pub fn beside(config: &ConfigStore) -> Self {
        Self::open(config.settings_dir())
    }

    /// Bumped by every saved change to the cameras, the groups or the views, so the app can tell
    /// when to look again.
    pub fn revision(&self) -> u64 {
        self.cameras.revision() + self.groups.revision() + self.views.revision()
    }

    /// Takes what another program has saved in the files since this library last read or wrote
    /// them, and returns whether anything changed; [`revision`](Self::revision) has moved on when
    /// it did. The desktop app and the server can share the files: each calls this every few
    /// seconds to learn what the other did. A change made here never needs it, since every change
    /// starts from the file as it is. It reads files, so it stays off a window's thread.
    pub fn refresh(&self) -> bool {
        let cameras = self.cameras.refresh();
        let groups = self.groups.refresh();
        let views = self.views.refresh();
        cameras || groups || views
    }

    // The cameras.

    /// Every camera, entered once, in the order they are shown.
    pub fn cameras(&self) -> Vec<CameraConfig> {
        self.cameras.list()
    }

    /// Camera `id`, if there is one.
    pub fn camera(&self, id: &CameraId) -> Option<CameraConfig> {
        self.cameras.get(id)
    }

    /// Replaces several cameras' settings in one save. A camera's id never changes, whatever its
    /// settings say, and an unknown id changes none of them.
    pub fn update_cameras(&self, cameras: Vec<(CameraId, CameraConfig)>) -> Result<(), StoreError> {
        self.cameras.update_many(cameras)
    }

    /// Moves camera `id` to `index` among the cameras, which is the order [`cameras`](Self::cameras)
    /// lists them in and the sidebar's All cameras shows. An index past the end moves it to the
    /// end. Groups keep their own order.
    pub fn move_camera(&self, id: &CameraId, index: usize) -> Result<(), StoreError> {
        self.cameras.move_camera(id, index)
    }

    // The groups. `tree` lists them.

    /// Adds a group at the end with `cameras` in it, in that order, in one save, and returns its
    /// id. Rejects a blank name, or a camera that doesn't exist, adding nothing.
    pub fn add_group(&self, name: &str, cameras: Vec<CameraId>) -> Result<GroupId, StoreError> {
        self.check_cameras(&cameras)?;
        self.groups.add_with(name, cameras)
    }

    /// Renames group `id`. Rejects a blank name.
    pub fn rename_group(&self, id: &GroupId, name: &str) -> Result<(), StoreError> {
        self.groups.rename(id, name)
    }

    /// Gives group `id` a name and `cameras`, in that order, in one save. Rejects a blank name, or
    /// a camera that doesn't exist, changing nothing.
    pub fn update_group(
        &self,
        id: &GroupId,
        name: &str,
        cameras: Vec<CameraId>,
    ) -> Result<(), StoreError> {
        self.check_cameras(&cameras)?;
        self.groups.update(id, name, cameras)
    }

    /// Removes every group in `ids` in one save. Their cameras stay, in the list and in every other
    /// group. An unknown group removes none.
    pub fn remove_groups(&self, ids: &[GroupId]) -> Result<(), StoreError> {
        self.groups.remove_many(ids)
    }

    /// Moves group `id` to `index` among the groups, as the sidebar lists them. An index past the
    /// end moves it to the end.
    pub fn move_group(&self, id: &GroupId, index: usize) -> Result<(), StoreError> {
        self.groups.move_group(id, index)
    }

    /// Takes camera `camera` out of group `group` only.
    pub fn remove_from_group(&self, group: &GroupId, camera: &CameraId) -> Result<(), StoreError> {
        self.groups.remove_camera(group, camera)
    }

    /// The ids of the groups camera `camera` is in.
    pub fn groups_of(&self, camera: &CameraId) -> Vec<GroupId> {
        self.groups.groups_of(camera)
    }

    // The views. `shown_tiles` says what each tile shows.

    /// Every view, in the order they are shown.
    pub fn views(&self) -> Vec<CameraView> {
        self.views.list()
    }

    /// Adds a view with empty tiles at the end and returns its id. Rejects a blank name.
    pub fn add_view(&self, name: &str, layout: Layout) -> Result<ViewId, StoreError> {
        self.views.add(name, layout)
    }

    /// Renames view `id`. Rejects a blank name.
    pub fn rename_view(&self, id: &ViewId, name: &str) -> Result<(), StoreError> {
        self.views.rename(id, name)
    }

    /// Removes view `id`. Its cameras are not touched.
    pub fn remove_view(&self, id: &ViewId) -> Result<(), StoreError> {
        self.views.remove(id)
    }

    /// Divides view `id` as `layout`, each tile keeping its camera. Tiles the layout has no room
    /// for keep their cameras, for when the view has room for them again.
    pub fn set_layout(&self, id: &ViewId, layout: Layout) -> Result<(), StoreError> {
        self.views.set_layout(id, layout)
    }

    /// Moves, resizes, adds and removes view `id`'s tiles at once
    /// ([`ViewStore::arrange`](crate::view_store::ViewStore::arrange)).
    pub fn arrange_tiles(&self, id: &ViewId, tiles: &[ArrangedTile]) -> Result<(), StoreError> {
        self.views.arrange(id, tiles)
    }

    /// Sets how view `id`'s pictures fill the tiles that pick no fit of their own.
    pub fn set_fit(&self, id: &ViewId, fit: Fit) -> Result<(), StoreError> {
        self.views.set_fit(id, fit)
    }

    /// Sets how the picture fills tile `index` of view `id`, which must show a camera
    /// ([`ViewStore::set_tile_fit`](crate::view_store::ViewStore::set_tile_fit)).
    pub fn set_tile_fit(&self, id: &ViewId, index: usize, fit: Fit) -> Result<(), StoreError> {
        self.views.set_tile_fit(id, index, fit)
    }

    // What needs more than one store.

    /// Adds a camera to the list and, when `group` is given, to that group as well.
    ///
    /// The group is checked first, so an unknown one adds nothing. If putting the camera in the
    /// group then fails, the camera is still saved, under All cameras, and the error says why it
    /// is not in the group.
    pub fn add_camera(
        &self,
        camera: CameraConfig,
        group: Option<&GroupId>,
    ) -> Result<CameraId, StoreError> {
        if let Some(group) = group
            && self.groups.get(group).is_none()
        {
            return Err(not_found(group));
        }

        let id = self.cameras.add(camera)?;
        if let Some(group) = group {
            self.groups.add_camera(group, &id)?;
        }
        Ok(id)
    }

    /// Puts camera `camera`, which must exist, at the end of group `group`.
    pub fn add_to_group(&self, group: &GroupId, camera: &CameraId) -> Result<(), StoreError> {
        self.check_cameras(std::slice::from_ref(camera))?;
        self.groups.add_camera(group, camera)
    }

    /// Says which of `cameras` doesn't exist, if one doesn't: a group lists only cameras that do.
    fn check_cameras(&self, cameras: &[CameraId]) -> Result<(), StoreError> {
        match cameras.iter().find(|camera| !self.cameras.contains(camera)) {
            Some(camera) => Err(not_found(camera)),
            None => Ok(()),
        }
    }

    /// Shows `tile` in tile `index` of view `view`: a camera, which must exist, and which of its
    /// streams. `None` empties it.
    pub fn show_in_tile(
        &self,
        view: &ViewId,
        index: usize,
        tile: Option<&ViewTile>,
    ) -> Result<(), StoreError> {
        if let Some(tile) = tile
            && !self.cameras.contains(&tile.camera)
        {
            return Err(not_found(&tile.camera));
        }
        self.views.set_tile(view, index, tile.cloned())
    }

    /// Deletes a camera everywhere: from the camera list, then from every group and every view.
    ///
    /// A delete that stopped half way, with the camera gone but still listed in a group or a tile,
    /// is finished by calling this again.
    pub fn delete_camera(&self, id: &CameraId) -> Result<(), StoreError> {
        match self.cameras.remove(id) {
            Ok(()) | Err(StoreError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
        self.groups.forget_camera(id)?;
        self.views.forget_camera(id)
    }

    /// What each tile view `view` shows holds, leaving out cameras that are gone.
    pub fn shown_tiles(&self, view: &ViewId) -> Option<Vec<Option<ViewTile>>> {
        Some(self.known_only(self.views.get(view)?.shown_tiles()))
    }

    /// What each tile of view `view` past its cells holds, kept for when the view has room for
    /// them again, leaving out cameras that are gone.
    pub fn hidden_tiles(&self, view: &ViewId) -> Option<Vec<Option<ViewTile>>> {
        Some(self.known_only(self.views.get(view)?.hidden_tiles()))
    }

    /// `tiles` with the cameras that are gone taken out.
    fn known_only(&self, mut tiles: Vec<Option<ViewTile>>) -> Vec<Option<ViewTile>> {
        let known = self.cameras.ids();
        for tile in &mut tiles {
            if tile
                .as_ref()
                .is_some_and(|tile| !known.contains(&tile.camera))
            {
                *tile = None;
            }
        }
        tiles
    }

    /// The groups in order, as the sidebar shows them: each lists only cameras that exist.
    pub fn tree(&self) -> Vec<CameraGroup> {
        let known = self.cameras.ids();
        let mut groups = self.groups.list();
        for group in &mut groups {
            group.cameras.retain(|camera| known.contains(camera));
        }
        groups
    }

    /// Everything that went wrong opening the library, for the app to tell the user once.
    pub fn problems(&self) -> Vec<String> {
        self.cameras
            .problem()
            .into_iter()
            .chain(self.groups.problem())
            .chain(self.views.problem())
            .map(str::to_string)
            .chain(self.problems.iter().cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::camera_view::TileStream;
    use tempfile::TempDir;

    fn named(name: &str) -> CameraConfig {
        CameraConfig {
            name: name.into(),
            ..CameraConfig::default()
        }
    }

    fn open(folder: &TempDir) -> Library {
        Library::open(folder.path())
    }

    #[test]
    fn a_library_with_no_files_keeps_the_same_rules_in_memory() {
        let library = Library::in_memory(vec![named("Porch")]);

        let porch = library.cameras()[0].id.clone();
        let view = library.views()[0].id.clone();
        assert_eq!(library.views()[0].name, "Default");
        assert_eq!(
            library.shown_tiles(&view).unwrap()[0],
            Some(porch.clone().into()),
            "the first view shows the first camera"
        );

        let before = library.revision();
        let group = library.add_group("Outside", vec![porch.clone()]).unwrap();
        assert!(library.revision() > before, "a change is heard as on disk");
        assert_eq!(
            library
                .add_group("Ghosts", vec![CameraId::from("gone")])
                .unwrap_err()
                .to_string(),
            "no camera `gone`"
        );
        library.delete_camera(&porch).unwrap();
        assert!(library.tree()[0].cameras.is_empty());
        assert_eq!(library.tree()[0].id, group);
        assert!(library.problems().is_empty());
    }

    #[test]
    fn a_first_launch_starts_with_no_cameras_and_an_empty_default_view() {
        let folder = TempDir::new().unwrap();

        let library = open(&folder);

        assert!(library.cameras().is_empty());
        assert!(
            folder.path().join(CameraStore::FILE).is_file(),
            "an empty cameras.json, ready to edit"
        );
        assert!(library.tree().is_empty(), "no group is made");
        assert!(
            folder.path().join(GroupStore::FILE).is_file(),
            "an empty groups.json, ready to edit"
        );

        let views = library.views();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].name, "Default");
        assert_eq!(views[0].layout(), Some(Layout::Four));
        assert_eq!(views[0].shown_tiles(), vec![None; 4]);
        assert!(library.problems().is_empty());
    }

    #[test]
    fn a_missing_views_file_gets_a_default_view_of_the_first_cameras() {
        let folder = TempDir::new().unwrap();
        let store = CameraStore::open(folder.path().join(CameraStore::FILE));
        let ids: Vec<_> = ["A", "B", "C", "D", "E"]
            .into_iter()
            .map(|name| store.add(named(name)).unwrap())
            .collect();
        drop(store);

        let library = open(&folder);

        let views = library.views();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].name, "Default");
        assert_eq!(
            views[0].shown_tiles(),
            ids[..4]
                .iter()
                .cloned()
                .map(|id| Some(ViewTile::from(id)))
                .collect::<Vec<_>>(),
            "as many as the 2 × 2 holds, in order"
        );
    }

    #[test]
    fn a_camera_file_with_no_cameras_gets_an_empty_default_view() {
        let folder = TempDir::new().unwrap();
        fs::write(
            folder.path().join(CameraStore::FILE),
            r#"{ "version": 1, "cameras": [] }"#,
        )
        .unwrap();

        let library = open(&folder);

        assert!(library.cameras().is_empty());
        assert_eq!(library.views()[0].shown_tiles(), vec![None; 4]);
        assert!(library.problems().is_empty());
    }

    #[test]
    fn a_camera_goes_only_in_a_tile_if_it_exists() {
        let folder = TempDir::new().unwrap();
        let library = open(&folder);
        let view = library.views()[0].id.clone();
        let porch = library.add_camera(named("Porch"), None).unwrap();

        let porch = ViewTile {
            camera: porch,
            stream: TileStream::from_key("sub"),
            fit: None,
        };
        library.show_in_tile(&view, 1, Some(&porch)).unwrap();
        assert_eq!(library.shown_tiles(&view).unwrap()[1], Some(porch.clone()));

        let error = library
            .show_in_tile(&view, 2, Some(&CameraId::from("ghost").into()))
            .unwrap_err();
        assert_eq!(error.to_string(), "no camera `ghost`");

        library.show_in_tile(&view, 1, None).unwrap();
        assert_eq!(library.shown_tiles(&view).unwrap()[1], None);

        library.show_in_tile(&view, 3, Some(&porch)).unwrap();
        library.set_layout(&view, Layout::Two).unwrap();
        assert_eq!(library.hidden_tiles(&view).unwrap(), [None, Some(porch)]);
    }

    #[test]
    fn deleting_a_camera_empties_its_tiles() {
        let folder = TempDir::new().unwrap();
        let library = open(&folder);
        let view = library.views()[0].id.clone();
        let garage = library.add_camera(named("Garage"), None).unwrap();
        library
            .show_in_tile(&view, 0, Some(&garage.clone().into()))
            .unwrap();
        let revision = library.revision();

        library.delete_camera(&garage).unwrap();

        assert_eq!(library.shown_tiles(&view).unwrap(), vec![None; 4]);
        assert_eq!(library.views.get(&view).unwrap().tiles[0], None);
        assert!(library.revision() > revision);
    }

    #[test]
    fn one_camera_in_two_groups_is_edited_once() {
        let folder = TempDir::new().unwrap();
        let library = open(&folder);
        let x = library.add_group("X", Vec::new()).unwrap();
        let y = library.add_group("Y", Vec::new()).unwrap();
        let a = library.add_camera(named("Driveway"), Some(&x)).unwrap();
        library.add_to_group(&y, &a).unwrap();

        let mut changed = library.camera(&a).unwrap();
        changed.host = "10.0.0.20".into();
        library.update_cameras(vec![(a.clone(), changed)]).unwrap();

        let tree = library.tree();
        assert!(tree[0].contains(&a) && tree[1].contains(&a));
        assert_eq!(library.camera(&a).unwrap().host, "10.0.0.20");
        assert_eq!(library.cameras().len(), 1, "Driveway is stored once");
    }

    #[test]
    fn deleting_a_camera_takes_it_out_of_every_group() {
        let folder = TempDir::new().unwrap();
        let library = open(&folder);
        let x = library.add_group("X", Vec::new()).unwrap();
        let y = library.add_group("Y", Vec::new()).unwrap();
        let a = library.add_camera(named("A"), Some(&x)).unwrap();
        library.add_to_group(&y, &a).unwrap();

        library.delete_camera(&a).unwrap();

        assert!(library.camera(&a).is_none());
        assert!(library.groups_of(&a).is_empty());
        assert!(
            library.tree().iter().any(|group| group.id == x),
            "the groups themselves stay"
        );
        library
            .delete_camera(&a)
            .expect("deleting again finishes or confirms the delete");
    }

    #[test]
    fn removing_a_group_keeps_its_cameras() {
        let folder = TempDir::new().unwrap();
        let library = open(&folder);
        let x = library.add_group("X", Vec::new()).unwrap();
        let a = library.add_camera(named("A"), Some(&x)).unwrap();

        library.remove_groups(std::slice::from_ref(&x)).unwrap();

        assert!(library.camera(&a).is_some());
    }

    #[test]
    fn a_camera_only_joins_a_group_that_exists_and_vice_versa() {
        let folder = TempDir::new().unwrap();
        let library = open(&folder);
        let x = library.add_group("X", Vec::new()).unwrap();

        let error = library
            .add_camera(named("A"), Some(&GroupId::from("nope")))
            .unwrap_err();
        assert_eq!(error.to_string(), "no group `nope`");
        assert!(library.cameras().is_empty(), "nothing was added");

        let error = library
            .add_to_group(&x, &CameraId::from("ghost"))
            .unwrap_err();
        assert_eq!(error.to_string(), "no camera `ghost`");
    }

    #[test]
    fn a_group_is_made_and_saved_whole_with_cameras_that_exist() {
        let folder = TempDir::new().unwrap();
        let library = open(&folder);
        let a = library.add_camera(named("A"), None).unwrap();
        let b = library.add_camera(named("B"), None).unwrap();
        let ghost = CameraId::from("ghost");

        let error = library
            .add_group("X", vec![a.clone(), ghost.clone()])
            .unwrap_err();
        assert_eq!(error.to_string(), "no camera `ghost`");
        assert!(library.tree().is_empty(), "nothing was added");

        let x = library.add_group("X", vec![b.clone(), a.clone()]).unwrap();
        assert_eq!(library.tree()[0].cameras, [b.clone(), a.clone()]);

        let error = library
            .update_group(&x, "Y", vec![a.clone(), ghost])
            .unwrap_err();
        assert_eq!(error.to_string(), "no camera `ghost`");
        assert_eq!(library.tree()[0].name, "X", "nothing changed");

        library.update_group(&x, "Y", vec![a.clone()]).unwrap();
        assert_eq!(library.tree()[0].name, "Y");
        assert_eq!(library.tree()[0].cameras, [a]);
        assert!(library.groups_of(&b).is_empty(), "B left the group");
    }

    #[test]
    fn a_group_never_shows_a_camera_that_is_gone() {
        let folder = TempDir::new().unwrap();
        let library = open(&folder);
        let x = library.add_group("X", Vec::new()).unwrap();
        let a = library.add_camera(named("A"), Some(&x)).unwrap();

        // As if a delete stopped half way: gone from the list, still in the group's file.
        library.cameras.remove(&a).unwrap();

        assert!(library.tree()[0].cameras.is_empty());
        assert!(library.groups.get(&x).unwrap().contains(&a));

        let reopened = open(&folder);
        assert!(
            !reopened.groups.get(&x).unwrap().contains(&a),
            "opening clears it out of the file"
        );
    }

    #[test]
    fn a_broken_camera_file_is_reported_and_leaves_the_groups_alone() {
        let folder = TempDir::new().unwrap();
        let library = open(&folder);
        let x = library.add_group("Kept", Vec::new()).unwrap();
        library.add_camera(named("Garage"), Some(&x)).unwrap();
        drop(library);
        fs::write(folder.path().join(CameraStore::FILE), "not json").unwrap();
        let groups_before = fs::read_to_string(folder.path().join(GroupStore::FILE)).unwrap();

        let reopened = open(&folder);

        assert!(
            reopened.cameras().is_empty(),
            "a broken file is not a first launch"
        );
        let problems = reopened.problems();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("moved to"), "{}", problems[0]);
        assert!(
            reopened.tree()[0].cameras.is_empty(),
            "nothing to show for now"
        );
        assert_eq!(
            fs::read_to_string(folder.path().join(GroupStore::FILE)).unwrap(),
            groups_before,
            "the groups keep the camera, for when the moved-aside file is put back"
        );
    }
}
