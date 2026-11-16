//! The camera groups: `data\settings\groups.json`.
//!
//! A group is a name and a list of camera ids, nothing more. This store never looks at the cameras
//! themselves, so a group edit can never change a camera or restart a stream. Putting a camera in a
//! group a second time does nothing, and removing it from one group leaves it in the others.
//!
//! [`Library`](crate::library::Library) is what checks a camera exists before it goes in a group,
//! and what clears out ids of cameras that were deleted.

use std::{collections::HashSet, path::PathBuf};

use dp_store::{Document, FileStore, Json, Stored};
use serde::{Deserialize, Serialize};

use crate::{
    StoreError,
    camera_group::CameraGroup,
    ids::{CameraId, GroupId},
    records::{find, find_mut, give_unique_ids, not_found, position, required_name},
};

/// `groups.json` as it is on disk.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GroupFile {
    version: u32,
    groups: Vec<CameraGroup>,
}

impl Document for GroupFile {
    type Format = Json;
    const VERSION: u32 = 1;

    fn version_mut(&mut self) -> Option<&mut u32> {
        Some(&mut self.version)
    }

    /// A group added by hand may have no id, or a copy of another's, and may list a camera twice
    /// or a blank id. Each group gets an id of its own, and each camera is listed once.
    fn repair(&mut self) -> bool {
        let mut changed = give_unique_ids(&mut self.groups);
        for group in &mut self.groups {
            let mut seen_cameras = HashSet::new();
            let before = group.cameras.len();
            group
                .cameras
                .retain(|camera| !camera.is_empty() && seen_cameras.insert(camera.clone()));
            changed |= group.cameras.len() != before;
        }
        changed
    }
}

/// The groups, and the file they are saved in.
pub struct GroupStore {
    file: FileStore<GroupFile>,
}

impl Stored for GroupStore {
    type Doc = GroupFile;

    fn file(&self) -> &FileStore<GroupFile> {
        &self.file
    }
}

impl GroupStore {
    /// The file's name inside `data\settings`.
    pub const FILE: &'static str = "groups.json";

    /// Opens the groups saved at `path`. A missing file means no groups, and nothing is written
    /// until the first change.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            file: FileStore::open(path.into()),
        }
    }

    /// No groups, and no file: changes are kept in memory only.
    pub(crate) fn in_memory() -> Self {
        Self {
            file: FileStore::in_memory(),
        }
    }

    /// `groups`, ids and all, and no file: a copy whose changes are kept in memory only.
    pub(crate) fn holding(groups: Vec<CameraGroup>) -> Self {
        Self {
            file: FileStore::holding(GroupFile {
                version: GroupFile::VERSION,
                groups,
            }),
        }
    }

    /// Every group, in the order they are shown.
    pub fn list(&self) -> Vec<CameraGroup> {
        self.file.read(|file| file.groups.clone())
    }

    /// The group `id`, if there is one.
    pub fn get(&self, id: &GroupId) -> Option<CameraGroup> {
        self.file.read(|file| find(&file.groups, id).cloned())
    }

    /// The groups `camera` is in, in the order they are shown.
    pub fn groups_of(&self, camera: &CameraId) -> Vec<GroupId> {
        self.file.read(|file| {
            file.groups
                .iter()
                .filter(|group| group.contains(camera))
                .map(|group| group.id.clone())
                .collect()
        })
    }

    /// Adds an empty group at the end and returns its id: the tests' shorthand for
    /// [`add_with`](Self::add_with), which the app uses.
    #[cfg(test)]
    pub fn add(&self, name: &str) -> Result<GroupId, StoreError> {
        self.add_with(name, Vec::new())
    }

    /// Adds a group at the end with `cameras` in it, in that order, in one save, and returns its
    /// id. A camera listed twice goes in once.
    ///
    /// This store cannot tell whether the cameras exist, so the app goes through
    /// [`Library::add_group`](crate::library::Library::add_group), which checks.
    pub fn add_with(&self, name: &str, cameras: Vec<CameraId>) -> Result<GroupId, StoreError> {
        let name = required_name::<GroupId>(name)?;
        let cameras = camera_list(cameras)?;
        let id = GroupId::generate();
        self.file.update(|file| {
            file.groups.push(CameraGroup {
                id: id.clone(),
                name,
                cameras,
            });
            Ok(())
        })?;
        Ok(id)
    }

    /// Renames group `id`. Its cameras stay as they are.
    pub fn rename(&self, id: &GroupId, name: &str) -> Result<(), StoreError> {
        let name = required_name::<GroupId>(name)?;
        self.change_group(id, |group| {
            group.name = name;
            Ok(())
        })
    }

    /// Gives group `id` a name and `cameras`, in that order, in one save: what a group's sheet
    /// saves. A camera listed twice goes in once. Writes nothing when the group already is so.
    ///
    /// As with [`add_with`](Self::add_with), the app goes through
    /// [`Library::update_group`](crate::library::Library::update_group), which checks the cameras.
    pub fn update(
        &self,
        id: &GroupId,
        name: &str,
        cameras: Vec<CameraId>,
    ) -> Result<(), StoreError> {
        let name = required_name::<GroupId>(name)?;
        let cameras = camera_list(cameras)?;
        self.change_group(id, |group| {
            group.name = name;
            group.cameras = cameras;
            Ok(())
        })
    }

    /// Removes every group in `ids` in one save. Their cameras are not touched: they stay in the
    /// camera list and in every other group. An unknown id removes none of them, so the file never
    /// holds half of a change.
    pub fn remove_many(&self, ids: &[GroupId]) -> Result<(), StoreError> {
        self.file.update(|file| {
            if let Some(unknown) = ids.iter().find(|id| find(&file.groups, id).is_none()) {
                return Err(not_found(unknown));
            }
            file.groups.retain(|group| !ids.contains(&group.id));
            Ok(())
        })
    }

    /// Moves group `id` to `index` among the groups, which is where the sidebar lists it. An index
    /// past the end moves it to the end.
    pub fn move_group(&self, id: &GroupId, index: usize) -> Result<(), StoreError> {
        self.file.update(|file| {
            let from = position(&file.groups, id)?;
            let moved = file.groups.remove(from);
            let to = index.min(file.groups.len());
            file.groups.insert(to, moved);
            Ok(())
        })
    }

    /// Puts `camera` at the end of group `id`. A camera already in it stays where it is.
    ///
    /// This store cannot tell whether the camera exists, so the app goes through
    /// [`Library::add_to_group`](crate::library::Library::add_to_group), which checks.
    pub fn add_camera(&self, id: &GroupId, camera: &CameraId) -> Result<(), StoreError> {
        if camera.is_empty() {
            return Err(no_camera_id());
        }
        self.change_group(id, |group| {
            if !group.contains(camera) {
                group.cameras.push(camera.clone());
            }
            Ok(())
        })
    }

    /// Takes `camera` out of group `id` only. It stays in every other group and in the camera list.
    pub fn remove_camera(&self, id: &GroupId, camera: &CameraId) -> Result<(), StoreError> {
        self.change_group(id, |group| {
            group.cameras.retain(|listed| listed != camera);
            Ok(())
        })
    }

    /// Takes `camera` out of every group, for when it is deleted. Writes nothing if it is in none.
    pub fn forget_camera(&self, camera: &CameraId) -> Result<(), StoreError> {
        self.keep_cameras(|listed| listed != camera)
    }

    /// Takes out every camera `known` doesn't contain: ones deleted since, or a hand edit's typo.
    /// Writes nothing when every camera is known.
    pub fn prune(&self, known: &HashSet<CameraId>) -> Result<(), StoreError> {
        self.keep_cameras(|camera| known.contains(camera))
    }

    /// Takes every camera `keep` refuses out of every group, in one save; none when it keeps all.
    fn keep_cameras(&self, keep: impl Fn(&CameraId) -> bool) -> Result<(), StoreError> {
        self.file.update(|file| {
            for group in &mut file.groups {
                group.cameras.retain(&keep);
            }
            Ok(())
        })
    }

    /// Makes `change` to group `id`, or says there is no such group.
    fn change_group(
        &self,
        id: &GroupId,
        change: impl FnOnce(&mut CameraGroup) -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        self.file
            .update(|file| change(find_mut(&mut file.groups, id)?))
    }
}

/// `cameras` as a group lists them: each once, in the order first given, or why they can't be.
fn camera_list(cameras: Vec<CameraId>) -> Result<Vec<CameraId>, StoreError> {
    let mut seen = HashSet::new();
    let mut list = Vec::with_capacity(cameras.len());
    for camera in cameras {
        if camera.is_empty() {
            return Err(no_camera_id());
        }
        if seen.insert(camera.clone()) {
            list.push(camera);
        }
    }
    Ok(list)
}

fn no_camera_id() -> StoreError {
    StoreError::Invalid("A camera with no id can't join a group.".into())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use tempfile::TempDir;

    fn store(folder: &TempDir) -> GroupStore {
        GroupStore::open(folder.path().join(GroupStore::FILE))
    }

    fn cameras_in(store: &GroupStore, id: &GroupId) -> Vec<CameraId> {
        store.get(id).unwrap().cameras
    }

    #[test]
    fn a_group_is_added_renamed_and_removed() {
        let folder = TempDir::new().unwrap();
        let groups = store(&folder);

        let outside = groups.add("  Outside ").unwrap();
        assert_eq!(groups.get(&outside).unwrap().name, "Outside");

        groups.rename(&outside, "Yard").unwrap();
        assert_eq!(
            GroupStore::open(groups.path()).get(&outside).unwrap().name,
            "Yard"
        );

        groups.remove_many(std::slice::from_ref(&outside)).unwrap();
        assert!(groups.list().is_empty());
        assert!(matches!(
            groups.remove_many(std::slice::from_ref(&outside)),
            Err(StoreError::NotFound(_))
        ));
    }

    #[test]
    fn a_group_needs_a_name() {
        let folder = TempDir::new().unwrap();
        let groups = store(&folder);

        assert_eq!(
            groups.add("   ").unwrap_err().to_string(),
            "A group needs a name."
        );
        let id = groups.add("Outside").unwrap();
        assert!(groups.rename(&id, "").is_err());
        assert_eq!(groups.get(&id).unwrap().name, "Outside");
    }

    #[test]
    fn one_camera_can_be_in_two_groups_and_leave_one() {
        let folder = TempDir::new().unwrap();
        let groups = store(&folder);
        let x = groups.add("X").unwrap();
        let y = groups.add("Y").unwrap();
        let a = CameraId::from("a");

        groups.add_camera(&x, &a).unwrap();
        groups.add_camera(&y, &a).unwrap();
        assert_eq!(groups.groups_of(&a), [x.clone(), y.clone()]);

        groups.remove_camera(&x, &a).unwrap();

        assert!(cameras_in(&groups, &x).is_empty());
        assert_eq!(cameras_in(&groups, &y), std::slice::from_ref(&a));
        assert_eq!(groups.groups_of(&a), [y]);
    }

    #[test]
    fn adding_a_camera_twice_changes_nothing() {
        let folder = TempDir::new().unwrap();
        let groups = store(&folder);
        let x = groups.add("X").unwrap();
        let a = CameraId::from("a");
        groups.add_camera(&x, &a).unwrap();
        let revision = groups.revision();

        groups.add_camera(&x, &a).unwrap();

        assert_eq!(cameras_in(&groups, &x), [a]);
        assert_eq!(groups.revision(), revision, "nothing to save");
    }

    #[test]
    fn a_camera_can_only_join_a_group_that_exists() {
        let folder = TempDir::new().unwrap();
        let groups = store(&folder);

        let error = groups
            .add_camera(&GroupId::from("nope"), &CameraId::from("a"))
            .unwrap_err();

        assert_eq!(error.to_string(), "no group `nope`");
    }

    #[test]
    fn a_forgotten_camera_leaves_every_group() {
        let folder = TempDir::new().unwrap();
        let groups = store(&folder);
        let x = groups.add("X").unwrap();
        let y = groups.add("Y").unwrap();
        let [a, b] = ["a", "b"].map(CameraId::from);
        groups.add_camera(&x, &a).unwrap();
        groups.add_camera(&x, &b).unwrap();
        groups.add_camera(&y, &a).unwrap();

        groups.forget_camera(&a).unwrap();

        assert_eq!(cameras_in(&groups, &x), [b]);
        assert!(cameras_in(&groups, &y).is_empty());
        let revision = groups.revision();
        groups.forget_camera(&a).unwrap();
        assert_eq!(groups.revision(), revision, "nothing left to forget");
    }

    #[test]
    fn prune_drops_cameras_that_do_not_exist() {
        let folder = TempDir::new().unwrap();
        let groups = store(&folder);
        let x = groups.add("X").unwrap();
        let [a, gone] = ["a", "gone"].map(CameraId::from);
        groups.add_camera(&x, &a).unwrap();
        groups.add_camera(&x, &gone).unwrap();

        groups.prune(&HashSet::from([a.clone()])).unwrap();

        assert_eq!(cameras_in(&groups, &x), [a]);
    }

    #[test]
    fn a_group_is_made_with_its_cameras_and_saved_whole() {
        let folder = TempDir::new().unwrap();
        let groups = store(&folder);
        let [a, b, c] = ["a", "b", "c"].map(CameraId::from);

        let x = groups
            .add_with(" Outside ", vec![b.clone(), a.clone(), b.clone()])
            .unwrap();
        let made = GroupStore::open(groups.path()).get(&x).unwrap();
        assert_eq!(made.name, "Outside");
        assert_eq!(made.cameras, [b.clone(), a.clone()], "in order, each once");

        let revision = groups.revision();
        groups
            .update(&x, "Yard", vec![c.clone(), a.clone()])
            .unwrap();
        assert_eq!(groups.revision(), revision + 1, "one save");
        let saved = GroupStore::open(groups.path()).get(&x).unwrap();
        assert_eq!((saved.name.as_str(), saved.cameras), ("Yard", vec![c, a]));

        let revision = groups.revision();
        let same = groups.get(&x).unwrap();
        groups.update(&x, &same.name, same.cameras).unwrap();
        assert_eq!(groups.revision(), revision, "nothing to save");
    }

    #[test]
    fn a_whole_save_with_a_problem_changes_nothing() {
        let folder = TempDir::new().unwrap();
        let groups = store(&folder);
        let a = CameraId::from("a");
        let x = groups.add_with("X", vec![a.clone()]).unwrap();

        assert!(groups.update(&x, " ", Vec::new()).is_err());
        assert!(groups.update(&x, "Y", vec![CameraId::from("")]).is_err());
        assert!(groups.add_with("", Vec::new()).is_err());
        assert!(matches!(
            groups.update(&GroupId::from("nope"), "Y", Vec::new()),
            Err(StoreError::NotFound(_))
        ));

        assert_eq!(groups.list().len(), 1);
        assert_eq!(groups.get(&x).unwrap().name, "X");
        assert_eq!(cameras_in(&groups, &x), [a]);
    }

    #[test]
    fn several_groups_are_removed_in_one_save_or_none_are() {
        let folder = TempDir::new().unwrap();
        let groups = store(&folder);
        let [x, y, z] = ["X", "Y", "Z"].map(|name| groups.add(name).unwrap());

        let error = groups
            .remove_many(&[x.clone(), GroupId::from("nope")])
            .unwrap_err();
        assert_eq!(error.to_string(), "no group `nope`");
        assert_eq!(groups.list().len(), 3, "none were removed");

        let revision = groups.revision();
        groups.remove_many(&[z.clone(), x.clone()]).unwrap();
        assert_eq!(groups.revision(), revision + 1, "one save");
        let left: Vec<_> = GroupStore::open(groups.path())
            .list()
            .into_iter()
            .map(|group| group.id)
            .collect();
        assert_eq!(left, [y]);
    }

    #[test]
    fn groups_are_moved_among_themselves() {
        let folder = TempDir::new().unwrap();
        let groups = store(&folder);
        let [x, y, z] = ["X", "Y", "Z"].map(|name| groups.add(name).unwrap());
        let order = |groups: &GroupStore| -> Vec<GroupId> {
            groups.list().into_iter().map(|group| group.id).collect()
        };

        groups.move_group(&z, 0).unwrap();
        assert_eq!(order(&groups), [z.clone(), x.clone(), y.clone()]);

        groups.move_group(&z, 99).unwrap();
        assert_eq!(order(&GroupStore::open(groups.path())), [x, y, z]);

        assert!(groups.move_group(&GroupId::from("nope"), 0).is_err());
    }

    #[test]
    fn a_hand_edited_file_is_tidied_up() {
        let folder = TempDir::new().unwrap();
        fs::write(
            folder.path().join(GroupStore::FILE),
            r#"{ "groups": [
                { "id": "x", "name": "X", "cameras": ["a", "a", "", "b"] },
                { "name": "No id" },
                { "id": "x", "name": "Copy of X" }
            ] }"#,
        )
        .unwrap();

        let groups = store(&folder);
        let list = groups.list();

        assert_eq!(list[0].id, GroupId::from("x"));
        assert_eq!(list[0].cameras, ["a", "b"].map(CameraId::from));
        assert!(!list[1].id.is_empty());
        assert_ne!(list[2].id, GroupId::from("x"));
        assert_eq!(
            GroupStore::open(groups.path()).list(),
            list,
            "the repair was saved"
        );
    }
}
