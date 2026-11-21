//! Every camera the app knows, in order: `data\settings\cameras.json`. Passwords are in the clear
//! in memory and sealed in the file (`saved_password.rs`).

use std::{collections::HashSet, path::PathBuf};

use dp_store::{Document, FileStore, Json, Stored};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::Error as _};

use crate::{
    StoreError,
    camera_config::CameraConfig,
    ids::CameraId,
    records::{find, find_mut, give_unique_ids, position},
    saved_password,
};

/// `cameras.json`, each password in the clear.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraFile {
    version: u32,
    cameras: Vec<CameraConfig>,
}

/// The file as written, each password sealed.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct OnDisk {
    version: u32,
    cameras: Vec<CameraConfig>,
}

impl Serialize for CameraFile {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut cameras = self.cameras.clone();
        for camera in &mut cameras {
            camera.password = saved_password::seal(&camera.password).map_err(|error| {
                S::Error::custom(format!("{}'s password was not saved: {error}", camera.name))
            })?;
        }
        OnDisk {
            version: self.version,
            cameras,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CameraFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let OnDisk {
            version,
            mut cameras,
        } = OnDisk::deserialize(deserializer)?;
        for camera in &mut cameras {
            camera.password = saved_password::open(&camera.password);
        }
        Ok(Self { version, cameras })
    }
}

impl Document for CameraFile {
    type Format = Json;
    const VERSION: u32 = 1;
    /// Without DPAPI, only this keeps the passwords from other accounts.
    const OWNER_ONLY: bool = true;

    fn version_mut(&mut self) -> Option<&mut u32> {
        Some(&mut self.version)
    }

    fn repair(&mut self) -> bool {
        give_unique_ids(&mut self.cameras)
    }
}

/// The cameras, and the file they are saved in.
pub struct CameraStore {
    file: FileStore<CameraFile>,
}

impl Stored for CameraStore {
    type Doc = CameraFile;

    fn file(&self) -> &FileStore<CameraFile> {
        &self.file
    }
}

impl CameraStore {
    /// The file's name inside `data\settings`.
    pub const FILE: &'static str = "cameras.json";

    /// Opens the cameras saved at `path`; a missing file is an empty list.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            file: FileStore::open(path.into()),
        }
    }

    pub(crate) fn in_memory() -> Self {
        Self {
            file: FileStore::in_memory(),
        }
    }

    pub(crate) fn holding(cameras: Vec<CameraConfig>) -> Self {
        Self {
            file: FileStore::holding(CameraFile {
                version: CameraFile::VERSION,
                cameras,
            }),
        }
    }

    /// Every camera, in order.
    pub fn list(&self) -> Vec<CameraConfig> {
        self.file.read(|file| file.cameras.clone())
    }

    /// Every camera's id.
    pub fn ids(&self) -> HashSet<CameraId> {
        self.file.read(|file| {
            file.cameras
                .iter()
                .map(|camera| camera.id.clone())
                .collect()
        })
    }

    /// The camera `id`, if there is one.
    pub fn get(&self, id: &CameraId) -> Option<CameraConfig> {
        self.file.read(|file| find(&file.cameras, id).cloned())
    }

    /// Whether there is a camera `id`.
    pub fn contains(&self, id: &CameraId) -> bool {
        self.file.read(|file| find(&file.cameras, id).is_some())
    }

    /// Adds `camera` at the end under a new id, which it returns.
    pub fn add(&self, mut camera: CameraConfig) -> Result<CameraId, StoreError> {
        let id = CameraId::generate();
        camera.id = id.clone();
        self.file.update(|file| {
            file.cameras.push(camera);
            Ok(())
        })?;
        Ok(id)
    }

    /// Replaces cameras' settings in one save, keeping their ids; an unknown id changes none.
    pub fn update_many(&self, cameras: Vec<(CameraId, CameraConfig)>) -> Result<(), StoreError> {
        self.file.update(|file| {
            for (id, mut camera) in cameras {
                let saved = find_mut(&mut file.cameras, &id)?;
                camera.id = id;
                *saved = camera;
            }
            Ok(())
        })
    }

    /// Moves camera `id` to `index` in the list, or to the end past it.
    pub fn move_camera(&self, id: &CameraId, index: usize) -> Result<(), StoreError> {
        self.file.update(|file| {
            let from = position(&file.cameras, id)?;
            let moved = file.cameras.remove(from);
            let to = index.min(file.cameras.len());
            file.cameras.insert(to, moved);
            Ok(())
        })
    }

    /// Removes camera `id` from the list, leaving the groups that list it as they are.
    pub fn remove(&self, id: &CameraId) -> Result<(), StoreError> {
        self.file.update(|file| {
            let index = position(&file.cameras, id)?;
            file.cameras.remove(index);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use tempfile::TempDir;

    fn named(name: &str) -> CameraConfig {
        CameraConfig {
            name: name.into(),
            ..CameraConfig::default()
        }
    }

    fn names(store: &CameraStore) -> Vec<String> {
        store.list().into_iter().map(|camera| camera.name).collect()
    }

    #[test]
    fn each_camera_added_gets_an_id_of_its_own() {
        let folder = TempDir::new().unwrap();
        let store = CameraStore::open(folder.path().join(CameraStore::FILE));

        let garage = store.add(named("Garage")).unwrap();
        let porch = store.add(named("Porch")).unwrap();

        assert_ne!(garage, porch);
        assert_eq!(names(&store), ["Garage", "Porch"]);
        assert_eq!(store.get(&garage).unwrap().name, "Garage");
        assert!(store.contains(&porch));
    }

    #[test]
    fn adding_a_copy_of_a_camera_makes_a_second_camera() {
        let folder = TempDir::new().unwrap();
        let store = CameraStore::open(folder.path().join(CameraStore::FILE));
        let first = store.add(named("Garage")).unwrap();

        let copy = store.add(store.get(&first).unwrap()).unwrap();

        assert_ne!(first, copy);
        assert_eq!(store.list().len(), 2);
    }

    #[test]
    fn an_update_replaces_the_details_and_keeps_the_id() {
        let folder = TempDir::new().unwrap();
        let store = CameraStore::open(folder.path().join(CameraStore::FILE));
        let id = store.add(named("Garage")).unwrap();

        let mut changed = named("Garage door");
        changed.id = CameraId::from("something else");
        changed.host = "10.0.0.9".into();
        store.update_many(vec![(id.clone(), changed)]).unwrap();

        let saved = store.get(&id).unwrap();
        assert_eq!(saved.id, id);
        assert_eq!(saved.name, "Garage door");
        assert_eq!(saved.host, "10.0.0.9");
        assert_eq!(
            CameraStore::open(store.path()).get(&id).unwrap().host,
            "10.0.0.9"
        );
    }

    #[test]
    fn an_unknown_camera_cannot_be_updated_or_removed() {
        let folder = TempDir::new().unwrap();
        let store = CameraStore::open(folder.path().join(CameraStore::FILE));
        let porch = CameraId::from("porch");

        assert_eq!(
            store
                .update_many(vec![(porch.clone(), named("Porch"))])
                .unwrap_err()
                .to_string(),
            "no camera `porch`"
        );
        assert!(matches!(store.remove(&porch), Err(StoreError::NotFound(_))));
        assert_eq!(store.revision(), 0);
    }

    #[test]
    fn several_cameras_change_in_one_save_and_keep_their_ids() {
        let folder = TempDir::new().unwrap();
        let store = CameraStore::open(folder.path().join(CameraStore::FILE));
        let garage = store.add(named("Garage")).unwrap();
        let porch = store.add(named("Porch")).unwrap();
        let yard = store.add(named("Yard")).unwrap();
        let before = store.revision();

        let moved = |name: &str| CameraConfig {
            id: CameraId::from("something else"),
            host: "10.0.0.9".into(),
            ..named(name)
        };
        store
            .update_many(vec![
                (garage.clone(), moved("Garage")),
                (yard.clone(), moved("Yard")),
            ])
            .unwrap();

        assert_eq!(store.revision(), before + 1, "one save for both");
        let reopened = CameraStore::open(store.path());
        for (id, host) in [(&garage, "10.0.0.9"), (&porch, ""), (&yard, "10.0.0.9")] {
            let saved = reopened.get(id).unwrap();
            assert_eq!(&saved.id, id);
            assert_eq!(saved.host, host);
        }
    }

    #[test]
    fn an_unknown_camera_among_several_changes_none() {
        let folder = TempDir::new().unwrap();
        let store = CameraStore::open(folder.path().join(CameraStore::FILE));
        let garage = store.add(named("Garage")).unwrap();
        let before = store.revision();

        let error = store
            .update_many(vec![
                (garage.clone(), named("Garage door")),
                (CameraId::from("porch"), named("Porch")),
            ])
            .unwrap_err();

        assert_eq!(error.to_string(), "no camera `porch`");
        assert_eq!(store.revision(), before);
        assert_eq!(store.get(&garage).unwrap().name, "Garage");
    }

    #[test]
    fn a_camera_moves_in_the_list_and_stays_there() {
        let folder = TempDir::new().unwrap();
        let store = CameraStore::open(folder.path().join(CameraStore::FILE));
        let garage = store.add(named("Garage")).unwrap();
        store.add(named("Porch")).unwrap();
        let yard = store.add(named("Yard")).unwrap();

        store.move_camera(&yard, 0).unwrap();
        assert_eq!(names(&store), ["Yard", "Garage", "Porch"]);
        store.move_camera(&garage, 99).unwrap();
        assert_eq!(names(&store), ["Yard", "Porch", "Garage"], "past the end");
        assert_eq!(
            names(&CameraStore::open(store.path())),
            ["Yard", "Porch", "Garage"],
            "saved"
        );

        let before = store.revision();
        store.move_camera(&garage, 2).unwrap();
        assert_eq!(
            store.revision(),
            before,
            "where it is already: nothing saved"
        );
        assert_eq!(
            store
                .move_camera(&CameraId::from("gone"), 0)
                .unwrap_err()
                .to_string(),
            "no camera `gone`"
        );
    }

    #[test]
    fn a_removed_camera_is_gone_from_the_file() {
        let folder = TempDir::new().unwrap();
        let store = CameraStore::open(folder.path().join(CameraStore::FILE));
        let garage = store.add(named("Garage")).unwrap();
        store.add(named("Porch")).unwrap();

        store.remove(&garage).unwrap();

        assert_eq!(names(&store), ["Porch"]);
        assert_eq!(names(&CameraStore::open(store.path())), ["Porch"]);
    }

    #[test]
    fn a_password_is_read_back_from_the_file() {
        let folder = TempDir::new().unwrap();
        let store = CameraStore::open(folder.path().join(CameraStore::FILE));
        let porch = store
            .add(CameraConfig {
                password: "s3cret".into(),
                ..named("Porch")
            })
            .unwrap();
        let garage = store.add(named("Garage")).unwrap();

        let reopened = CameraStore::open(store.path());

        assert_eq!(reopened.get(&porch).unwrap().password, "s3cret");
        assert_eq!(
            reopened.get(&garage).unwrap().password,
            "",
            "none stays none"
        );
        let file: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(store.path()).unwrap()).unwrap();
        assert_eq!(
            file["cameras"][1]["password"], "",
            "the file shows which have one"
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_file_never_holds_a_password_as_itself() {
        let folder = TempDir::new().unwrap();
        let store = CameraStore::open(folder.path().join(CameraStore::FILE));
        store
            .add(CameraConfig {
                password: "s3cret".into(),
                ..named("Porch")
            })
            .unwrap();

        let text = fs::read_to_string(store.path()).unwrap();

        assert!(!text.contains("s3cret"), "{text}");
    }

    #[cfg(windows)]
    #[test]
    fn a_password_this_account_cannot_decrypt_reads_as_none_and_is_saved_as_none() {
        let folder = TempDir::new().unwrap();
        let path = folder.path().join(CameraStore::FILE);
        fs::write(
            &path,
            r#"{ "cameras": [ { "id": "a", "name": "Porch", "password": "c2VjcmV0" } ] }"#,
        )
        .unwrap();
        let store = CameraStore::open(&path);
        let porch = CameraId::from("a");

        assert_eq!(store.get(&porch).unwrap().password, "");

        store
            .update_many(vec![(porch.clone(), named("Front porch"))])
            .unwrap();
        let file: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(file["cameras"][0]["password"], "");
    }

    #[test]
    fn hand_added_cameras_get_ids_that_stay_put() {
        let folder = TempDir::new().unwrap();
        fs::write(
            folder.path().join(CameraStore::FILE),
            r#"{ "cameras": [
                { "id": "a", "name": "Garage" },
                { "name": "Porch" },
                { "id": "a", "name": "Copy of Garage" }
            ] }"#,
        )
        .unwrap();

        let store = CameraStore::open(folder.path().join(CameraStore::FILE));
        let ids: Vec<_> = store.list().into_iter().map(|camera| camera.id).collect();

        assert_eq!(ids[0], CameraId::from("a"), "the first keeps its id");
        assert!(!ids[1].is_empty());
        assert_ne!(ids[2], CameraId::from("a"), "a repeated id is replaced");
        assert_eq!(store.ids().len(), 3);

        let reopened: Vec<_> = CameraStore::open(store.path())
            .list()
            .into_iter()
            .map(|camera| camera.id)
            .collect();
        assert_eq!(reopened, ids, "the new ids were saved");
    }
}
