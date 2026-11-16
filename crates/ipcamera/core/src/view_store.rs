//! The saved views: `data\settings\views.json`.
//!
//! Like [`GroupStore`](crate::group_store::GroupStore), this store holds camera ids and never looks
//! at the cameras. [`Library`](crate::library::Library) checks a camera exists before it goes in a
//! tile, and empties the tiles of a camera that is deleted.

use std::{collections::HashSet, path::PathBuf};

use dp_store::{Document, FileStore, Json, Stored};
use serde::{Deserialize, Serialize};

use crate::{
    StoreError,
    camera_view::{ArrangedTile, CameraView, Cell, Fit, Layout, ViewTile, check_arrangement},
    ids::{CameraId, ViewId},
    records::{find, find_mut, give_unique_ids, position, required_name},
};

/// `views.json` as it is on disk.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewFile {
    version: u32,
    views: Vec<CameraView>,
}

impl Document for ViewFile {
    type Format = Json;
    const VERSION: u32 = 3;

    fn version_mut(&mut self) -> Option<&mut u32> {
        Some(&mut self.version)
    }

    /// A view added by hand may have no id, or a copy of another's, a blank camera id in a tile,
    /// and no cells or cells that can't be a view's. Each view gets an id of its own, a blank tile
    /// becomes an empty one, and cells that can't be a view's become the default 2 × 2.
    fn repair(&mut self) -> bool {
        let mut changed = give_unique_ids(&mut self.views);
        for view in &mut self.views {
            if check_arrangement(&view.cells).is_err() {
                view.cells = Layout::default().on_grid();
                changed = true;
            }
        }
        for tile in self.views.iter_mut().flat_map(|view| &mut view.tiles) {
            if tile.as_ref().is_some_and(|tile| tile.camera.is_empty()) {
                *tile = None;
                changed = true;
            }
        }
        changed
    }
}

/// The views, and the file they are saved in.
pub struct ViewStore {
    file: FileStore<ViewFile>,
}

impl Stored for ViewStore {
    type Doc = ViewFile;

    fn file(&self) -> &FileStore<ViewFile> {
        &self.file
    }
}

impl ViewStore {
    /// The file's name inside `data\settings`.
    pub const FILE: &'static str = "views.json";

    /// Opens the views saved at `path`. A missing file means no views, and nothing is written until
    /// the first change.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self {
            file: FileStore::open(path.into()),
        }
    }

    /// No views, and no file: changes are kept in memory only.
    pub(crate) fn in_memory() -> Self {
        Self {
            file: FileStore::in_memory(),
        }
    }

    /// `views`, ids and all, and no file: a copy whose changes are kept in memory only.
    pub(crate) fn holding(views: Vec<CameraView>) -> Self {
        Self {
            file: FileStore::holding(ViewFile {
                version: ViewFile::VERSION,
                views,
            }),
        }
    }

    /// Every view, in the order they are shown.
    pub fn list(&self) -> Vec<CameraView> {
        self.file.read(|file| file.views.clone())
    }

    /// The view `id`, if there is one.
    pub fn get(&self, id: &ViewId) -> Option<CameraView> {
        self.file.read(|file| find(&file.views, id).cloned())
    }

    /// Adds a view with empty tiles, divided as `layout`, at the end and returns its id.
    pub fn add(&self, name: &str, layout: Layout) -> Result<ViewId, StoreError> {
        self.add_with(name, layout, Vec::new())
    }

    /// Adds a view with `tiles` already filled in, for the first launch.
    pub(crate) fn add_with(
        &self,
        name: &str,
        layout: Layout,
        tiles: Vec<Option<ViewTile>>,
    ) -> Result<ViewId, StoreError> {
        let name = required_name::<ViewId>(name)?;
        let id = ViewId::generate();
        self.file.update(|file| {
            file.views.push(CameraView {
                id: id.clone(),
                name,
                cells: layout.on_grid(),
                fit: Fit::default(),
                tiles,
            });
            Ok(())
        })?;
        Ok(id)
    }

    /// Renames view `id`.
    pub fn rename(&self, id: &ViewId, name: &str) -> Result<(), StoreError> {
        let name = required_name::<ViewId>(name)?;
        self.change_view(id, |view| {
            view.name = name;
            Ok(())
        })
    }

    /// Removes view `id`. The cameras in it are not touched.
    pub fn remove(&self, id: &ViewId) -> Result<(), StoreError> {
        self.file.update(|file| {
            let index = position(&file.views, id)?;
            file.views.remove(index);
            Ok(())
        })
    }

    /// Divides view `id` as `layout`, in place of however its tiles were arranged: tile by tile,
    /// in order, each keeping its camera. Tiles the layout has no room for keep their cameras,
    /// for when the view has room for them again.
    pub fn set_layout(&self, id: &ViewId, layout: Layout) -> Result<(), StoreError> {
        self.change_view(id, |view| {
            view.cells = layout.on_grid();
            Ok(())
        })
    }

    /// Moves, resizes, adds and removes view `id`'s tiles at once, as `tiles` says, which
    /// [`check_arrangement`] must allow. Each keeps the camera of the tile it was, and a new one is
    /// empty; a tile left out takes its camera with it. Tiles past the old cells, kept for when the
    /// view had room for them again, stay past the new ones.
    pub fn arrange(&self, id: &ViewId, tiles: &[ArrangedTile]) -> Result<(), StoreError> {
        let cells: Vec<Cell> = tiles.iter().map(|tile| tile.cell).collect();
        check_arrangement(&cells).map_err(StoreError::Invalid)?;
        self.change_view(id, |view| {
            let used: HashSet<usize> = tiles.iter().filter_map(|tile| tile.was).collect();
            let mut arranged: Vec<Option<ViewTile>> = tiles
                .iter()
                .map(|tile| {
                    tile.was
                        .and_then(|was| view.tiles.get(was).cloned().flatten())
                })
                .collect();
            arranged.extend(
                view.tiles
                    .iter()
                    .enumerate()
                    .skip(view.cells.len())
                    .filter(|(index, _)| !used.contains(index))
                    .map(|(_, tile)| tile.clone()),
            );
            view.cells = cells;
            view.tiles = arranged;
            Ok(())
        })
    }

    /// Sets how the pictures in view `id` fill their tiles.
    pub fn set_fit(&self, id: &ViewId, fit: Fit) -> Result<(), StoreError> {
        self.change_view(id, |view| {
            view.fit = fit;
            Ok(())
        })
    }

    /// Shows `tile`, a camera and which of its streams, in tile `index` of view `id`, or empties
    /// the tile with `None`. A tile that picks no fit keeps the one the tile had, so another
    /// camera or stream there fills it as before; an empty tile keeps none.
    ///
    /// This store cannot tell whether the camera exists, so the app goes through
    /// [`Library::show_in_tile`](crate::library::Library::show_in_tile), which checks.
    pub fn set_tile(
        &self,
        id: &ViewId,
        index: usize,
        tile: Option<ViewTile>,
    ) -> Result<(), StoreError> {
        self.change_view(id, |view| {
            shown_tile(view, index)?;
            if view.tiles.len() <= index {
                view.tiles.resize(index + 1, None);
            }
            let had = view.tiles[index].as_ref().and_then(|old| old.fit);
            view.tiles[index] = tile
                .filter(|tile| !tile.camera.is_empty())
                .map(|tile| ViewTile {
                    fit: tile.fit.or(had),
                    ..tile
                });
            Ok(())
        })
    }

    /// Sets how the picture fills tile `index` of view `id`, which must show a camera: an empty
    /// tile has no picture to fit.
    pub fn set_tile_fit(&self, id: &ViewId, index: usize, fit: Fit) -> Result<(), StoreError> {
        self.change_view(id, |view| {
            shown_tile(view, index)?;
            match view.tiles.get_mut(index).and_then(Option::as_mut) {
                Some(tile) => {
                    tile.fit = Some(fit);
                    Ok(())
                }
                None => Err(StoreError::Invalid(format!(
                    "Tile {} is empty. Put a camera in it first.",
                    index + 1
                ))),
            }
        })
    }

    /// Empties every tile showing `camera`, for when it is deleted. Writes nothing when no tile
    /// shows it.
    pub fn forget_camera(&self, camera: &CameraId) -> Result<(), StoreError> {
        self.keep_cameras(|shown| shown != camera)
    }

    /// Empties every tile showing a camera `known` doesn't contain. Writes nothing when every
    /// camera is known.
    pub fn prune(&self, known: &HashSet<CameraId>) -> Result<(), StoreError> {
        self.keep_cameras(|camera| known.contains(camera))
    }

    /// Empties every tile showing a camera `keep` refuses, in one save; none when it keeps all.
    fn keep_cameras(&self, keep: impl Fn(&CameraId) -> bool) -> Result<(), StoreError> {
        self.file.update(|file| {
            for tile in file.views.iter_mut().flat_map(|view| &mut view.tiles) {
                if tile.as_ref().is_some_and(|tile| !keep(&tile.camera)) {
                    *tile = None;
                }
            }
            Ok(())
        })
    }

    /// Makes `change` to view `id`, or says there is no such view.
    fn change_view(
        &self,
        id: &ViewId,
        change: impl FnOnce(&mut CameraView) -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        self.file
            .update(|file| change(find_mut(&mut file.views, id)?))
    }
}

/// Says there is no tile `index` when `view` doesn't show that many: the tiles it keeps past its
/// cells aren't on screen to change.
fn shown_tile(view: &CameraView, index: usize) -> Result<(), StoreError> {
    if index < view.cells.len() {
        return Ok(());
    }
    Err(StoreError::Invalid(format!(
        "The view has {} tiles, so there is no tile {}.",
        view.cells.len(),
        index + 1
    )))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::camera_view::TileStream;
    use tempfile::TempDir;

    fn store(folder: &TempDir) -> ViewStore {
        ViewStore::open(folder.path().join(ViewStore::FILE))
    }

    #[test]
    fn a_view_is_added_renamed_and_removed() {
        let folder = TempDir::new().unwrap();
        let views = store(&folder);

        let id = views.add(" Outside ", Layout::Nine).unwrap();
        let view = views.get(&id).unwrap();
        assert_eq!(view.name, "Outside");
        assert_eq!(view.layout(), Some(Layout::Nine));
        assert_eq!(view.fit, Fit::Original);
        assert_eq!(view.shown_tiles(), vec![None; 9]);

        views.rename(&id, "Yard").unwrap();
        assert_eq!(ViewStore::open(views.path()).get(&id).unwrap().name, "Yard");
        assert!(views.rename(&id, " ").is_err());

        views.remove(&id).unwrap();
        assert!(views.list().is_empty());
    }

    #[test]
    fn tiles_take_cameras_and_give_them_up() {
        let folder = TempDir::new().unwrap();
        let views = store(&folder);
        let id = views.add("Outside", Layout::Four).unwrap();
        let a = ViewTile::from(CameraId::from("a"));

        views.set_tile(&id, 3, Some(a.clone())).unwrap();
        assert_eq!(views.get(&id).unwrap().shown_tiles()[3], Some(a));

        views.set_tile(&id, 3, None).unwrap();
        assert_eq!(views.get(&id).unwrap().shown_tiles(), vec![None; 4]);

        let error = views
            .set_tile(&id, 4, Some(CameraId::from("b").into()))
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "The view has 4 tiles, so there is no tile 5."
        );
    }

    #[test]
    fn tiles_are_moved_resized_added_and_removed_keeping_their_cameras() {
        let folder = TempDir::new().unwrap();
        let views = store(&folder);
        let id = views.add("Outside", Layout::Nine).unwrap();
        let [a, b, c, hidden] =
            ["a", "b", "c", "hidden"].map(|id| ViewTile::from(CameraId::from(id)));
        views.set_tile(&id, 0, Some(a.clone())).unwrap();
        views.set_tile(&id, 1, Some(b.clone())).unwrap();
        views.set_tile(&id, 2, Some(c.clone())).unwrap();
        views.set_tile(&id, 8, Some(hidden.clone())).unwrap();
        views.set_layout(&id, Layout::Four).unwrap();

        // B's tile goes wide along the top, a new one goes under it, A moves down, and C's goes.
        let tiles = [
            ArrangedTile {
                cell: Cell::new(0, 0, 12, 6),
                was: Some(1),
            },
            ArrangedTile {
                cell: Cell::new(0, 6, 6, 6),
                was: None,
            },
            ArrangedTile {
                cell: Cell::new(6, 6, 6, 6),
                was: Some(0),
            },
        ];
        views.arrange(&id, &tiles).unwrap();

        let view = ViewStore::open(views.path()).get(&id).unwrap();
        assert_eq!(view.cells, tiles.map(|tile| tile.cell));
        assert_eq!(view.layout(), None);
        assert_eq!(view.shown_tiles(), [Some(b), None, Some(a)]);
        assert_eq!(
            view.tiles[3..].iter().flatten().collect::<Vec<_>>(),
            [&hidden],
            "kept for later"
        );
        assert!(
            !view.tiles.contains(&Some(c)),
            "a tile left out takes its camera"
        );

        // A layout puts the tiles back in its order, each keeping its camera.
        views.set_layout(&id, Layout::Nine).unwrap();
        assert_eq!(views.get(&id).unwrap().layout(), Some(Layout::Nine));
    }

    #[test]
    fn an_arrangement_with_overlapping_tiles_is_refused() {
        let folder = TempDir::new().unwrap();
        let views = store(&folder);
        let id = views.add("Outside", Layout::Four).unwrap();
        let overlapping = [Cell::new(0, 0, 6, 6), Cell::new(4, 4, 6, 6)]
            .map(|cell| ArrangedTile { cell, was: None });

        let error = views.arrange(&id, &overlapping).unwrap_err();
        assert_eq!(error.to_string(), "Tiles 1 and 2 overlap.");
        assert_eq!(views.get(&id).unwrap().layout(), Some(Layout::Four));
    }

    #[test]
    fn a_view_with_no_cells_or_cells_that_cant_be_a_views_is_two_by_two() {
        let folder = TempDir::new().unwrap();
        fs::write(
            folder.path().join(ViewStore::FILE),
            r#"{ "version": 3, "views": [
                { "id": "w", "name": "W", "tiles": ["a"] },
                { "id": "x", "name": "X", "cells": [{ "column": 0, "row": 0, "column_span": 1, "row_span": 1 }] }
            ] }"#,
        )
        .unwrap();

        let views = store(&folder);
        let list = views.list();
        assert_eq!(list[0].layout(), Some(Layout::Four), "none was saved");
        assert_eq!(list[0].shown_tiles()[0], Some(CameraId::from("a").into()));
        assert_eq!(
            list[1].layout(),
            Some(Layout::Four),
            "a tile too small to see"
        );
        let saved = fs::read_to_string(views.path()).unwrap();
        assert!(
            saved.contains(r#""cells""#),
            "the repair was saved: {saved}"
        );
    }

    #[test]
    fn a_smaller_layout_keeps_the_cameras_it_hides() {
        let folder = TempDir::new().unwrap();
        let views = store(&folder);
        let id = views.add("Outside", Layout::Nine).unwrap();
        let a = ViewTile {
            camera: CameraId::from("a"),
            stream: TileStream::from_key("sub"),
            fit: Some(Fit::Stretch),
        };
        views.set_tile(&id, 8, Some(a.clone())).unwrap();

        views.set_layout(&id, Layout::Four).unwrap();
        views.set_layout(&id, Layout::Nine).unwrap();

        assert_eq!(views.get(&id).unwrap().shown_tiles()[8], Some(a));
    }

    #[test]
    fn the_fit_is_saved() {
        let folder = TempDir::new().unwrap();
        let views = store(&folder);
        let id = views.add("Outside", Layout::Four).unwrap();

        views.set_fit(&id, Fit::Fill).unwrap();

        assert_eq!(
            ViewStore::open(views.path()).get(&id).unwrap().fit,
            Fit::Fill
        );
    }

    #[test]
    fn a_tile_keeps_its_fit_through_another_camera_or_stream_but_not_emptied() {
        let folder = TempDir::new().unwrap();
        let views = store(&folder);
        let id = views.add("Outside", Layout::Four).unwrap();
        let [a, b] = ["a", "b"].map(|id| ViewTile::from(CameraId::from(id)));
        let fit_of = |views: &ViewStore, index: usize| {
            let view = views.get(&id).unwrap();
            view.tiles[index].as_ref().and_then(|tile| tile.fit)
        };

        // An empty tile has no picture to fit, and a tile the view doesn't show isn't on screen.
        assert!(matches!(
            views.set_tile_fit(&id, 0, Fit::Fill),
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            views.set_tile_fit(&id, 4, Fit::Fill),
            Err(StoreError::Invalid(_))
        ));

        views.set_tile(&id, 0, Some(a.clone())).unwrap();
        views.set_tile_fit(&id, 0, Fit::Fill).unwrap();
        assert_eq!(
            ViewStore::open(views.path()).get(&id).unwrap().tiles[0],
            Some(ViewTile {
                fit: Some(Fit::Fill),
                ..a.clone()
            }),
            "saved"
        );

        let a_sub = ViewTile {
            stream: TileStream::from_key("sub"),
            ..a.clone()
        };
        views.set_tile(&id, 0, Some(a_sub)).unwrap();
        assert_eq!(fit_of(&views, 0), Some(Fit::Fill), "another stream");
        views.set_tile(&id, 0, Some(b.clone())).unwrap();
        assert_eq!(fit_of(&views, 0), Some(Fit::Fill), "another camera");

        views.set_tile(&id, 0, None).unwrap();
        views.set_tile(&id, 0, Some(b)).unwrap();
        assert_eq!(fit_of(&views, 0), None, "emptied first");
    }

    #[test]
    fn a_forgotten_camera_leaves_every_tile() {
        let folder = TempDir::new().unwrap();
        let views = store(&folder);
        let x = views.add("X", Layout::Four).unwrap();
        let y = views.add("Y", Layout::One).unwrap();
        let [a, b] = ["a", "b"].map(|id| ViewTile::from(CameraId::from(id)));
        let a_sub = ViewTile {
            stream: TileStream::from_key("sub"),
            ..a.clone()
        };
        views.set_tile(&x, 0, Some(a.clone())).unwrap();
        views.set_tile(&x, 1, Some(b.clone())).unwrap();
        views.set_tile(&x, 2, Some(a_sub)).unwrap();
        views.set_tile(&y, 0, Some(a.clone())).unwrap();

        views.forget_camera(&a.camera).unwrap();

        assert_eq!(
            views.get(&x).unwrap().shown_tiles(),
            [None, Some(b), None, None]
        );
        assert_eq!(views.get(&y).unwrap().shown_tiles(), [None]);
    }

    #[test]
    fn prune_empties_tiles_of_cameras_that_do_not_exist() {
        let folder = TempDir::new().unwrap();
        let views = store(&folder);
        let x = views.add("X", Layout::Two).unwrap();
        let [a, gone] = ["a", "gone"].map(|id| ViewTile::from(CameraId::from(id)));
        views.set_tile(&x, 0, Some(a.clone())).unwrap();
        views.set_tile(&x, 1, Some(gone)).unwrap();

        views.prune(&HashSet::from([a.camera.clone()])).unwrap();

        assert_eq!(views.get(&x).unwrap().shown_tiles(), [Some(a), None]);
    }

    #[test]
    fn a_hand_edited_file_is_tidied_up() {
        let folder = TempDir::new().unwrap();
        fs::write(
            folder.path().join(ViewStore::FILE),
            r#"{ "views": [
                { "id": "v", "name": "V", "tiles": ["a", "", { "camera": "" }, { "camera": "b", "stream": "sub" }] },
                { "name": "No id" },
                { "id": "v", "name": "Copy of V" }
            ] }"#,
        )
        .unwrap();

        let views = store(&folder);
        let list = views.list();

        assert_eq!(
            list[0].shown_tiles(),
            [
                Some(CameraId::from("a").into()),
                None,
                None,
                Some(ViewTile {
                    camera: CameraId::from("b"),
                    stream: TileStream::from_key("sub"),
                    fit: None,
                })
            ]
        );
        assert!(!list[1].id.is_empty());
        assert_ne!(list[2].id, ViewId::from("v"));
        assert_eq!(
            ViewStore::open(views.path()).list(),
            list,
            "the repair was saved"
        );
    }
}
