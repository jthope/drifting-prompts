// Drags run as pans on the UI thread: nsh tells JS only where a pan starts and ends.
import { type AnimatedNode, type AnimatedValue, batch, computed, createAnimatedValue, keymap, signal, Text, View, type ViewStyle } from "nsh";
import type { ShownStream, ViewInfo } from "ipcamera/native";
import { Button, IconButton, look, notify, OVERLAY, OverlayBar, OverlayChip, OverlaySeparator, OverlayToolbar, solid } from "../../../dp/ui";
import { addTile, cancelEditing, type DraftTile, draft, finishEditing, fitOf, moveTile, removeTile, resizeTile, showingOf } from "./arrange";
import { cameraOf, grid, playedIn, roomOf } from "./model";
import { Picture, pictureZoom } from "./picture";

const percent = (cells: number): `${number}%` => `${(cells / grid.size) * 100}%`;

const CORNER = 28;

const EDGE = 14;

/** A frame's handle; its classes are written out whole for the bundler. */
type Side = { x: -1 | 0 | 1; y: -1 | 0 | 1; name: string; cursor: ViewStyle["cursor"]; layout: string; mark: string };

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

type Grip = { side: Side; x: AnimatedValue; y: AnimatedValue };

function placeOf(side: Side): ViewStyle {
  const deep = (across: number) => (across === 0 ? EDGE : CORNER);
  const x: ViewStyle = side.x === 0 ? { left: CORNER, right: CORNER } : side.x < 0 ? { left: 0, width: deep(side.y) } : { right: 0, width: deep(side.y) };
  const y: ViewStyle = side.y === 0 ? { top: CORNER, bottom: CORNER } : side.y < 0 ? { top: 0, height: deep(side.x) } : { bottom: 0, height: deep(side.x) };
  return { position: "absolute", ...x, ...y };
}

function panOf(grip: Grip): { x?: AnimatedValue; y?: AnimatedValue } {
  if (grip.side.x === 0) return { y: grip.y };
  if (grip.side.y === 0) return { x: grip.x };
  return { x: grip.x, y: grip.y };
}

function kept(pull: AnimatedValue, dir: -1 | 0 | 1, at: number, span: number, size: number): AnimatedNode | 0 {
  if (dir === 0) return 0;
  const room = (dir < 0 ? at : grid.size - at - span) * size;
  const give = (span - grid.smallest) * size;
  const low = dir < 0 ? -room : -give;
  // `interpolate` needs a range of at least a point, for an edge that can't move.
  const high = Math.max(dir < 0 ? give : room, low + 1);
  return pull.interpolate({ inputRange: [low, high], outputRange: [low, high] });
}

function GridLines() {
  const lines = Array.from({ length: grid.size - 1 }, (_, index) => index + 1);
  return (
    <>
      <Each of={lines} key={(line) => `column:${line}`}>
        {(line) => <View class="absolute bottom-0 top-0 w-px bg-foreground/10" style={{ left: percent(line()) }} />}
      </Each>
      <Each of={lines} key={(line) => `row:${line}`}>
        {(line) => <View class="absolute left-0 right-0 h-px bg-foreground/10" style={{ top: percent(line()) }} />}
      </Each>
    </>
  );
}

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
  const pulled = signal<Grip | null>(null);
  // Never zoomed: the frame takes the drags and the wheel.
  const zoom = pictureZoom();
  const cellWidth = () => Math.max(1, props.stage.width / grid.size);
  const cellHeight = () => Math.max(1, props.stage.height / grid.size);
  const cell = () => props.tile.cell;
  const label = () => `Tile ${props.number}, ${props.name}`;
  const room = () => roomOf(cell());
  const outline = () => ({
    width: Math.max(1, cell().columnSpan * cellWidth() - room().paddingLeft - room().paddingRight),
    height: Math.max(1, cell().rowSpan * cellHeight() - room().paddingTop - room().paddingBottom),
  });
  const reach = (grip: Grip) => ({
    x: kept(grip.x, grip.side.x, cell().column, cell().columnSpan, cellWidth()),
    y: kept(grip.y, grip.side.y, cell().row, cell().rowSpan, cellHeight()),
  });
  const handleStyle = (grip: Grip): ViewStyle => {
    const along = reach(grip);
    return { ...placeOf(grip.side), cursor: grip.side.cursor, transform: [{ translateX: along.x }, { translateY: along.y }] };
  };
  /** Scaled from its middle and moved by half the growth, so the edges across from the handle stay put. */
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
  /** Resets the drags in the same batch as the draft moves the frame, so it lands without a jump. */
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
        ...room(),
        left: percent(cell().column),
        top: percent(cell().row),
        width: percent(cell().columnSpan),
        height: percent(cell().rowSpan),
        zIndex: moving() || pulled() ? 1 : 0,
        transform: [{ translateX: move.x }, { translateY: move.y }],
      }}
    >
      <View class={`flex-1 overflow-hidden rounded-lg ${props.shows ? "bg-black" : ""}`} style={props.shows ? {} : { backgroundImage: solid(look().tiers.document) }}>
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
      </View>

      <If when={pulled()}>
        {(grip) => (
          <View
            class={`absolute ${OVERLAY.arrange.ghost}`}
            style={{ left: room().paddingLeft, top: room().paddingTop, width: outline().width, height: outline().height, pointerEvents: "none", transform: stretched(grip()) }}
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

      <OverlayBar class="absolute right-3.5 top-3.5">
        <IconButton kind="overlay" icon="x" size="small" label={`Remove tile ${props.number}`} disabled={props.only} onPress={() => removeTile(props.tile.key)} />
      </OverlayBar>
    </View>
  );
}

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
      <OverlayChip>
        <Text class={`text-xs ${OVERLAY.arrange.hint}`} style={{ numberOfLines: 1 }}>
          Drag a tile to move it, or an edge or corner to resize it.
        </Text>
      </OverlayChip>
    </View>
  );
}

/** The stage while `view`'s layout is edited: the draft's tiles as frames on the grid. */
export function LayoutEditor(props: { view: ViewInfo }) {
  const stage = signal({ width: 0, height: 0 });
  const tiles = () => draft()?.tiles ?? [];
  const played = computed(() => playedIn(props.view));
  const numbered = () =>
    tiles().map((tile, index) => {
      const shows = showingOf(props.view, tile.was, played());
      return { tile, number: index + 1, shows, fit: fitOf(props.view, tile.was), name: (shows && cameraOf(shows.camera)?.name) ?? "Empty" };
    });

  return (
    <View
      accessibilityLabel={`Editing the layout of ${props.view.name}`}
      class="flex-1"
      style={{ backgroundImage: solid(look().tiers.frame) }}
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
