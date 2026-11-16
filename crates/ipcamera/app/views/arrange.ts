// Editing a view's layout: its tiles moved, resized, added and removed on the
// grid every view's tiles sit on (`Views.grid`, 12 × 12), as a draft that the
// stage shows in place of the live tiles until Done saves it (`Views.arrange`)
// or Cancel drops it. A draft keeps the library's rule
// (`ipcamera_core::check_arrangement`): every tile on the grid, at least the
// smallest size each way, and no two overlapping. A move or a resize that would
// break it isn't made, so the tile goes back where it was. Like `model.ts`, the
// draft is a module-level signal, so the header's layout menu and the stage
// share it.
import { computed, signal } from "nsh";
import { type CellInfo, type LayoutInfo, type ShownStream, type ViewInfo, Views } from "ipcamera/native";
import { failed } from "./actions";
import { eachOnce, grid, layouts, playedIn, playedStream, refresh, views, zoomedTile } from "./model";

/**
 * One tile of a draft: where it is, and which of the view's tiles it was, by
 * index (`null` for a new one), whose camera it keeps. `key` stays with it as it
 * moves, for the stage.
 */
export type DraftTile = { key: number; was: number | null; cell: CellInfo };

/** The layout being edited: the view's id, and its tiles as they are now; `null` while none is. */
export const draft = signal<{ view: string; tiles: DraftTile[] } | null>(null);

/** Where the draft's tiles are, in order: what the layout menu shows and checks while it is open. */
export const draftCells = computed(() => draft()?.tiles.map((tile) => tile.cell) ?? []);

/**
 * The camera in `view`'s tile `was`, one it shows or one it keeps past its
 * cells (`hiddenTiles`), and the stream it plays: the one it played on the
 * stage (`played`, as `playedIn` has them) for one it shows, and for one it
 * keeps, which no picture has shown, the one it picks, its camera's own on
 * Auto. None for a new tile or an empty one.
 */
export function showingOf(view: ViewInfo, was: number | null, played = playedIn(view)): ShownStream | undefined {
  if (was === null) return undefined;
  const shown = was < view.tiles.length;
  const past = was - view.tiles.length;
  const camera = (shown ? view.tiles[was] : view.hiddenTiles[past]) ?? "";
  if (camera === "") return undefined;
  return { camera, stream: (shown ? played[was] : playedStream(camera, view.hiddenStreams[past] ?? "", [])) ?? "" };
}

/**
 * How the picture of `view`'s tile `was` fills it, as `showingOf` finds the
 * tile: its own fit, which it keeps wherever it is moved, else the view's,
 * which a new tile takes.
 */
export function fitOf(view: ViewInfo, was: number | null): string {
  if (was === null) return view.fit;
  const fit = was < view.tiles.length ? view.fits[was] : view.hiddenFits[was - view.tiles.length];
  return fit ?? view.fit;
}

/** The streams `tiles` of `view` show, each once. */
export function streamsOf(view: ViewInfo, tiles: DraftTile[]): ShownStream[] {
  const played = playedIn(view);
  return eachOnce(tiles.flatMap((tile) => showingOf(view, tile.was, played) ?? []));
}

/**
 * The streams the draft's tiles show, each once, while a layout is edited:
 * the stage's in place of its view's own, so a camera a layout brings back
 * from past the cells plays at once. `undefined` while none is.
 */
export const draftStreams = computed(() => {
  const current = draft();
  const view = current && views().find((each) => each.id === current.view);
  return current && view ? streamsOf(view, current.tiles) : undefined;
});

let keys = 0;
const tileOf = (cell: CellInfo, was: number | null): DraftTile => ({ key: keys++, was, cell });

/** Starts editing `view`'s layout, its tiles as they are; a zoomed tile goes back among the others. */
export function startEditing(view: ViewInfo) {
  zoomedTile.set(null);
  draft.set({ view: view.id, tiles: view.cells.map((cell, index) => tileOf(cell, index)) });
}

/** Drops the draft: the view stays as it was saved. */
export function cancelEditing() {
  draft.set(null);
}

/** Whether `cell` lies on the grid, at least the smallest size each way. */
export function onGrid(cell: CellInfo): boolean {
  return (
    cell.column >= 0 &&
    cell.row >= 0 &&
    cell.columnSpan >= grid.smallest &&
    cell.rowSpan >= grid.smallest &&
    cell.column + cell.columnSpan <= grid.size &&
    cell.row + cell.rowSpan <= grid.size
  );
}

/** Whether `one` and `other` cover a grid cell in common. */
export function overlap(one: CellInfo, other: CellInfo): boolean {
  return one.column < other.column + other.columnSpan && other.column < one.column + one.columnSpan && one.row < other.row + other.rowSpan && other.row < one.row + one.rowSpan;
}

/** Whether `cell` can be among `tiles`: on the grid, and over none of them but the one keyed `moving`, which it would replace. */
export function fits(tiles: DraftTile[], cell: CellInfo, moving?: number): boolean {
  return onGrid(cell) && tiles.every((tile) => tile.key === moving || !overlap(tile.cell, cell));
}

const within = (value: number, low: number, high: number) => Math.max(low, Math.min(high, value));

/** Puts the draft's tile keyed `key` at `to(cell)`, when that fits; says whether it did. */
function place(key: number, to: (cell: CellInfo) => CellInfo): boolean {
  const current = draft();
  const tile = current?.tiles.find((each) => each.key === key);
  if (!current || !tile) return false;
  const cell = to(tile.cell);
  if (!fits(current.tiles, cell, key)) return false;
  draft.set({ ...current, tiles: current.tiles.map((each) => (each.key === key ? { ...each, cell } : each)) });
  return true;
}

/**
 * Moves the tile keyed `key` by `columns` and `rows`, stopping at the grid's
 * edges; says whether it moved, which it doesn't onto another tile.
 */
export function moveTile(key: number, columns: number, rows: number): boolean {
  return place(key, (cell) => ({
    ...cell,
    column: within(cell.column + columns, 0, grid.size - cell.columnSpan),
    row: within(cell.row + rows, 0, grid.size - cell.rowSpan),
  }));
}

/** How far a resize moves each of a tile's edges, in cells, right and down being positive; an edge left out stays put. */
export type EdgeMoves = { left?: number; top?: number; right?: number; bottom?: number };

/** One axis of a resize: the tile's start and span along it after its start edge moved by `start` and its end edge by `end`, no smaller than the smallest and within the grid. */
function stretch(at: number, span: number, start: number, end: number): [number, number] {
  const from = within(at + start, 0, at + span - grid.smallest);
  const to = within(at + span + end, from + grid.smallest, grid.size);
  return [from, to - from];
}

/**
 * Moves the edges of the tile keyed `key` by `edges`, each no closer to the
 * one across from it than the smallest size and no further than the grid's
 * edge, so a tile resizes from any edge or corner with the others staying put;
 * says whether it changed, which it doesn't over another tile.
 */
export function resizeTile(key: number, edges: EdgeMoves): boolean {
  return place(key, (cell) => {
    const [column, columnSpan] = stretch(cell.column, cell.columnSpan, edges.left ?? 0, edges.right ?? 0);
    const [row, rowSpan] = stretch(cell.row, cell.rowSpan, edges.top ?? 0, edges.bottom ?? 0);
    return { ...cell, column, row, columnSpan, rowSpan };
  });
}

/** Where a new tile goes among `tiles`: the first room from the top left for one 4, then 3, then the smallest cells square; `null` when there is none. */
export function roomFor(tiles: DraftTile[]): CellInfo | null {
  for (const span of [4, 3, grid.smallest]) {
    for (let row = 0; row + span <= grid.size; row++) {
      for (let column = 0; column + span <= grid.size; column++) {
        const cell = { column, row, columnSpan: span, rowSpan: span };
        if (fits(tiles, cell)) return cell;
      }
    }
  }
  return null;
}

/** Adds an empty tile where there is room; says whether there was. */
export function addTile(): boolean {
  const current = draft();
  const cell = current && roomFor(current.tiles);
  if (!current || !cell) return false;
  draft.set({ ...current, tiles: [...current.tiles, tileOf(cell, null)] });
  return true;
}

/** Takes the tile keyed `key` out, with its camera; the last one stays, as a view needs one. */
export function removeTile(key: number) {
  const current = draft();
  if (!current || current.tiles.length < 2) return;
  draft.set({ ...current, tiles: current.tiles.filter((tile) => tile.key !== key) });
}

/** Lays the draft out as `layout`, tile by tile in order, each keeping the camera of the tile at its place in the view. */
export function startFrom(layout: LayoutInfo) {
  const current = draft();
  if (current) draft.set({ ...current, tiles: layout.cells.map((cell, index) => tileOf(cell, index)) });
}

const sameCell = (one: CellInfo, other: CellInfo) => one.column === other.column && one.row === other.row && one.columnSpan === other.columnSpan && one.rowSpan === other.rowSpan;

/** The layout whose tiles are `cells`, in its order, if one is. */
export function matchingLayout(cells: CellInfo[]): LayoutInfo | undefined {
  return layouts.find((layout) => layout.cells.length === cells.length && layout.cells.every((cell, index) => sameCell(cell, cells[index])));
}

/**
 * Saves the draft to its view and stops editing; resolves with whether it was
 * saved. A draft no different from the view closes with nothing to save; one
 * the library refuses stays open, saying why.
 */
export async function finishEditing(view: ViewInfo): Promise<boolean> {
  const current = draft();
  if (!current || current.view !== view.id) return false;
  const unchanged = current.tiles.length === view.cells.length && current.tiles.every((tile, index) => tile.was === index && sameCell(tile.cell, view.cells[index]));
  if (unchanged) {
    draft.set(null);
    return true;
  }
  try {
    await Views.arrange(
      view.id,
      current.tiles.map((tile) => ({ ...tile.cell, was: tile.was })),
    );
    draft.set(null);
    return true;
  } catch (error) {
    failed(error);
    return false;
  } finally {
    refresh();
  }
}
