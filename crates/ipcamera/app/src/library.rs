//! The groups, the views, the cameras' order and changes to several cameras, as TSX sees them.
//! Nothing here makes a change: each is a `Change` the backend applies where the cameras are kept.

use ipcamera_core::{
    ArrangedTile, Brand, CameraConfig, CameraId, CameraStream, CameraUpdate, CameraView, Cell,
    Change, Fit, GRID, GroupId, Layout, Library, SMALLEST_SPAN, Stream, TileStream, ViewId,
    ViewTile,
};

use crate::{
    bridge::{
        ArrangedTileInfo, CameraChanges, CellInfo, DrawnPicture, FitInfo, GridInfo, GroupInfo,
        LayoutInfo, StreamChoice, ViewInfo,
    },
    form, form_check,
};

/// Every group, each listing only cameras that exist.
pub fn groups(library: &Library) -> Vec<GroupInfo> {
    library
        .tree()
        .into_iter()
        .map(|group| GroupInfo {
            id: group.id.to_string(),
            name: group.name,
            cameras: group.cameras.iter().map(ToString::to_string).collect(),
        })
        .collect()
}

fn camera_ids(cameras: &[String]) -> Vec<CameraId> {
    cameras
        .iter()
        .map(|camera| CameraId::from(camera.as_str()))
        .collect()
}

/// Moves a camera to `index` among the cameras: the order All cameras shows.
pub fn move_camera(id: &str, index: u32) -> Change {
    Change::MoveCamera {
        id: CameraId::from(id),
        index: index as usize,
    }
}

/// Adds a group with `cameras` in it, in one save.
pub fn add_group(name: &str, cameras: &[String]) -> Change {
    Change::AddGroup {
        name: name.to_string(),
        cameras: camera_ids(cameras),
    }
}

/// Renames a group.
pub fn rename_group(id: &str, name: &str) -> Change {
    Change::RenameGroup {
        id: GroupId::from(id),
        name: name.to_string(),
    }
}

/// Gives a group its name and cameras, in one save.
pub fn save_group(id: &str, name: &str, cameras: &[String]) -> Change {
    Change::SaveGroup {
        id: GroupId::from(id),
        name: name.to_string(),
        cameras: camera_ids(cameras),
    }
}

/// Removes groups, in one save. Their cameras stay.
pub fn remove_groups(ids: &[String]) -> Change {
    Change::RemoveGroups {
        ids: ids.iter().map(|id| GroupId::from(id.as_str())).collect(),
    }
}

/// Moves a group to `index` among the groups.
pub fn move_group(id: &str, index: u32) -> Change {
    Change::MoveGroup {
        id: GroupId::from(id),
        index: index as usize,
    }
}

/// Puts a camera that exists at the end of a group.
pub fn add_to_group(group: &str, camera: &str) -> Change {
    Change::AddToGroup {
        group: GroupId::from(group),
        camera: CameraId::from(camera),
    }
}

/// Takes a camera out of one group.
pub fn remove_from_group(group: &str, camera: &str) -> Change {
    Change::RemoveFromGroup {
        group: GroupId::from(group),
        camera: CameraId::from(camera),
    }
}

/// The ids of the groups camera `camera` is in.
pub fn groups_of(library: &Library, camera: &str) -> Vec<String> {
    library
        .groups_of(&CameraId::from(camera))
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn cameras_in(tiles: &[Option<ViewTile>]) -> Vec<String> {
    tiles
        .iter()
        .map(|tile| {
            tile.as_ref()
                .map(|tile| tile.camera.to_string())
                .unwrap_or_default()
        })
        .collect()
}

fn streams_in(tiles: &[Option<ViewTile>]) -> Vec<String> {
    tiles
        .iter()
        .map(|tile| {
            tile.as_ref()
                .and_then(|tile| tile.stream.as_ref())
                .map(|stream| stream.key().to_string())
                .unwrap_or_default()
        })
        .collect()
}

fn fits_in(view: &CameraView, tiles: &[Option<ViewTile>]) -> Vec<String> {
    tiles
        .iter()
        .map(|tile| view.fit_of(tile.as_ref()).key().to_string())
        .collect()
}

/// Every view, with exactly as many tiles as it has cells, and those it keeps past them.
pub fn views(library: &Library) -> Vec<ViewInfo> {
    library
        .views()
        .into_iter()
        .map(|view| {
            let shown = library.shown_tiles(&view.id).unwrap_or_default();
            let hidden = library.hidden_tiles(&view.id).unwrap_or_default();
            ViewInfo {
                id: view.id.to_string(),
                layout: view
                    .layout()
                    .map(|layout| layout.key().to_string())
                    .unwrap_or_default(),
                cells: view.cells.iter().copied().map(cell_info).collect(),
                fit: view.fit.key().to_string(),
                tiles: cameras_in(&shown),
                streams: streams_in(&shown),
                fits: fits_in(&view, &shown),
                hidden_tiles: cameras_in(&hidden),
                hidden_streams: streams_in(&hidden),
                hidden_fits: fits_in(&view, &hidden),
                name: view.name,
            }
        })
        .collect()
}

fn cell_info(cell: Cell) -> CellInfo {
    CellInfo {
        column: u32::from(cell.column),
        row: u32::from(cell.row),
        column_span: u32::from(cell.column_span),
        row_span: u32::from(cell.row_span),
    }
}

/// Every layout a view can be set to, in the picker's order.
pub fn layouts() -> Vec<LayoutInfo> {
    Layout::ALL
        .into_iter()
        .map(|layout| LayoutInfo {
            key: layout.key().to_string(),
            title: layout.title().to_string(),
            cells: layout.on_grid().into_iter().map(cell_info).collect(),
        })
        .collect()
}

/// The grid views' tiles sit on.
pub fn grid() -> GridInfo {
    GridInfo {
        size: u32::from(GRID),
        smallest: u32::from(SMALLEST_SPAN),
    }
}

/// Arranges a view's tiles; numbers past the grid are left for the library to refuse.
pub fn arrange(id: &str, tiles: Vec<ArrangedTileInfo>) -> Change {
    let small = |value: u32| u8::try_from(value).unwrap_or(u8::MAX);
    Change::ArrangeTiles {
        id: ViewId::from(id),
        tiles: tiles
            .into_iter()
            .map(|tile| ArrangedTile {
                cell: Cell::new(
                    small(tile.column),
                    small(tile.row),
                    small(tile.column_span),
                    small(tile.row_span),
                ),
                was: tile.was.map(|was| was as usize),
            })
            .collect(),
    }
}

/// Every way a picture can fill its tile, in the order the menu shows them.
pub fn fits() -> Vec<FitInfo> {
    Fit::ALL
        .into_iter()
        .map(|fit| FitInfo {
            key: fit.key().to_string(),
            title: fit.title().to_string(),
            reshapes: fit.aspect().is_some(),
        })
        .collect()
}

/// Adds a view with empty tiles.
pub fn add_view(name: &str, layout: &str) -> Result<Change, String> {
    Ok(Change::AddView {
        name: name.to_string(),
        layout: parse_layout(layout)?,
    })
}

/// Renames a view.
pub fn rename_view(id: &str, name: &str) -> Change {
    Change::RenameView {
        id: ViewId::from(id),
        name: name.to_string(),
    }
}

/// Removes a view. Its cameras stay.
pub fn remove_view(id: &str) -> Change {
    Change::RemoveView {
        id: ViewId::from(id),
    }
}

/// Divides a view a new way.
pub fn set_layout(id: &str, layout: &str) -> Result<Change, String> {
    Ok(Change::SetLayout {
        id: ViewId::from(id),
        layout: parse_layout(layout)?,
    })
}

/// Sets how the picture fills tile `index` of view `id`.
pub fn set_tile_fit(id: &str, index: u32, fit: &str) -> Result<Change, String> {
    Ok(Change::SetTileFit {
        id: ViewId::from(id),
        index: index as usize,
        fit: Fit::from_key(fit).ok_or_else(|| format!("no fit `{fit}`"))?,
    })
}

/// Puts `camera` in tile `index` of view `id` on `stream` (`""` for Auto, `own` for its own); a
/// camera of `""` empties the tile.
pub fn set_tile(id: &str, index: u32, camera: &str, stream: &str) -> Change {
    let camera = (!camera.is_empty()).then(|| CameraId::from(camera));
    Change::SetTile {
        id: ViewId::from(id),
        index: index as usize,
        stream: camera.as_ref().and_then(|_| TileStream::from_key(stream)),
        camera,
    }
}

struct StreamRow {
    stream: Option<TileStream>,
    /// Where it is, with no login in it.
    address: String,
    fallback: String,
    listed: Option<CameraStream>,
}

impl StreamRow {
    fn choice(self) -> StreamChoice {
        let listed = self.listed.as_ref();
        let name = listed
            .map(|stream| stream.name.trim())
            .filter(|name| !name.is_empty())
            .map_or(self.fallback, str::to_string);
        let size = listed.map(form::size_text).unwrap_or_default();
        let codec = listed.and_then(CameraStream::codec).unwrap_or_default();
        StreamChoice {
            key: self
                .stream
                .as_ref()
                .map(|stream| stream.key().to_string())
                .unwrap_or_default(),
            title: form::stream_title(&size, &codec, &name),
            name,
            size,
            lens: listed
                .and_then(CameraStream::lens)
                .map(|lens| lens.title().to_string())
                .unwrap_or_default(),
            own: self.stream.is_none(),
        }
    }
}

/// One row per stream: its own, the brand's main and sub, then those it listed. A listed stream
/// already there joins that row, which keeps its key.
fn stream_rows(camera: &CameraConfig) -> Vec<StreamRow> {
    let own_name = match camera.rtsp_path_override.trim() {
        "" if camera.stream == Stream::Sub => "Sub stream".to_string(),
        "" => "Main stream".to_string(),
        path => path.to_string(),
    };
    let mut ways = vec![(None, own_name, None)];
    if camera.brand != Brand::Other {
        ways.push((
            Some(TileStream::Brand(Stream::Main)),
            "Main stream".to_string(),
            None,
        ));
        ways.push((
            Some(TileStream::Brand(Stream::Sub)),
            "Sub stream".to_string(),
            None,
        ));
    }
    for listed in &camera.streams {
        ways.push((
            Some(TileStream::Path(listed.path.clone())),
            listed.path.trim().to_string(),
            Some(listed.clone()),
        ));
    }

    let mut rows: Vec<StreamRow> = Vec::new();
    for (stream, fallback, listed) in ways {
        let address = camera.with_stream(stream.as_ref()).stream_address();
        match rows.iter_mut().find(|row| row.address == address) {
            Some(row) => {
                if row.listed.is_none() {
                    row.listed = listed;
                }
            }
            None => rows.push(StreamRow {
                stream,
                address,
                fallback,
                listed,
            }),
        }
    }
    let lenses: Vec<_> = rows
        .iter()
        .map(|row| row.listed.as_ref().and_then(CameraStream::lens))
        .collect();
    rows.sort_by_key(|row| {
        let listed = row.listed.as_ref();
        form::stream_order(
            &lenses,
            listed.and_then(CameraStream::lens),
            listed.and_then(CameraStream::size),
        )
    });
    rows
}

/// Every stream camera `id` has, once each, as the menus list them; empty for an unknown camera.
pub fn stream_choices(library: &Library, id: &str) -> Vec<StreamChoice> {
    let Some(camera) = library.camera(&CameraId::from(id)) else {
        return Vec::new();
    };
    stream_rows(&camera)
        .into_iter()
        .map(StreamRow::choice)
        .collect()
}

/// The key of camera `id`'s row that is the same stream as `key`; `key` itself where none is.
pub fn choice_key(library: &Library, id: &str, key: &str) -> String {
    let Some(camera) = library.camera(&CameraId::from(id)) else {
        return key.to_string();
    };
    let address = camera
        .with_stream(TileStream::from_key(key).as_ref())
        .stream_address();
    stream_rows(&camera)
        .into_iter()
        .find(|row| row.address == address)
        .map_or_else(
            || key.to_string(),
            |row| {
                row.stream
                    .map(|stream| stream.key().to_string())
                    .unwrap_or_default()
            },
        )
}

/// How much larger than its size Auto draws a stream before it plays a larger one.
const AUTO_STRETCH: f32 = 1.25;

fn auto_streams(camera: &CameraConfig) -> Vec<(String, (u32, u32))> {
    let rows = stream_rows(camera);
    let Some(own) = rows
        .iter()
        .find(|row| row.stream.is_none())
        .and_then(|row| row.listed.as_ref())
        .filter(|listed| listed.size().is_some())
    else {
        return Vec::new();
    };
    let lens = own.lens();
    rows.iter()
        .filter_map(|row| {
            let listed = row.listed.as_ref().filter(|listed| listed.lens() == lens)?;
            let size = listed
                .size()
                .filter(|&(width, height)| width > 0 && height > 0)?;
            let key = row.stream.as_ref().map(TileStream::key).unwrap_or_default();
            Some((key.to_string(), size))
        })
        .collect()
}

/// Whether Auto has a choice to make for camera `id`: streams of more than one size.
pub fn auto_picks(library: &Library, id: &str) -> bool {
    library.camera(&CameraId::from(id)).is_some_and(|camera| {
        let streams = auto_streams(&camera);
        streams
            .iter()
            .any(|(_, size)| Some(size) != streams.first().map(|(_, first)| first))
    })
}

/// The key Auto plays for camera `id`'s `pictures` (`""` for its own): the smallest none draws more
/// than `AUTO_STRETCH` times its size, else the largest. One stream for all its tiles.
pub fn auto_stream(library: &Library, id: &str, pictures: &[DrawnPicture]) -> String {
    let Some(camera) = library.camera(&CameraId::from(id)) else {
        return String::new();
    };
    let drawn: Vec<(Fit, (f32, f32))> = pictures
        .iter()
        .filter(|picture| picture.width > 0.0 && picture.height > 0.0)
        .map(|picture| {
            let fit = Fit::from_key(&picture.fit).unwrap_or_default();
            (fit, (picture.width as f32, picture.height as f32))
        })
        .collect();
    if drawn.is_empty() {
        return String::new();
    }
    let enlarged = |(width, height): (u32, u32)| {
        drawn
            .iter()
            .map(|(fit, room)| fit.enlargement((width as f32, height as f32), *room))
            .fold(0.0, f32::max)
    };
    let area = |(width, height): (u32, u32)| u64::from(width) * u64::from(height);
    let streams = auto_streams(&camera);
    streams
        .iter()
        .filter(|(_, size)| enlarged(*size) <= AUTO_STRETCH)
        .min_by_key(|(key, size)| (area(*size), !key.is_empty()))
        .or_else(|| {
            streams
                .iter()
                .max_by_key(|(key, size)| (area(*size), key.is_empty()))
        })
        .map(|(key, _)| key.clone())
        .unwrap_or_default()
}

/// The change giving every camera in `ids` the changes, and the ids it changes; `None` for none.
/// A saved password stays unless the changes set one, also in the web build's copy, which lacks it.
pub fn save_changes(
    library: &Library,
    ids: &[String],
    changes: &CameraChanges,
) -> Result<Option<(Change, Vec<CameraId>)>, String> {
    if let Some(problem) = form_check::change_problems(changes).into_iter().next() {
        return Err(problem.message);
    }
    let mut updates = Vec::new();
    for id in camera_ids(ids) {
        let before = library
            .camera(&id)
            .ok_or_else(|| format!("no camera `{id}`"))?;
        let mut form = form::from_config(&before);
        changes.apply_to(&mut form);
        let after = CameraConfig {
            id: before.id.clone(),
            ..form::to_config(&form)
        };
        if after != before {
            updates.push(CameraUpdate {
                id,
                camera: after,
                keep_password: changes.password.is_none(),
            });
        }
    }
    if updates.is_empty() {
        return Ok(None);
    }
    let ids = updates.iter().map(|update| update.id.clone()).collect();
    Ok(Some((Change::UpdateCameras { cameras: updates }, ids)))
}

fn parse_layout(key: &str) -> Result<Layout, String> {
    Layout::from_key(key).ok_or_else(|| format!("no layout `{key}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        Library::in_memory(vec![CameraConfig {
            name: "Porch".into(),
            host: "10.0.0.7".into(),
            user: "admin".into(),
            password: "s3cret".into(),
            ..CameraConfig::default()
        }])
    }

    fn porch(library: &Library) -> String {
        library.cameras()[0].id.to_string()
    }

    fn ids(list: Vec<GroupInfo>) -> Vec<String> {
        list.into_iter().map(|group| group.id).collect()
    }

    fn make(library: &Library, change: impl Into<Described>) -> Result<String, String> {
        let change = change.into().0?;
        library
            .apply(&change)
            .map(|applied| applied.added_id())
            .map_err(|error| error.to_string())
    }

    struct Described(Result<Change, String>);

    impl From<Change> for Described {
        fn from(change: Change) -> Self {
            Self(Ok(change))
        }
    }

    impl From<Result<Change, String>> for Described {
        fn from(change: Result<Change, String>) -> Self {
            Self(change)
        }
    }

    fn revision(library: &Library) -> u64 {
        library.revision()
    }

    fn save_changes_on(
        library: &Library,
        ids: &[String],
        changes: &CameraChanges,
    ) -> Result<Vec<CameraId>, String> {
        let Some((change, changed)) = save_changes(library, ids, changes)? else {
            return Ok(Vec::new());
        };
        make(library, change)?;
        Ok(changed)
    }

    #[test]
    fn groups_are_made_saved_moved_and_removed_whole() {
        let library = library();
        let porch = porch(&library);
        let yard = library
            .add_camera(
                CameraConfig {
                    name: "Yard".into(),
                    ..CameraConfig::default()
                },
                None,
            )
            .unwrap()
            .to_string();

        let outside = make(
            &library,
            add_group(" Outside ", &[yard.clone(), porch.clone(), yard.clone()]),
        )
        .unwrap();
        assert_eq!(groups(&library)[0].name, "Outside");
        assert_eq!(
            groups(&library)[0].cameras,
            [yard.clone(), porch.clone()],
            "in order, each once"
        );
        assert_eq!(
            make(&library, add_group("Ghosts", &["gone".to_string()])).unwrap_err(),
            "no camera `gone`"
        );

        let before = revision(&library);
        make(
            &library,
            save_group(&outside, "Yard", std::slice::from_ref(&yard)),
        )
        .unwrap();
        assert_eq!(groups(&library)[0].name, "Yard");
        assert!(revision(&library) > before, "the page hears about it");
        assert!(make(&library, save_group(&outside, " ", &[])).is_err());
        assert!(
            make(
                &library,
                save_group(&outside, "Yard", &["gone".to_string()])
            )
            .is_err()
        );
        assert_eq!(
            groups(&library)[0].cameras,
            [yard],
            "a refused save changes nothing"
        );

        let garage = make(&library, add_group("Garage", &[])).unwrap();
        let attic = make(&library, add_group("Attic", &[])).unwrap();
        make(&library, move_group(&attic, 0)).unwrap();
        assert_eq!(
            ids(groups(&library)),
            [attic.clone(), outside.clone(), garage.clone()]
        );
        assert!(make(&library, move_group("gone", 0)).is_err());

        assert_eq!(
            make(
                &library,
                remove_groups(&[attic.clone(), "gone".to_string()])
            )
            .unwrap_err(),
            "no group `gone`"
        );
        assert_eq!(groups(&library).len(), 3, "none were removed");
        make(&library, remove_groups(&[attic, garage])).unwrap();
        assert_eq!(ids(groups(&library)), std::slice::from_ref(&outside));
        assert!(make(&library, remove_groups(&["gone".to_string()])).is_err());
        make(&library, remove_groups(std::slice::from_ref(&outside))).unwrap();
        assert!(groups(&library).is_empty());
    }

    #[test]
    fn a_camera_moves_among_the_cameras_where_they_are_kept() {
        let saved = library();
        let porch = porch(&saved);
        let yard = saved
            .add_camera(
                CameraConfig {
                    name: "Yard".into(),
                    ..CameraConfig::default()
                },
                None,
            )
            .unwrap()
            .to_string();
        let order = |library: &Library| {
            library
                .cameras()
                .into_iter()
                .map(|camera| camera.id.to_string())
                .collect::<Vec<_>>()
        };

        make(&saved, move_camera(&yard, 0)).unwrap();
        assert_eq!(order(&saved), [yard.clone(), porch.clone()]);

        make(&saved, move_camera(&yard, 9)).unwrap();
        assert_eq!(order(&saved), [porch, yard], "past the end is the end");
        assert_eq!(
            make(&saved, move_camera("gone", 0)).unwrap_err(),
            "no camera `gone`"
        );
    }

    #[test]
    fn a_view_takes_only_layouts_fits_and_tiles_that_exist() {
        let library = library();
        let porch = porch(&library);
        let view = views(&library)[0].id.clone();

        assert_eq!(
            views(&library)[0].tiles,
            [porch.clone(), "".into(), "".into(), "".into()]
        );
        assert_eq!(
            make(&library, set_layout(&view, "5")).unwrap_err(),
            "no layout `5`"
        );
        assert_eq!(
            make(&library, set_tile_fit(&view, 0, "tiled")).unwrap_err(),
            "no fit `tiled`"
        );
        assert_eq!(
            make(&library, set_tile_fit(&view, 1, "fill")).unwrap_err(),
            "Tile 2 is empty. Put a camera in it first."
        );
        assert_eq!(
            make(&library, set_tile(&view, 4, &porch, "")).unwrap_err(),
            "The view has 4 tiles, so there is no tile 5."
        );
        assert_eq!(
            make(&library, set_tile(&view, 1, "gone", "")).unwrap_err(),
            "no camera `gone`"
        );
        assert!(make(&library, remove_view("gone")).is_err());

        make(&library, set_layout(&view, "9")).unwrap();
        make(&library, set_tile(&view, 8, &porch, "")).unwrap();
        make(&library, set_tile_fit(&view, 8, "16:9")).unwrap();
        assert_eq!(views(&library)[0].tiles.len(), 9);
        assert_eq!(views(&library)[0].tiles[8], porch);
        assert_eq!(views(&library)[0].streams, vec![String::new(); 9]);
        let original = views(&library)[0].fit.clone();
        assert_eq!(views(&library)[0].fits[8], "16:9");
        assert_eq!(views(&library)[0].fits[..8], vec![original.clone(); 8]);
        assert!(views(&library)[0].hidden_tiles.is_empty());

        make(&library, set_layout(&view, "4")).unwrap();
        let shown = &views(&library)[0];
        assert_eq!(shown.tiles.len(), 4);
        assert_eq!(shown.hidden_tiles.len(), 5, "tiles 5 to 9, kept");
        assert_eq!(shown.hidden_tiles[4], porch);
        assert_eq!(shown.hidden_streams, vec![String::new(); 5]);
        assert_eq!(shown.hidden_fits[4], "16:9", "kept with its tile");
        assert_eq!(layouts().len(), Layout::ALL.len());
        let listed = fits();
        assert_eq!(listed.len(), Fit::ALL.len());
        assert!(!listed[0].reshapes && listed[3].reshapes, "{listed:?}");
    }

    #[test]
    fn a_views_tiles_are_arranged_on_the_grid_and_say_no_layout_once_moved() {
        let library = library();
        let porch = porch(&library);
        let view = views(&library)[0].id.clone();
        let tile = |column, row, column_span, row_span, was| ArrangedTileInfo {
            column,
            row,
            column_span,
            row_span,
            was,
        };
        assert_eq!(views(&library)[0].layout, "4");
        assert_eq!(views(&library)[0].cells, layouts()[5].cells);
        assert_eq!(grid().size, 12);

        assert_eq!(
            make(
                &library,
                arrange(
                    &view,
                    vec![tile(0, 0, 8, 8, Some(0)), tile(6, 6, 6, 6, None)]
                )
            )
            .unwrap_err(),
            "Tiles 1 and 2 overlap."
        );
        assert_eq!(
            make(&library, arrange(&view, vec![tile(0, 0, 300, 2, None)])).unwrap_err(),
            "Tile 1 goes past the edge of the 12 × 12 grid."
        );

        make(
            &library,
            arrange(
                &view,
                vec![tile(4, 0, 8, 8, Some(0)), tile(0, 8, 4, 4, None)],
            ),
        )
        .unwrap();
        let shown = &views(&library)[0];
        assert_eq!(shown.layout, "", "its own arrangement");
        assert_eq!(shown.tiles, [porch, String::new()]);
        assert_eq!(
            shown.cells[0],
            CellInfo {
                column: 4,
                row: 0,
                column_span: 8,
                row_span: 8
            }
        );
    }

    #[test]
    fn a_tile_shows_its_camera_on_the_stream_it_picks() {
        let library = library();
        let porch = porch(&library);
        let view = views(&library)[0].id.clone();

        make(&library, set_tile(&view, 1, &porch, "sub")).unwrap();
        make(&library, set_tile(&view, 2, &porch, "/live?stream=2")).unwrap();
        let shown = &views(&library)[0];
        assert_eq!(
            shown.tiles[..3],
            [porch.clone(), porch.clone(), porch.clone()]
        );
        assert_eq!(
            shown.streams[..3],
            ["".to_string(), "sub".into(), "/live?stream=2".into()]
        );

        make(&library, set_tile(&view, 1, "", "sub")).unwrap();
        assert_eq!(
            (
                views(&library)[0].tiles[1].as_str(),
                views(&library)[0].streams[1].as_str()
            ),
            ("", ""),
            "an empty tile has no stream"
        );
    }

    #[test]
    fn a_tile_is_offered_every_stream_its_camera_has_its_own_among_them() {
        let library = library();
        let porch = porch(&library);
        let rows = |library: &Library| {
            stream_choices(library, &porch)
                .into_iter()
                .map(|choice| (choice.key, choice.title, choice.own))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows(&library),
            [
                (String::new(), "Main stream".to_string(), true),
                ("sub".to_string(), "Sub stream".to_string(), false),
            ]
        );

        let mut camera = library.cameras()[0].clone();
        camera.streams = vec![
            ipcamera_core::CameraStream {
                name: "Profile 2".into(),
                path: "/cam/realmonitor?channel=1&subtype=1".into(),
                detail: "640×360, H264".into(),
            },
            ipcamera_core::CameraStream {
                name: String::new(),
                path: "/onvif/profile3".into(),
                detail: String::new(),
            },
        ];
        camera.brand = Brand::Other;
        camera.rtsp_path_override = "/onvif/profile1".into();
        make(&library, update_camera(camera)).unwrap();
        let choices = stream_choices(&library, &porch);
        assert_eq!(
            choices
                .iter()
                .map(|choice| (choice.key.as_str(), choice.title.as_str(), choice.own))
                .collect::<Vec<_>>(),
            [
                (
                    "/cam/realmonitor?channel=1&subtype=1",
                    "640×360 · H.264 · Profile 2",
                    false
                ),
                ("", "/onvif/profile1", true),
                ("/onvif/profile3", "/onvif/profile3", false),
            ],
            "another brand gets only what it listed, a nameless one by its path, a sized one first"
        );
        assert_eq!(choices[0].size, "640×360");
        assert!(stream_choices(&library, "gone").is_empty());
    }

    #[test]
    fn one_stream_reached_two_ways_is_one_row_under_the_cameras_own_name() {
        let library = library();
        let porch = porch(&library);
        let mut camera = library.cameras()[0].clone();
        let listed = |name: &str, path: &str, detail: &str| ipcamera_core::CameraStream {
            name: name.into(),
            path: path.into(),
            detail: detail.into(),
        };
        camera.brand = Brand::Reolink;
        camera.rtsp_path_override = "/Preview_01_autotrack".into();
        camera.streams = vec![
            listed(
                "Profile000_MainStream",
                "/Preview_01_main",
                "3840×2160, H265",
            ),
            listed("Profile001_SubStream", "/Preview_01_sub", "896×512, H264"),
            listed(
                "Profile003_AutotrackStream",
                "/Preview_01_autotrack",
                "1920×1080, H264",
            ),
            listed(
                "autotrack_sub",
                "flv:channel0_autotrack_sub.bcs",
                "896×512, H264",
            ),
        ];
        make(&library, update_camera(camera)).unwrap();

        let rows: Vec<_> = stream_choices(&library, &porch)
            .into_iter()
            .map(|choice| (choice.key, choice.title, choice.lens, choice.own))
            .collect();
        let row = |key: &str, title: &str, lens: &str, own: bool| {
            (key.to_string(), title.to_string(), lens.to_string(), own)
        };
        assert_eq!(
            rows,
            [
                row(
                    "main",
                    "3840×2160 · H.265 · Profile000_MainStream",
                    "",
                    false
                ),
                row("sub", "896×512 · H.264 · Profile001_SubStream", "", false),
                row(
                    "",
                    "1920×1080 · H.264 · Profile003_AutotrackStream",
                    "Tracking lens",
                    true
                ),
                row(
                    "flv:channel0_autotrack_sub.bcs",
                    "896×512 · H.264 · autotrack_sub",
                    "Tracking lens",
                    false
                ),
            ],
            "the brand's main and sub are the listed ones, sized; the main lens's first"
        );

        assert_eq!(choice_key(&library, &porch, "/Preview_01_autotrack"), "");
        assert_eq!(choice_key(&library, &porch, "own"), "");
        assert_eq!(choice_key(&library, &porch, "/Preview_01_sub"), "sub");
        assert_eq!(choice_key(&library, &porch, "main"), "main");
        assert_eq!(choice_key(&library, &porch, "/elsewhere"), "/elsewhere");
        assert_eq!(choice_key(&library, "gone", "sub"), "sub");
    }

    #[test]
    fn auto_plays_the_smallest_stream_of_the_cameras_lens_that_fills_its_largest_picture() {
        let library = library();
        let porch = porch(&library);
        let drawn = |width: f64, height: f64, fit: &str| DrawnPicture {
            width,
            height,
            fit: fit.to_string(),
        };
        let auto = |pictures: &[DrawnPicture]| auto_stream(&library, &porch, pictures);
        assert!(!auto_picks(&library, &porch));
        assert_eq!(auto(&[drawn(320.0, 180.0, "original")]), "");

        let mut camera = library.cameras()[0].clone();
        let listed = |name: &str, path: &str, detail: &str| ipcamera_core::CameraStream {
            name: name.into(),
            path: path.into(),
            detail: detail.into(),
        };
        camera.brand = Brand::Reolink;
        camera.rtsp_path_override = String::new();
        camera.streams = vec![
            listed("main", "/Preview_01_main", "3840×2160, H265"),
            listed("sub", "/Preview_01_sub", "896×512, H264"),
            listed("autotrack_main", "/Preview_01_autotrack", "1920×1080, H264"),
            listed(
                "autotrack_sub",
                "flv:channel0_autotrack_sub.bcs",
                "640×360, H264",
            ),
        ];
        make(&library, update_camera(camera.clone())).unwrap();
        assert!(auto_picks(&library, &porch));

        assert_eq!(auto(&[drawn(480.0, 270.0, "original")]), "sub");
        assert_eq!(auto(&[drawn(1120.0, 640.0, "original")]), "sub");
        assert_eq!(auto(&[drawn(1280.0, 720.0, "original")]), "");
        assert_eq!(auto(&[drawn(7680.0, 4320.0, "original")]), "");
        assert_eq!(auto(&[drawn(600.0, 600.0, "original")]), "sub");
        assert_eq!(auto(&[drawn(600.0, 800.0, "fill")]), "");
        assert_eq!(
            auto(&[
                drawn(480.0, 270.0, "original"),
                drawn(1920.0, 1080.0, "original")
            ]),
            ""
        );
        assert_eq!(auto(&[]), "", "nothing drawn yet");

        camera.rtsp_path_override = "/Preview_01_autotrack".into();
        make(&library, update_camera(camera)).unwrap();
        assert_eq!(
            auto(&[drawn(480.0, 270.0, "original")]),
            "flv:channel0_autotrack_sub.bcs"
        );
        assert_eq!(auto(&[drawn(1920.0, 1080.0, "original")]), "");
        assert_eq!(
            auto_stream(&library, "gone", &[drawn(1.0, 1.0, "fill")]),
            ""
        );
    }

    fn update_camera(camera: CameraConfig) -> Change {
        Change::UpdateCameras {
            cameras: vec![CameraUpdate {
                id: camera.id.clone(),
                camera,
                keep_password: false,
            }],
        }
    }

    #[test]
    fn changes_reach_only_the_cameras_they_change_in_one_save() {
        let library = library();
        let porch = porch(&library);
        let before = revision(&library);
        let changes = CameraChanges {
            user: Some("viewer".to_string()),
            ..CameraChanges::default()
        };

        let changed = save_changes_on(&library, std::slice::from_ref(&porch), &changes).unwrap();

        assert_eq!(changed, [CameraId::from(porch.as_str())]);
        assert_eq!(revision(&library), before + 1, "one save");
        assert_eq!(library.cameras()[0].user, "viewer");
        assert_eq!(library.cameras()[0].name, "Porch", "names stay their own");
        assert_eq!(
            library.cameras()[0].password,
            "s3cret",
            "the password stays"
        );
        assert!(
            save_changes_on(&library, std::slice::from_ref(&porch), &changes)
                .unwrap()
                .is_empty(),
            "nothing left to change"
        );

        let bad = CameraChanges {
            dvrip_port: Some("0".to_string()),
            ..CameraChanges::default()
        };
        assert_eq!(
            save_changes_on(&library, std::slice::from_ref(&porch), &bad).unwrap_err(),
            "Use a port from 1 to 65535."
        );
        assert_eq!(
            save_changes_on(&library, &[porch, "gone".to_string()], &changes).unwrap_err(),
            "no camera `gone`"
        );
        assert_eq!(
            revision(&library),
            before + 1,
            "a refused change saves nothing"
        );
    }

    #[test]
    fn changes_from_a_copy_without_passwords_keep_the_saved_ones() {
        let saved = library();
        let porch = porch(&saved);
        // The web build's copy, which was never told the password.
        let copy = Library::from_snapshot(saved.snapshot().for_client().snapshot);
        let changes = CameraChanges {
            user: Some("viewer".to_string()),
            ..CameraChanges::default()
        };

        let (change, _) = save_changes(&copy, std::slice::from_ref(&porch), &changes)
            .unwrap()
            .expect("the user changes");
        make(&saved, change).unwrap();
        assert_eq!(saved.cameras()[0].user, "viewer");
        assert_eq!(saved.cameras()[0].password, "s3cret", "the password stays");

        let new_password = CameraChanges {
            password: Some("n3w".to_string()),
            ..CameraChanges::default()
        };
        let (change, _) = save_changes(&copy, std::slice::from_ref(&porch), &new_password)
            .unwrap()
            .expect("the password changes");
        make(&saved, change).unwrap();
        assert_eq!(saved.cameras()[0].password, "n3w");
    }
}
