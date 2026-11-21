//! A view: a saved arrangement of camera tiles on a [`GRID`] × [`GRID`] grid, each tile a camera
//! by id, which of its streams and how its picture fills it. A [`Layout`] is a preset arrangement.

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::{
    camera_config::Stream,
    ids::{CameraId, ViewId},
};

/// How many columns, and how many rows, a view's grid has. Every [`Layout`] fits it exactly.
pub const GRID: u8 = 12;

/// The fewest columns or rows a tile covers.
pub const SMALLEST_SPAN: u8 = 2;

/// One view.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraView {
    /// What the view is found by; the name can change and repeat.
    pub id: ViewId,
    /// The view's name.
    pub name: String,
    /// Where each tile sits on the [`GRID`], in the order of `tiles`.
    pub cells: Vec<Cell>,
    /// How the picture fills a tile that picks no fit of its own.
    pub fit: Fit,
    /// What each tile shows (`None` is empty); those past `cells` are kept for when there is room.
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

    /// The tiles kept past its `cells`.
    pub fn hidden_tiles(&self) -> Vec<Option<ViewTile>> {
        self.tiles.iter().skip(self.cells.len()).cloned().collect()
    }

    /// The layout its tiles are exactly, in order; `None` for an arrangement of their own.
    pub fn layout(&self) -> Option<Layout> {
        Layout::matching(&self.cells)
    }

    /// How `tile`'s picture fills it: its own fit, else the view's.
    pub fn fit_of(&self, tile: Option<&ViewTile>) -> Fit {
        tile.and_then(|tile| tile.fit).unwrap_or(self.fit)
    }
}

/// Why `cells` can't be a view's tiles, as a sentence for the user.
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

/// One tile of a view as its layout is edited: where it goes, and which tile it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrangedTile {
    /// Its place on the [`GRID`].
    pub cell: Cell,
    /// The tile it was, from 0, counting kept tiles too; `None` for a new, empty one.
    #[serde(default)]
    pub was: Option<usize>,
}

/// What one tile shows. Saved as the camera's id alone when it picks no stream and no fit.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ViewTile {
    /// The camera, by id.
    pub camera: CameraId,
    /// The stream it shows; `None` for Auto, picked by the size it is drawn at.
    pub stream: Option<TileStream>,
    /// How the picture fills the tile; `None` for the view's fit.
    pub fit: Option<Fit>,
}

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

/// A blank stream reads as Auto, and a blank or unknown fit as the view's.
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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TileStream {
    /// The stream the camera's settings name.
    Own,
    /// The main or sub stream at the brand's path for the camera's channel.
    Brand(Stream),
    /// A path the camera listed, as `rtsp_path_override` takes it.
    Path(String),
}

impl TileStream {
    /// `own`, `main`, `sub`, or the path.
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

    /// The stream `key` plays; `None` for the camera's own, blank or `own`, so both share a
    /// connection.
    pub fn played(key: &str) -> Option<Self> {
        Self::from_key(key).filter(|stream| *stream != Self::Own)
    }
}

impl Serialize for TileStream {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.key())
    }
}

/// A blank stream is refused: a missing one is an `Option`.
impl<'de> Deserialize<'de> for TileStream {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Self::from_key(&key).ok_or_else(|| serde::de::Error::custom("a blank stream"))
    }
}

/// One tile's place on a grid, in whole cells from the top left.
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

/// A preset way to divide a view.
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
    /// Every layout, in order.
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

    /// Its display name.
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

    /// Where each tile goes: the large tile first, then the rest a row at a time.
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

    /// Where each tile goes on a view's [`GRID`], in the order of [`cells`](Self::cells).
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

/// An unknown layout reads as the default rather than making the file unreadable.
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
    /// Every fit, in order.
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

    /// The name saved in `views.json`.
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

    /// Its display name.
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

    /// How many times its own size a `picture` is drawn in a `room`, on the side enlarged most.
    pub fn enlargement(self, picture: (f32, f32), room: (f32, f32)) -> f32 {
        let (across, down) = (room.0 / picture.0, room.1 / picture.1);
        match self {
            Self::Original => across.min(down),
            Self::Fill | Self::Stretch => across.max(down),
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

/// An unknown fit reads as the default.
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
        assert_eq!(Fit::Original.enlargement(hd, (960.0, 960.0)), 0.5);
        assert_eq!(Fit::Fill.enlargement(hd, (960.0, 960.0)), 960.0 / 1080.0);
        assert_eq!(Fit::Stretch.enlargement(hd, (960.0, 960.0)), 960.0 / 1080.0);
        for fit in [Fit::Original, Fit::Fill, Fit::Stretch, Fit::SixteenByNine] {
            assert_eq!(fit.enlargement(hd, (3840.0, 2160.0)), 2.0, "{fit:?}");
        }
        assert_eq!(Fit::Square.enlargement(hd, (1920.0, 1080.0)), 1.0);
        assert_eq!(
            Fit::Square.enlargement((640.0, 480.0), (1920.0, 1080.0)),
            2.25
        );
    }
}
