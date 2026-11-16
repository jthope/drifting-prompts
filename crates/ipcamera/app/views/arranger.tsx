// The layout editor, in the stage's place while a view's layout is edited
// (`arrange.ts`): the grid's lines over black, and each tile as a frame over
// its camera's live picture, named for the camera. A frame is dragged to move
// it, and by any of its edges or corners to resize it, the edges across from
// that handle staying put. It follows the pointer in the same frame, its
// picture with it, as a pan drives its transform on the UI thread with no JS
// while it moves (nsh's pans tell JS only where they start and end), and lands
// on the nearest grid cells when let go, or goes back where it was when it
// would cover another tile. A resize shows the outline the tile is being
// stretched to, which grows the same way, as far as the grid's edges and down
// to the smallest size; the picture takes the new size when it is let go. A
// bar at the top adds a tile, and closes the editor: Done saves (Return),
// Cancel drops the changes (Escape).
import { type AnimatedNode, type AnimatedValue, batch, computed, createAnimatedValue, keymap, signal, Text, View, type ViewStyle } from "nsh";
import type { ShownStream, ViewInfo } from "ipcamera/native";
import { Button, IconButton, notify, OVERLAY, OverlayBar, OverlayChip, OverlaySeparator, OverlayToolbar } from "../../../dp/ui";
import { addTile, cancelEditing, type DraftTile, draft, finishEditing, fitOf, moveTile, removeTile, resizeTile, showingOf } from "./arrange";
import { cameraOf, grid, playedIn } from "./model";
import { Picture, pictureZoom } from "./picture";

const percent = (cells: number): `${number}%` => `${(cells / grid.size) * 100}%`;

/** Room around a frame inside its cells, so neighbors stand apart and each frame's handles are its own, where the stage's tiles meet. */
const INSET = 3;

/** A corner handle's square, in points from the corner of the tile's box. */
const CORNER = 28;

/** How far an edge's handle reaches in from the edge, in points; inside it, a drag moves the tile. */
const EDGE = 14;

/**
 * A handle on a frame: which of the tile's edges a drag on it moves along each
 * axis (-1 the left or top one, 1 the right or bottom one, 0 neither), what a
 * screen reader calls it, its cursor, and its mark, a corner's L or an edge's
 * bar 6 points in from the tile's box (3 inside its frame): the mark's classes
 * and how the handle lays it out, written out whole for the bundler.
 */
type Side = { x: -1 | 0 | 1; y: -1 | 0 | 1; name: string; cursor: ViewStyle["cursor"]; layout: string; mark: string };

/** A frame's eight handles, clockwise from its top left corner. */
const SIDES: Side[] = [
  { x: -1, y: -1, name: "top left corner", cursor: "nwse-resize", layout: "", mark: "absolute left-1.5 top-1.5 size-2.5 border-l-2 border-t-2" },
  { x: 0, y: -1, name: "top edge", cursor: "ns-resize", layout: "items-center pt-1.5", mark: "h-[3px] w-[18px]" },
  { x: 1, y: -1, name: "top right corner", cursor: "nesw-resize", layout: "", mark: "absolute right-1.5 top-1.5 size-2.5 border-r-2 border-t-2" },
  { x: 1, y: 0, name: "right edge", cursor: "ew-resize", layout: "items-end justify-center pr-1.5", mark: "h-[18px] w-[3px]" },
  { x: 1, y: 1, name: "bottom right corner", cursor: "nwse-resize", layout: "", mark: "absolute bottom-1.5 right-1.5 size-2.5 border-b-2 border-r-2" },
  { x: 0, y: 1, name: "bottom edge", cursor: "ns-resize", layout: "items-center justify-end pb-1.5", mark: "h-[3px] w-[18px]" },
  { x: -1, y: 1, name: "bottom left corner", cursor: "nesw-resize", layout: "", mark: "absolute bottom-1.5 left-1.5 size-2.5 border-b-2 border-l-2" },
  { x: -1, y: 0, name: "left edge", cursor: "ew-resize", layout: "justify-center pl-1.5", mark: "h-[18px] w-[3px]" },
];

/** A handle and its drag, in points along each axis, which nsh's pan drives on the UI thread. */
type Grip = { side: Side; x: AnimatedValue; y: AnimatedValue };

/** Where `side`'s handle sits in its tile's box: a corner's square, or the band along an edge between the corners. */
function placeOf(side: Side): ViewStyle {
  const deep = (across: number) => (across === 0 ? EDGE : CORNER);
  const x: ViewStyle = side.x === 0 ? { left: CORNER, right: CORNER } : side.x < 0 ? { left: 0, width: deep(side.y) } : { right: 0, width: deep(side.y) };
  const y: ViewStyle = side.y === 0 ? { top: CORNER, bottom: CORNER } : side.y < 0 ? { top: 0, height: deep(side.x) } : { bottom: 0, height: deep(side.x) };
  return { position: "absolute", ...x, ...y };
}

/** What a drag on `grip` drives: an edge's handle only the axis across its edge, so it can't drift along it. */
function panOf(grip: Grip): { x?: AnimatedValue; y?: AnimatedValue } {
  if (grip.side.x === 0) return { y: grip.y };
  if (grip.side.y === 0) return { x: grip.x };
  return { x: grip.x, y: grip.y };
}

/**
 * `pull`, a drag along one axis in points, kept to how far it can move the
 * edge `dir` picks (-1 the start one, 1 the end one) of a tile at `at` that
 * spans `span` cells of `size` points: to the grid's edge one way, to the
 * smallest size the other. Nothing (0) for an axis the handle doesn't move.
 */
function kept(pull: AnimatedValue, dir: -1 | 0 | 1, at: number, span: number, size: number): AnimatedNode | 0 {
  if (dir === 0) return 0;
  const room = (dir < 0 ? at : grid.size - at - span) * size;
  const give = (span - grid.smallest) * size;
  const low = dir < 0 ? -room : -give;
  // A point of range at least, as `interpolate` needs one, for an edge that can't move either way.
  const high = Math.max(dir < 0 ? give : room, low + 1);
  return pull.interpolate({ inputRange: [low, high], outputRange: [low, high] });
}

/** The grid's inner lines, one at every column and row between its edges. */
function GridLines() {
  const lines = Array.from({ length: grid.size - 1 }, (_, index) => index + 1);
  return (
    <>
      <Each of={lines} key={(line) => `column:${line}`}>
        {(line) => <View class={`absolute bottom-0 top-0 w-px ${OVERLAY.arrange.line}`} style={{ left: percent(line()) }} />}
      </Each>
      <Each of={lines} key={(line) => `row:${line}`}>
        {(line) => <View class={`absolute left-0 right-0 h-px ${OVERLAY.arrange.line}`} style={{ top: percent(line()) }} />}
      </Each>
    </>
  );
}

/**
 * One tile of the draft: its frame, over the live picture of what it `shows`
 * (with the tile's `fit`), which a drag moves; its handles on every corner and
 * edge, which a drag resizes it by; and its ×, on a dark chip so it shows on a
 * bright picture, which takes it out (not the last one). `stage` is the stage's
 * size in points, which turns a drag's distance into grid cells.
 */
function Frame(props: {
  tile: DraftTile;
  number: number;
  name: string;
  shows: ShownStream | undefined;
  fit: string;
  only: boolean;
  stage: { width: number; height: number };
}) {
  const move = { x: createAnimatedValue(0), y: createAnimatedValue(0) };
  const grips: Grip[] = SIDES.map((side) => ({ side, x: createAnimatedValue(0), y: createAnimatedValue(0) }));
  const moving = signal(false);
  /** The handle being dragged, while one is. */
  const pulled = signal<Grip | null>(null);
  // The editor's pictures aren't zoomed: the frame takes the drags, and the wheel never reaches them.
  const zoom = pictureZoom();
  const cellWidth = () => Math.max(1, props.stage.width / grid.size);
  const cellHeight = () => Math.max(1, props.stage.height / grid.size);
  const cell = () => props.tile.cell;
  const label = () => `Tile ${props.number}, ${props.name}`;
  /** The outline's size before it grows, in points: the frame's. */
  const outline = () => ({ width: Math.max(1, cell().columnSpan * cellWidth() - 2 * INSET), height: Math.max(1, cell().rowSpan * cellHeight() - 2 * INSET) });
  /** How far `grip`'s drag has moved its edges along each axis, in points, kept to where they can go. */
  const reach = (grip: Grip) => ({
    x: kept(grip.x, grip.side.x, cell().column, cell().columnSpan, cellWidth()),
    y: kept(grip.y, grip.side.y, cell().row, cell().rowSpan, cellHeight()),
  });
  /** Where `grip`'s handle is: its place, and while it is dragged, as far along as its edges have moved. */
  const handleStyle = (grip: Grip): ViewStyle => {
    const along = reach(grip);
    return { ...placeOf(grip.side), cursor: grip.side.cursor, transform: [{ translateX: along.x }, { translateY: along.y }] };
  };
  /** The outline `grip` stretches: scaled from its middle, then moved by half the growth, so the edges across from the handle stay put. */
  const stretched = (grip: Grip): ViewStyle["transform"] => {
    const along = reach(grip);
    const half = (node: AnimatedNode | 0) => (node === 0 ? 0 : node.mul(0.5));
    const scale = (node: AnimatedNode | 0, dir: number, size: number) => (node === 0 ? 1 : node.mul(dir / size).add(1));
    return [
      { translateX: half(along.x) },
      { translateY: half(along.y) },
      { scaleX: scale(along.x, grip.side.x, outline().width) },
      { scaleY: scale(along.y, grip.side.y, outline().height) },
    ];
  };
  /** Moves the edges `side` drags by the whole cells nearest to `by`, a drag in points; an axis it doesn't drag stays. */
  const resize = (side: Side, by: { x: number; y: number }) => {
    const columns = Math.round(by.x / cellWidth());
    const rows = Math.round(by.y / cellHeight());
    resizeTile(props.tile.key, {
      left: side.x < 0 ? columns : 0,
      right: side.x > 0 ? columns : 0,
      top: side.y < 0 ? rows : 0,
      bottom: side.y > 0 ? rows : 0,
    });
  };
  /** Back to no drag, in the same turn as the draft moves the frame, so it lands without a jump. */
  const settle = () =>
    batch(() => {
      move.x.set(0);
      move.y.set(0);
      for (const grip of grips) {
        grip.x.set(0);
        grip.y.set(0);
      }
      moving.set(false);
      pulled.set(null);
    });

  return (
    <View
      style={{
        position: "absolute",
        padding: INSET,
        left: percent(cell().column),
        top: percent(cell().row),
        width: percent(cell().columnSpan),
        height: percent(cell().rowSpan),
        zIndex: moving() || pulled() ? 1 : 0,
        transform: [{ translateX: move.x }, { translateY: move.y }],
      }}
    >
      <View
        accessibilityLabel={label()}
        class={`flex-1 items-center justify-center overflow-hidden p-3 ${moving() ? OVERLAY.arrange.lifted : OVERLAY.arrange.frame}`}
        style={{ cursor: moving() ? "grabbing" : "grab" }}
        pan={move}
        onPanStart={() => moving.set(true)}
        onPanEnd={(event) => {
          batch(() => {
            if (!event.cancelled) moveTile(props.tile.key, Math.round(event.translation.x / cellWidth()), Math.round(event.translation.y / cellHeight()));
            settle();
          });
        }}
      >
        <If when={props.shows}>
          {(shown) => (
            <View class="absolute inset-0" style={{ pointerEvents: "none" }}>
              <Picture camera={shown().camera} stream={shown().stream} fit={props.fit} zoom={zoom} />
            </View>
          )}
        </If>
        <OverlayChip class="max-w-full">
          <Text class="text-sm font-medium text-white" style={{ numberOfLines: 1 }}>
            {props.name}
          </Text>
          <Text class={`text-xs ${OVERLAY.arrange.hint}`} style={{ numberOfLines: 1, fontVariant: ["tabular-nums"] }}>
            {`${cell().columnSpan} × ${cell().rowSpan}`}
          </Text>
        </OverlayChip>
      </View>

      <If when={pulled()}>
        {(grip) => (
          <View
            class={`absolute ${OVERLAY.arrange.ghost}`}
            style={{ left: INSET, top: INSET, width: outline().width, height: outline().height, pointerEvents: "none", transform: stretched(grip()) }}
          />
        )}
      </If>

      <Each of={grips} key={(grip) => grip.side.name}>
        {(grip) => (
          <View
            accessibilityLabel={`Resize tile ${props.number} from its ${grip().side.name}`}
            class={`group ${grip().side.layout}`}
            style={handleStyle(grip())}
            pan={panOf(grip())}
            onPanStart={() => pulled.set(grip())}
            onPanEnd={(event) => {
              batch(() => {
                if (!event.cancelled) resize(grip().side, event.translation);
                settle();
              });
            }}
          >
            <View
              class={`${grip().side.mark} ${grip().side.x !== 0 && grip().side.y !== 0 ? OVERLAY.arrange.grip.corner : OVERLAY.arrange.grip.edge}`}
              style={{ filter: OVERLAY.arrange.grip.shade, pointerEvents: "none" }}
            />
          </View>
        )}
      </Each>

      {/* Past the top right corner's mark, over its handle, which keeps the rest of its square. */}
      <OverlayBar class="absolute right-3.5 top-3.5">
        <IconButton kind="overlay" icon="x" size="small" label={`Remove tile ${props.number}`} disabled={props.only} onPress={() => removeTile(props.tile.key)} />
      </OverlayBar>
    </View>
  );
}

/** The editor's bar, over the top of the stage: add a tile, then cancel or save. Escape cancels and Return saves while it shows. */
function EditorBar(props: { view: ViewInfo }) {
  const save = () => void finishEditing(props.view);
  const add = () => {
    if (!addTile()) notify("There's no room for another tile. Make one smaller or remove one first.");
  };
  keymap.bind("escape", cancelEditing, { when: "notEditing" });
  keymap.bind("enter", save, { when: "notEditing" });
  return (
    <View class="absolute left-0 right-0 top-3 items-center gap-1.5 px-3" style={{ pointerEvents: "box-none" }}>
      <OverlayToolbar label="Layout editor">
        <Button kind="overlay" icon="plus" label="Add tile" onPress={add} />
        <OverlaySeparator />
        <Button kind="overlay" label="Cancel" onPress={cancelEditing} />
        <Button kind="primary" label="Done" onPress={save} />
      </OverlayToolbar>
      <Text class={`text-xs ${OVERLAY.arrange.hint}`} style={{ numberOfLines: 1 }}>
        Drag a tile to move it, or an edge or corner to resize it.
      </Text>
    </View>
  );
}

/**
 * The stage while `view`'s layout is edited: the draft's tiles as frames on
 * the grid, each over the live picture of the camera it keeps and named for
 * it, `Empty` for none. Leaving the view drops the draft, as Cancel does.
 * Its black backdrop fills the stage, which the video's island clips to its
 * rounded corners.
 */
export function LayoutEditor(props: { view: ViewInfo }) {
  const stage = signal({ width: 0, height: 0 });
  const tiles = () => draft()?.tiles ?? [];
  // Read once for every frame, as the stage played them.
  const played = computed(() => playedIn(props.view));
  const numbered = () =>
    tiles().map((tile, index) => {
      const shows = showingOf(props.view, tile.was, played());
      return { tile, number: index + 1, shows, fit: fitOf(props.view, tile.was), name: (shows && cameraOf(shows.camera)?.name) ?? "Empty" };
    });

  return (
    <View
      accessibilityLabel={`Editing the layout of ${props.view.name}`}
      class="flex-1 bg-black"
      onLayout={(event) => stage.set({ width: event.bounds.size.width, height: event.bounds.size.height })}
    >
      <GridLines />
      <Each of={numbered()} key={(each) => String(each.tile.key)}>
        {(each) => (
          <Frame
            tile={each().tile}
            number={each().number}
            name={each().name}
            shows={each().shows}
            fit={each().fit}
            only={tiles().length === 1}
            stage={stage()}
          />
        )}
      </Each>
      <EditorBar view={props.view} />
    </View>
  );
}
