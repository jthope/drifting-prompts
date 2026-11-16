//! A view: a saved arrangement of camera tiles, such as "Outside" in a 2 × 2 grid.
//!
//! This is what security viewers call a saved view or a split: where each tile sits on the
//! stage, the camera in each tile, and a [`Fit`] (how each picture fills its tile). Tiles refer
//! to cameras by id, like groups do, so a camera's details stay in one place.
//!
//! A tile's place is a [`Cell`] on a grid of [`GRID`] columns and rows over the stage: any size
//! from [`SMALLEST_SPAN`] cells each way up, anywhere, so long as no two tiles overlap
//! ([`check_arrangement`]). The splits security viewers offer, even grids and a large tile with
//! small ones beside it, are [`Layout`]s: a view starts as one, and can be set to one again, but
//! is then moved and resized tile by tile ([`ArrangedTile`]). Each fits the grid exactly, as 12
//! shares out evenly in 1, 2, 3 and 4. On a 16:9 stage, a tile as many cells wide as tall is
//! 16:9 too, the shape most cameras send.
//!
//! A tile that picks no stream is on Auto: the app plays whichever of its camera's streams the
//! size it is drawn at calls for, a small one in a grid of small tiles and a large one where the
//! same camera fills the stage, and [`Fit::enlargement`] is how it tells. A tile can also pick one
//! of the camera's streams for good ([`TileStream`]), its own among them. Only the stream is the
//! tile's; the address, the login and everything else stay the camera's
//! ([`CameraConfig::with_stream`](crate::CameraConfig::with_stream)).
//!
//! A tile can also pick its own [`Fit`], since the tiles of one view come in every shape: a tall
//! one stretched, a wide one filled. A tile that picks none shows the view's.

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::{
    camera_config::Stream,
    ids::{CameraId, ViewId},
};

/// How many columns, and how many rows, a view's grid has. Every [`Layout`] fits it exactly.
pub const GRID: u8 = 12;

/// The fewest columns or rows a tile covers, so none is too small to make out.
pub const SMALLEST_SPAN: u8 = 2;

/// One view.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraView {
    /// What the view is found by; the name can change and repeat.
    pub id: ViewId,
    /// The name the sidebar shows.
    pub name: String,
    /// Where each tile sits on the [`GRID`], in the order of `tiles`.
    pub cells: Vec<Cell>,
    /// How the picture fills a tile that picks no fit of its own ([`ViewTile::fit`]).
    pub fit: Fit,
    /// What each tile shows, in the order of `cells`. `None` is an empty tile. Tiles past the
    /// count of `cells` keep their cameras for when the view has room for them again.
    pub tiles: Vec<Option<ViewTile>>,
}

impl CameraView {
    /// What each tile holds: exactly one for each of its `cells`.
    pub fn shown_tiles(&self) -> Vec<Option<ViewTile>> {
        let count = self.cells.len();
        let mut tiles: Vec<_> = self.tiles.iter().take(count).cloned().collect();
        tiles.resize(count, None);
        tiles
    }

    /// What each tile past its `cells` holds, kept for when the view has room for them again: the
    /// tile after the last shown one first.
    pub fn hidden_tiles(&self) -> Vec<Option<ViewTile>> {
        self.tiles.iter().skip(self.cells.len()).cloned().collect()
    }

    /// The layout its tiles are, when they are one of [`Layout::ALL`] exactly, in its order;
    /// `None` for tiles moved or resized to an arrangement of their own.
    pub fn layout(&self) -> Option<Layout> {
        Layout::matching(&self.cells)
    }

    /// How `tile`'s picture fills it: its own fit, else the view's, which an empty tile has too.
    pub fn fit_of(&self, tile: Option<&ViewTile>) -> Fit {
        tile.and_then(|tile| tile.fit).unwrap_or(self.fit)
    }
}

/// Why `cells` can't be a view's tiles, as a sentence for the user; `Ok` when they can: at least
/// one, each on the [`GRID`] and at least [`SMALLEST_SPAN`] each way, and no two overlapping.
pub fn check_arrangement(cells: &[Cell]) -> Result<(), String> {
    if cells.is_empty() {
        return Err("A view needs at least one tile.".to_string());
    }
    for (index, cell) in cells.iter().enumerate() {
        if cell.column_span < SMALLEST_SPAN || cell.row_span < SMALLEST_SPAN {
            return Err(format!(
                "Tile {} is smaller than {SMALLEST_SPAN} × {SMALLEST_SPAN}.",
                index + 1
            ));
        }
        if cell.column + cell.column_span > GRID || cell.row + cell.row_span > GRID {
            return Err(format!(
                "Tile {} goes past the edge of the {GRID} × {GRID} grid.",
                index + 1
            ));
        }
    }
    for (index, cell) in cells.iter().enumerate() {
        if let Some(other) = cells[index + 1..]
            .iter()
            .position(|other| cell.overlaps(*other))
        {
            return Err(format!(
                "Tiles {} and {} overlap.",
                index + 1,
                index + other + 2
            ));
        }
    }
    Ok(())
}

/// One tile of a view as its layout is edited: where it goes, and which of the view's tiles it
/// was, by index, so it keeps that tile's camera; `None` for a new, empty one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrangedTile {
    /// Its place on the [`GRID`].
    pub cell: Cell,
    /// The tile it was, from 0, counting the view's kept tiles past its cells too.
    #[serde(default)]
    pub was: Option<usize>,
}

/// What one tile of a view shows: a camera, which of its streams, and how its picture fills it.
///
/// Saved as the camera's id alone when the tile picks neither a stream (it is on Auto) nor a fit
/// (it shows the view's), and otherwise as `{ "camera": "<id>", "stream": "sub", "fit": "fill" }`,
/// with only what it picks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ViewTile {
    /// The camera, by id.
    pub camera: CameraId,
    /// The stream it shows; `None` for Auto, which plays the one the size it is drawn at calls for.
    pub stream: Option<TileStream>,
    /// How the picture fills the tile; `None` for the view's fit ([`CameraView::fit_of`]).
    pub fit: Option<Fit>,
}

/// A tile showing `camera` on Auto, with the view's fit.
impl From<CameraId> for ViewTile {
    fn from(camera: CameraId) -> Self {
        Self {
            camera,
            stream: None,
            fit: None,
        }
    }
}

impl Serialize for ViewTile {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.stream.is_none() && self.fit.is_none() {
            return self.camera.serialize(serializer);
        }
        let fields = 1 + usize::from(self.stream.is_some()) + usize::from(self.fit.is_some());
        let mut tile = serializer.serialize_struct("ViewTile", fields)?;
        tile.serialize_field("camera", &self.camera)?;
        if let Some(stream) = &self.stream {
            tile.serialize_field("stream", stream.key())?;
        }
        if let Some(fit) = self.fit {
            tile.serialize_field("fit", fit.key())?;
        }
        tile.end()
    }
}

/// A tile as `views.json` can hold it: a camera's id, or the camera with a stream, a fit or both.
#[derive(Deserialize)]
#[serde(untagged)]
enum SavedTile {
    Camera(CameraId),
    Picked {
        camera: CameraId,
        #[serde(default)]
        stream: String,
        #[serde(default)]
        fit: String,
    },
}

/// A blank stream, from a hand edit, reads as Auto, and a blank or unknown fit as the view's.
impl<'de> Deserialize<'de> for ViewTile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match SavedTile::deserialize(deserializer)? {
            SavedTile::Camera(camera) => Self::from(camera),
            SavedTile::Picked {
                camera,
                stream,
                fit,
            } => Self {
                camera,
                stream: TileStream::from_key(&stream),
                fit: Fit::from_key(&fit),
            },
        })
    }
}

/// The stream a tile picks, in place of Auto.
///
/// Saved by [`key`](Self::key): `own`, `main` or `sub`, or the path itself.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TileStream {
    /// The stream the camera's settings name, whichever that is then.
    Own,
    /// The main or the sub stream at the path the camera's brand and channel give (Dahua's
    /// `subtype`, Reolink's `Preview_01_sub`), so it follows the camera's channel when that
    /// changes.
    Brand(Stream),
    /// A path the camera listed when Detect asked it (`CameraConfig::streams`), as
    /// `rtsp_path_override` takes it: an RTSP path, query included, or `flv:` and the name of one
    /// of a Reolink's FLV streams ([`StreamPath`](crate::StreamPath)).
    Path(String),
}

impl TileStream {
    /// What `views.json` and the app call it: `own`, `main`, `sub`, or the path.
    pub fn key(&self) -> &str {
        match self {
            Self::Own => "own",
            Self::Brand(stream) => stream.label(),
            Self::Path(path) => path,
        }
    }

    /// The stream saved as `key`; `None` for a blank one, which is Auto.
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "" => None,
            "own" => Some(Self::Own),
            "main" => Some(Self::Brand(Stream::Main)),
            "sub" => Some(Self::Brand(Stream::Sub)),
            path => Some(Self::Path(path.to_string())),
        }
    }

    /// The stream a picture given `key` plays, as one camera's connections are told apart: `None`
    /// for the camera's own, whether `key` is blank or `own`, so both are one connection. Auto is
    /// never played as such: the app has picked a stream for it by then.
    pub fn played(key: &str) -> Option<Self> {
        Self::from_key(key).filter(|stream| *stream != Self::Own)
    }
}

impl Serialize for TileStream {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.key())
    }
}

/// A blank stream is refused: where there may be none, it is an `Option`, and none is left out.
impl<'de> Deserialize<'de> for TileStream {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Self::from_key(&key).ok_or_else(|| serde::de::Error::custom("a blank stream"))
    }
}

/// One tile's place on a grid, counted from the top left in whole grid cells: a view's [`GRID`],
/// or a [`Layout`]'s own smaller one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    /// The first column, from 0.
    pub column: u8,
    /// The first row, from 0.
    pub row: u8,
    /// How many columns the tile covers.
    pub column_span: u8,
    /// How many rows the tile covers.
    pub row_span: u8,
}

impl Cell {
    /// The tile from `column` and `row`, `column_span` wide and `row_span` tall.
    pub const fn new(column: u8, row: u8, column_span: u8, row_span: u8) -> Self {
        Self {
            column,
            row,
            column_span,
            row_span,
        }
    }

    /// Whether it and `other` cover a grid cell in common.
    pub fn overlaps(self, other: Cell) -> bool {
        self.column < other.column + other.column_span
            && other.column < self.column + self.column_span
            && self.row < other.row + other.row_span
            && other.row < self.row + self.row_span
    }
}

/// A way to divide a view, which it starts as and can be set to again. These are the splits
/// security viewers offer: even grids, and a large tile with small ones along one or two sides
/// for the camera that matters most.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Layout {
    /// One tile filling the view.
    One,
    /// Two tiles side by side.
    Two,
    /// Two tiles, one above the other.
    TwoStacked,
    /// A large tile on the left with 2 small ones stacked on its right, on a 3 × 2 grid.
    OnePlusTwoBeside,
    /// A large tile on top with 2 small ones side by side under it, on a 2 × 3 grid.
    OnePlusTwoBelow,
    /// A 2 × 2 grid.
    #[default]
    Four,
    /// A large tile with 5 small ones on a 3 × 3 grid.
    OnePlusFive,
    /// A large tile with 7 small ones on a 4 × 4 grid.
    OnePlusSeven,
    /// A 3 × 3 grid.
    Nine,
    /// A 4 × 4 grid.
    Sixteen,
}

impl Layout {
    /// Every layout, in the order the picker shows them.
    pub const ALL: [Layout; 10] = [
        Layout::One,
        Layout::Two,
        Layout::TwoStacked,
        Layout::OnePlusTwoBeside,
        Layout::OnePlusTwoBelow,
        Layout::Four,
        Layout::OnePlusFive,
        Layout::OnePlusSeven,
        Layout::Nine,
        Layout::Sixteen,
    ];

    /// The name saved in `views.json`.
    pub fn key(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
            Self::TwoStacked => "2-stacked",
            Self::OnePlusTwoBeside => "1+2-beside",
            Self::OnePlusTwoBelow => "1+2-below",
            Self::Four => "4",
            Self::OnePlusFive => "1+5",
            Self::OnePlusSeven => "1+7",
            Self::Nine => "9",
            Self::Sixteen => "16",
        }
    }

    /// The layout saved as `key`, if there is one.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|layout| layout.key() == key.trim())
    }

    /// What the picker calls it.
    pub fn title(self) -> &'static str {
        match self {
            Self::One => "Single",
            Self::Two => "2 side by side",
            Self::TwoStacked => "2 stacked",
            Self::OnePlusTwoBeside => "1 large + 2 beside",
            Self::OnePlusTwoBelow => "1 large + 2 below",
            Self::Four => "2 × 2",
            Self::OnePlusFive => "1 large + 5",
            Self::OnePlusSeven => "1 large + 7",
            Self::Nine => "3 × 3",
            Self::Sixteen => "4 × 4",
        }
    }

    /// The grid the cells sit on: columns, then rows.
    pub fn grid(self) -> (u8, u8) {
        match self {
            Self::One => (1, 1),
            Self::Two => (2, 1),
            Self::TwoStacked => (1, 2),
            Self::OnePlusTwoBeside => (3, 2),
            Self::OnePlusTwoBelow => (2, 3),
            Self::Four => (2, 2),
            Self::OnePlusFive | Self::Nine => (3, 3),
            Self::OnePlusSeven | Self::Sixteen => (4, 4),
        }
    }

    /// Where each tile goes. The large tile, when there is one, comes first, then the rest from
    /// the top left, a row at a time.
    pub fn cells(self) -> Vec<Cell> {
        let (columns, rows) = self.grid();
        let large = match self {
            Self::OnePlusTwoBeside | Self::OnePlusTwoBelow | Self::OnePlusFive => {
                Some(Cell::new(0, 0, 2, 2))
            }
            Self::OnePlusSeven => Some(Cell::new(0, 0, 3, 3)),
            _ => None,
        };

        let mut cells: Vec<Cell> = large.into_iter().collect();
        for row in 0..rows {
            for column in 0..columns {
                let covered = large.is_some_and(|large| {
                    column < large.column + large.column_span && row < large.row + large.row_span
                });
                if !covered {
                    cells.push(Cell::new(column, row, 1, 1));
                }
            }
        }
        cells
    }

    /// How many tiles it shows.
    pub fn tile_count(self) -> usize {
        self.cells().len()
    }

    /// Where each tile goes on a view's [`GRID`], in the order of [`cells`](Self::cells): each
    /// of its own columns and rows made the share of the 12 it takes.
    pub fn on_grid(self) -> Vec<Cell> {
        let (columns, rows) = self.grid();
        let (wide, tall) = (GRID / columns, GRID / rows);
        self.cells()
            .into_iter()
            .map(|cell| {
                Cell::new(
                    cell.column * wide,
                    cell.row * tall,
                    cell.column_span * wide,
                    cell.row_span * tall,
                )
            })
            .collect()
    }

    /// The layout whose tiles are `cells` on the [`GRID`], in the same order, if one is.
    pub fn matching(cells: &[Cell]) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|layout| layout.on_grid() == cells)
    }
}

impl Serialize for Layout {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.key())
    }
}

/// A layout this build doesn't know, from a hand edit or a newer app, reads as the default rather
/// than making the whole file unreadable.
impl<'de> Deserialize<'de> for Layout {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Ok(Self::from_key(&key).unwrap_or_default())
    }
}

/// How a picture fills its tile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Fit {
    /// The whole picture in its own shape, with bars where it doesn't fill the tile.
    #[default]
    Original,
    /// The tile filled in the picture's shape, cropping what doesn't fit.
    Fill,
    /// The whole picture stretched to the tile's shape.
    Stretch,
    /// The picture shown at 1:1, with bars.
    Square,
    /// The picture shown at 4:3, with bars.
    FourByThree,
    /// The picture shown at 3:4, with bars.
    ThreeByFour,
    /// The picture shown at 16:9, with bars.
    SixteenByNine,
    /// The picture shown at 9:16, with bars.
    NineBySixteen,
}

impl Fit {
    /// Every fit, in the order the picker shows them.
    pub const ALL: [Fit; 8] = [
        Fit::Original,
        Fit::Fill,
        Fit::Stretch,
        Fit::Square,
        Fit::FourByThree,
        Fit::ThreeByFour,
        Fit::SixteenByNine,
        Fit::NineBySixteen,
    ];

    /// The name saved in `views.json`, which is also what the video component is given.
    pub fn key(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Fill => "fill",
            Self::Stretch => "stretch",
            Self::Square => "1:1",
            Self::FourByThree => "4:3",
            Self::ThreeByFour => "3:4",
            Self::SixteenByNine => "16:9",
            Self::NineBySixteen => "9:16",
        }
    }

    /// The fit saved as `key`, if there is one.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|fit| fit.key() == key.trim())
    }

    /// What the picker calls it.
    pub fn title(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Fill => "Fill",
            Self::Stretch => "Stretch",
            other => other.key(),
        }
    }

    /// The shape a picture is forced to, width then height, for the fits that force one.
    pub fn aspect(self) -> Option<(f32, f32)> {
        match self {
            Self::Square => Some((1.0, 1.0)),
            Self::FourByThree => Some((4.0, 3.0)),
            Self::ThreeByFour => Some((3.0, 4.0)),
            Self::SixteenByNine => Some((16.0, 9.0)),
            Self::NineBySixteen => Some((9.0, 16.0)),
            Self::Original | Self::Fill | Self::Stretch => None,
        }
    }

    /// How many times its own size a `picture` is drawn in a `room` this fit fills, both in
    /// pixels, on the side it is enlarged the most: under 1 where it is drawn smaller than it is.
    /// Auto plays the smallest stream this keeps close to 1.
    pub fn enlargement(self, picture: (f32, f32), room: (f32, f32)) -> f32 {
        let (across, down) = (room.0 / picture.0, room.1 / picture.1);
        match self {
            Self::Original => across.min(down),
            Self::Fill | Self::Stretch => across.max(down),
            // Reshaped to the ratio, then fitted like `Original`: as large as the room allows.
            forced => {
                let shape = forced.aspect().unwrap_or(picture);
                let scale = (room.0 / shape.0).min(room.1 / shape.1);
                (shape.0 * scale / picture.0).max(shape.1 * scale / picture.1)
            }
        }
    }
}

impl Serialize for Fit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.key())
    }
}

/// As for [`Layout`], an unknown fit reads as the default.
impl<'de> Deserialize<'de> for Fit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Ok(Self::from_key(&key).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_covers_its_grid_exactly_once() {
        for layout in Layout::ALL {
            let (columns, rows) = layout.grid();
            let mut covered = vec![0; usize::from(columns) * usize::from(rows)];
            for cell in layout.cells() {
                for row in cell.row..cell.row + cell.row_span {
                    for column in cell.column..cell.column + cell.column_span {
                        covered[usize::from(row) * usize::from(columns) + usize::from(column)] += 1;
                    }
                }
            }
            assert!(
                covered.iter().all(|&count| count == 1),
                "{layout:?} leaves a gap or overlaps: {covered:?}"
            );
        }
    }

    #[test]
    fn the_tile_counts_are_the_ones_the_names_promise() {
        let counts: Vec<_> = Layout::ALL.map(Layout::tile_count).into();
        assert_eq!(counts, [1, 2, 2, 3, 3, 4, 6, 8, 9, 16]);
    }

    #[test]
    fn two_tiles_go_side_by_side_or_one_above_the_other() {
        assert_eq!(
            Layout::Two.cells(),
            [Cell::new(0, 0, 1, 1), Cell::new(1, 0, 1, 1)]
        );
        assert_eq!(
            Layout::TwoStacked.cells(),
            [Cell::new(0, 0, 1, 1), Cell::new(0, 1, 1, 1)]
        );
    }

    #[test]
    fn the_large_tile_comes_first() {
        assert_eq!(Layout::OnePlusFive.cells()[0], Cell::new(0, 0, 2, 2));
        assert_eq!(Layout::OnePlusSeven.cells()[0], Cell::new(0, 0, 3, 3));
        assert_eq!(Layout::OnePlusFive.cells()[1], Cell::new(2, 0, 1, 1));
    }

    #[test]
    fn the_small_tiles_go_beside_or_below_the_large_one() {
        let large = Cell::new(0, 0, 2, 2);
        assert_eq!(
            Layout::OnePlusTwoBeside.cells(),
            [large, Cell::new(2, 0, 1, 1), Cell::new(2, 1, 1, 1)]
        );
        assert_eq!(
            Layout::OnePlusTwoBelow.cells(),
            [large, Cell::new(0, 2, 1, 1), Cell::new(1, 2, 1, 1)]
        );
    }

    #[test]
    fn every_layout_fills_the_twelve_by_twelve_grid_exactly_once() {
        for layout in Layout::ALL {
            let cells = layout.on_grid();
            assert_eq!(check_arrangement(&cells), Ok(()), "{layout:?}");
            let covered: u32 = cells
                .iter()
                .map(|cell| u32::from(cell.column_span) * u32::from(cell.row_span))
                .sum();
            assert_eq!(covered, u32::from(GRID) * u32::from(GRID), "{layout:?}");
            assert_eq!(Layout::matching(&cells), Some(layout));
        }
        assert_eq!(
            Layout::OnePlusFive.on_grid()[..2],
            [Cell::new(0, 0, 8, 8), Cell::new(8, 0, 4, 4)]
        );
        assert_eq!(Layout::Sixteen.on_grid()[5], Cell::new(3, 3, 3, 3));
    }

    #[test]
    fn tiles_moved_off_a_layout_are_their_own_arrangement() {
        let mut cells = Layout::Four.on_grid();
        cells.swap(0, 1);
        assert_eq!(Layout::matching(&cells), None, "the order counts");
        cells[0] = Cell::new(6, 0, 6, 4);
        assert_eq!(check_arrangement(&cells), Ok(()), "a gap is fine");
        assert_eq!(Layout::matching(&cells), None);
    }

    #[test]
    fn tiles_stay_on_the_grid_big_enough_to_see_and_never_overlap() {
        let check = |cells: &[Cell]| check_arrangement(cells).unwrap_err();
        assert_eq!(check(&[]), "A view needs at least one tile.");
        assert_eq!(
            check(&[Cell::new(0, 0, 4, 4), Cell::new(4, 0, 1, 4)]),
            "Tile 2 is smaller than 2 × 2."
        );
        assert_eq!(
            check(&[Cell::new(10, 0, 4, 4)]),
            "Tile 1 goes past the edge of the 12 × 12 grid."
        );
        assert_eq!(
            check(&[
                Cell::new(0, 0, 4, 4),
                Cell::new(6, 6, 2, 2),
                Cell::new(3, 3, 2, 2)
            ]),
            "Tiles 1 and 3 overlap."
        );
        assert!(
            !Cell::new(0, 0, 6, 6).overlaps(Cell::new(6, 0, 6, 6)),
            "they touch"
        );
    }

    #[test]
    fn a_views_cells_and_fit_are_saved() {
        let view = CameraView {
            cells: Layout::OnePlusFive.on_grid(),
            fit: Fit::SixteenByNine,
            ..CameraView::default()
        };
        let text = serde_json::to_string(&view).unwrap();
        assert!(
            text.contains(r#""cells":[{"column":0,"row":0,"column_span":8,"row_span":8},"#),
            "{text}"
        );
        assert!(!text.contains("layout"), "{text}");
        assert!(text.contains(r#""fit":"16:9""#), "{text}");
        assert_eq!(serde_json::from_str::<CameraView>(&text).unwrap(), view);
        assert_eq!(view.layout(), Some(Layout::OnePlusFive));

        let odd: CameraView = serde_json::from_str(r#"{ "fit": "sideways" }"#).unwrap();
        assert_eq!(odd.fit, Fit::Original, "a fit this version doesn't know");
    }

    #[test]
    fn a_view_shows_as_many_tiles_as_it_has_cells_and_keeps_the_rest() {
        let a = ViewTile::from(CameraId::from("a"));
        let mut view = CameraView {
            cells: Layout::Nine.on_grid(),
            tiles: vec![None; 9],
            ..CameraView::default()
        };
        view.tiles[8] = Some(a.clone());

        view.cells = Layout::Four.on_grid();
        assert_eq!(view.shown_tiles(), vec![None; 4]);
        assert_eq!(
            view.hidden_tiles(),
            [None, None, None, None, Some(a.clone())]
        );

        view.cells = Layout::Nine.on_grid();
        assert_eq!(view.shown_tiles()[8], Some(a));
        assert_eq!(view.hidden_tiles(), []);
    }

    #[test]
    fn a_tile_on_auto_is_saved_as_the_id_alone() {
        let view = CameraView {
            tiles: vec![
                Some(CameraId::from("a").into()),
                Some(ViewTile {
                    camera: CameraId::from("a"),
                    stream: Some(TileStream::Brand(Stream::Sub)),
                    fit: None,
                }),
                Some(ViewTile {
                    camera: CameraId::from("b"),
                    stream: Some(TileStream::Path("/h264Preview_01_sub".into())),
                    fit: None,
                }),
                Some(ViewTile {
                    camera: CameraId::from("b"),
                    stream: Some(TileStream::Own),
                    fit: None,
                }),
                None,
            ],
            ..CameraView::default()
        };
        let text = serde_json::to_string(&view).unwrap();
        assert!(
            text.contains(
                r#""tiles":["a",{"camera":"a","stream":"sub"},{"camera":"b","stream":"/h264Preview_01_sub"},{"camera":"b","stream":"own"},null]"#
            ),
            "{text}"
        );
        assert_eq!(serde_json::from_str::<CameraView>(&text).unwrap(), view);
    }

    #[test]
    fn a_tile_saves_only_the_fit_it_picks_and_shows_the_views_otherwise() {
        let view = CameraView {
            fit: Fit::Stretch,
            tiles: vec![
                Some(CameraId::from("a").into()),
                Some(ViewTile {
                    camera: CameraId::from("b"),
                    stream: None,
                    fit: Some(Fit::Fill),
                }),
                Some(ViewTile {
                    camera: CameraId::from("c"),
                    stream: Some(TileStream::Brand(Stream::Sub)),
                    fit: Some(Fit::NineBySixteen),
                }),
            ],
            ..CameraView::default()
        };
        let text = serde_json::to_string(&view).unwrap();
        assert!(
            text.contains(
                r#""tiles":["a",{"camera":"b","fit":"fill"},{"camera":"c","stream":"sub","fit":"9:16"}]"#
            ),
            "{text}"
        );
        assert_eq!(serde_json::from_str::<CameraView>(&text).unwrap(), view);

        let fits: Vec<Fit> = view
            .tiles
            .iter()
            .map(|tile| view.fit_of(tile.as_ref()))
            .collect();
        assert_eq!(fits, [Fit::Stretch, Fit::Fill, Fit::NineBySixteen]);
        // An empty tile has nothing of its own: the view's.
        assert_eq!(view.fit_of(None), Fit::Stretch);
    }

    #[test]
    fn a_blank_stream_is_auto_and_an_unknown_fit_the_views() {
        let view: CameraView = serde_json::from_str(
            r#"{ "tiles": [{ "camera": "a", "stream": " " }, { "camera": "b", "fit": "sideways" }] }"#,
        )
        .unwrap();
        assert_eq!(
            view.tiles,
            [
                Some(CameraId::from("a").into()),
                Some(CameraId::from("b").into())
            ]
        );
    }

    #[test]
    fn own_main_and_sub_are_the_cameras_streams_and_anything_else_a_path() {
        assert_eq!(TileStream::from_key("own"), Some(TileStream::Own));
        assert_eq!(
            TileStream::from_key("sub"),
            Some(TileStream::Brand(Stream::Sub))
        );
        assert_eq!(
            TileStream::from_key(" main "),
            Some(TileStream::Brand(Stream::Main))
        );
        assert_eq!(
            TileStream::from_key("/live?stream=1"),
            Some(TileStream::Path("/live?stream=1".into()))
        );
        assert_eq!(TileStream::from_key(""), None);
        for key in ["own", "main", "sub", "/Preview_01_sub"] {
            assert_eq!(TileStream::from_key(key).unwrap().key(), key);
        }
    }

    #[test]
    fn the_cameras_own_stream_is_played_as_one_however_it_is_named() {
        assert_eq!(TileStream::played(""), None);
        assert_eq!(TileStream::played("own"), None);
        assert_eq!(
            TileStream::played("sub"),
            Some(TileStream::Brand(Stream::Sub))
        );
    }

    #[test]
    fn a_pictures_enlargement_is_on_the_side_its_fit_stretches_most() {
        let hd = (1920.0, 1080.0);
        // A 16:9 picture in a square room: Original fits its width, Fill its height.
        assert_eq!(Fit::Original.enlargement(hd, (960.0, 960.0)), 0.5);
        assert_eq!(Fit::Fill.enlargement(hd, (960.0, 960.0)), 960.0 / 1080.0);
        assert_eq!(Fit::Stretch.enlargement(hd, (960.0, 960.0)), 960.0 / 1080.0);
        // Twice its size in a room twice as large, whichever fit.
        for fit in [Fit::Original, Fit::Fill, Fit::Stretch, Fit::SixteenByNine] {
            assert_eq!(fit.enlargement(hd, (3840.0, 2160.0)), 2.0, "{fit:?}");
        }
        // Square in a wide room: a 1080 x 1080 box, which stretches the picture's height only.
        assert_eq!(Fit::Square.enlargement(hd, (1920.0, 1080.0)), 1.0);
        assert_eq!(
            Fit::Square.enlargement((640.0, 480.0), (1920.0, 1080.0)),
            2.25
        );
    }
}
