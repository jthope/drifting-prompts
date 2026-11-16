import { effect, type JSX, pixelRatio, platform, signal, Text, View } from "nsh";
import { Cameras, type CameraState, type CellInfo, type RunningStream, type ViewInfo } from "ipcamera/native";
import {
  Appear,
  Button,
  EmptyState,
  Icon,
  IconButton,
  type IconName,
  type MenuEntries,
  MenuOpener,
  OverlayChip,
  OverlaySeparator,
  OverlayToolbar,
  PictureNotice,
  RecMark,
  StatusDot,
} from "../../../dp/ui";
import {
  addToTile,
  cameraPageMenu,
  editCamera,
  type FitChoice,
  fitMenu,
  newCamera,
  pageFit,
  pageStream,
  pickLabel,
  popup,
  type StreamPick,
  setTile,
  streamLabel,
  streamMenu,
  takeSnapshotOf,
  tileFit,
  tileMenu,
  toggleConnection,
  toggleMute,
  toggleRecordingOf,
  toggleSpotlight,
  tryAgain,
} from "./actions";
import { draft, startEditing } from "./arrange";
import { LayoutEditor } from "./arranger";
import { Picture, type PictureZoom, pictureZoom } from "./picture";
import {
  activityOf,
  cameraOf,
  currentView,
  drawnAt,
  fitTitle,
  grid,
  type PictureNoticeInfo,
  page,
  pagePlace,
  pictureNotice,
  playedOnStage,
  selectedTile,
  show,
  streamName,
  tilePlace,
  zoomedTile,
} from "./model";

const percent = (part: number): `${number}%` => `${part * 100}%`;

/** The line between two tiles: one device pixel at any display scaling, the thinnest line the screen draws. */
const hairline = () => 1 / pixelRatio();

/** Which sides of a tile lie along the stage's edge, rather than inside the grid. */
type Edges = { left: boolean; top: boolean; right: boolean; bottom: boolean };

/** The sides of a tile at `cell` along the stage's edge: every side of one that `fills` the stage (a double click's). */
function edgesOf(cell: CellInfo, fills: boolean): Edges {
  if (fills) return { left: true, top: true, right: true, bottom: true };
  return {
    left: cell.column === 0,
    top: cell.row === 0,
    right: cell.column + cell.columnSpan === grid.size,
    bottom: cell.row + cell.rowSpan === grid.size,
  };
}

/** Each notice's icon. */
const NOTICE_ICONS: Record<PictureNoticeInfo["kind"], IconName> = { off: "video-off", reconnecting: "wifi-off", stopped: "triangle-alert", refused: "key-round" };

/**
 * The way forward from a notice: Connect for a camera that isn't connected and
 * may, Try again for a stream that gave up, and the camera's settings for one
 * that needs a password first or whose login was refused. A stream that
 * reconnects by itself needs none.
 */
function NoticeAction(props: { kind: PictureNoticeInfo["kind"]; camera: CameraState }) {
  const settings = () => <Button label="Camera settings" onPress={() => editCamera(props.camera.id)} />;
  return (
    <>
      <If when={props.kind === "off"}>
        {props.camera.ready ? <Button kind="primary" label="Connect" onPress={() => toggleConnection(props.camera)} /> : settings()}
      </If>
      <If when={props.kind === "stopped"}>
        <Button kind="primary" label="Try again" onPress={() => tryAgain(props.camera)} />
      </If>
      <If when={props.kind === "refused"}>{settings()}</If>
    </>
  );
}

/**
 * What covers a camera's picture while it isn't live (`pictureNotice`), and
 * the way forward: over the last picture, dimmed, while its stream lost the
 * camera or gave up. `stream` is the stream the picture plays, while it runs.
 * One that isn't connected says it in a warning's icon when its status is one.
 */
function Notice(props: { camera: CameraState; stream?: RunningStream; compact?: boolean }) {
  return (
    <If when={pictureNotice(props.camera, props.stream)}>
      {(notice) => (
        <PictureNotice
          icon={notice().kind === "off" && props.camera.tone === "warn" ? "triangle-alert" : NOTICE_ICONS[notice().kind]}
          title={notice().title}
          text={notice().text}
          detail={notice().detail}
          action={<NoticeAction kind={notice().kind} camera={props.camera} />}
          compact={props.compact}
        />
      )}
    </If>
  );
}

/**
 * What a watched camera sees now (`Person, Vehicle`, or `Motion`), in a chip
 * at the picture's bottom left; nothing while nothing moves.
 */
function Activity(props: { camera: CameraState }) {
  return (
    <If when={activityOf(props.camera)}>
      {(seen) => (
        <OverlayChip class="absolute bottom-1 left-1">
          <Text class="text-xs text-white" style={{ numberOfLines: 1 }}>
            {seen()}
          </Text>
        </OverlayChip>
      )}
    </If>
  );
}

/**
 * The camera's spotlight switch, on the toolbar over its picture: for a camera
 * that has one (a watched Reolink), on while the light is.
 */
function SpotlightButton(props: { camera: CameraState }) {
  return (
    <If when={props.camera.spotlight}>
      <IconButton
        kind="overlay"
        icon="sun"
        label={props.camera.spotlightOn ? `Turn ${props.camera.name}'s spotlight off` : `Turn ${props.camera.name}'s spotlight on`}
        on={props.camera.spotlightOn}
        tint={props.camera.spotlightOn ? "warning" : undefined}
        onPress={() => toggleSpotlight(props.camera)}
      />
    </If>
  );
}

/**
 * What lies over a camera's picture, the same on its own and in a view's tile:
 * why it isn't live (`Notice`, smaller in a tile, which is `compact`), its name
 * with its status, the stream a tile picks, how far it is zoomed in, whether
 * its spotlight is on and whether it records; under those, where the camera is
 * and what the stream it shows delivers, always there; and what it sees. A
 * `narrow` picture's numbers are its size and frame rate alone. A badge added
 * here shows in both.
 */
function Badges(props: { camera: CameraState; stream: StreamPick; zoom: number; compact?: boolean; narrow?: boolean }) {
  // Named where it picked another than its own: on Auto, its numbers say the size.
  const stream = () => streamName(props.camera.id, props.stream.current);
  const running = () => props.camera.streams.find((each) => each.stream === props.stream.played);
  const numbers = () => {
    const shown = running();
    if (!shown) return "";
    // The picture before it stays up meanwhile (`src/video.rs`), so say what is coming.
    if (shown.waiting) return `Waiting for ${streamLabel(props.camera.id, shown.stream)}…`;
    return props.narrow ? shown.brief : shown.stats;
  };
  const dim = "text-xs text-neutral-300";
  /** The lines under the name, a point smaller than it. */
  const small = { numberOfLines: 1, fontSize: 11 };
  const smallNumbers = { ...small, fontVariant: ["tabular-nums" as const] };
  return (
    <>
      <Notice camera={props.camera} stream={running()} compact={props.compact} />
      <OverlayChip class="absolute left-1 top-1">
        <View class="min-w-0 gap-0.5">
          <View class="flex-row items-center gap-1.5">
            <StatusDot tone={props.camera.tone} />
            <Text class="text-xs text-white" style={{ numberOfLines: 1 }}>
              {props.camera.name}
            </Text>
            <If when={stream()}>
              <Text class={dim} style={{ numberOfLines: 1 }}>
                {stream()}
              </Text>
            </If>
            <If when={props.zoom > 1}>
              <Text class={dim} style={{ numberOfLines: 1, fontVariant: ["tabular-nums"] }}>
                {`${props.zoom.toFixed(1)}×`}
              </Text>
            </If>
            <If when={props.camera.spotlightOn}>
              <Icon name="sun" size={12} tint="warning" />
            </If>
            <If when={props.camera.recording}>
              <RecMark />
            </If>
          </View>
          <If when={props.camera.address}>
            <Text class={dim} style={smallNumbers}>
              {props.camera.address}
            </Text>
          </If>
          <If when={numbers()}>
            <Text class={dim} style={smallNumbers}>
              {numbers()}
            </Text>
          </If>
        </View>
      </OverlayChip>
      <Activity camera={props.camera} />
    </>
  );
}

/** How wide a picture must be, in points, for its toolbar to hold every button; a narrower one keeps the stream and More. */
const FULL_TOOLBAR = 340;

/**
 * Whether a picture's toolbar shows: while the pointer is over the picture
 * where there is one, and where there isn't (a touch screen), while
 * `selected`. `hovered` is set from the picture's `onMouseEnter` and
 * `onMouseLeave`, and `size` from its `onLayout`.
 */
function toolbarState(selected: () => boolean) {
  const hovered = signal(false);
  const size = signal<{ width: number; height: number } | undefined>(undefined);
  return {
    hovered,
    size,
    shown: () => (platform.desktop ? hovered() : selected()),
    compact: () => (size()?.width ?? 0) < FULL_TOOLBAR,
  };
}

/**
 * Tells Auto, from now on, how the picture at `place` (`tilePlace`,
 * `pagePlace`) is drawn: the size its view was laid out at, its zoom and its
 * fit (`drawnAt`), each time one changes.
 */
function reportDrawn(place: () => string, size: () => { width: number; height: number } | undefined, zoom: PictureZoom, fit: () => string) {
  effect(() => {
    const laid = size();
    if (laid) drawnAt(place(), laid, zoom.scale(), fit());
  });
}

/**
 * The toolbar at the bottom of a camera's picture, the same on its own and in
 * a view's tile, fading up while it shows: the stream it is on, by its size,
 * which opens every stream the camera has (`streamMenu`), for a camera with
 * more than one; then its spotlight, a
 * snapshot, recording, Fit, which picks how the picture fills its place, and
 * its sound, `children` (a tile's Open on its own), the whole picture again
 * while `zoom` is zoomed in, then, apart, Connect or Disconnect, and More,
 * whose menu is `menu`'s (`moreLabel` names it). `stream` is the stream the picture picks and plays, and what
 * picking another does; `fit` is how it fills its place, the same way. A
 * narrow picture (`compact`) keeps the stream,
 * the way out of a zoom and More. A button for a camera's picture goes here,
 * so both places get it; the picture's menu (`pictureMenu`) holds what it does
 * too, for the right button, the keyboard and a narrow picture.
 */
function PictureToolbar(props: {
  camera: CameraState;
  stream: StreamPick;
  zoom: PictureZoom;
  fit: FitChoice;
  menu: () => MenuEntries;
  moreLabel: string;
  compact: boolean;
  children?: JSX.Element;
}) {
  const id = () => props.camera.id;
  const name = () => props.camera.name;
  /** Whether the camera has a stream to pick besides the one it shows by default. */
  const choosing = () => Cameras.streamChoices(id()).length > 1;
  return (
    <View class="absolute inset-x-0 bottom-2 items-center" style={{ pointerEvents: "box-none" }}>
      <Appear>
        <OverlayToolbar label={`${name()}'s toolbar`}>
          <If when={choosing()}>
            <MenuOpener>
              {(open) => (
                <Button
                  kind="overlay"
                  icon="layers"
                  label={pickLabel(id(), props.stream)}
                  accessibilityLabel={`${name()}'s stream`}
                  onPress={() => open(streamMenu(id(), props.stream))}
                />
              )}
            </MenuOpener>
          </If>
          <If when={!props.compact}>
            <If when={choosing()}>
              <OverlaySeparator />
            </If>
            <SpotlightButton camera={props.camera} />
            <IconButton kind="overlay" icon="camera" label={`Snapshot of ${name()}`} onPress={() => takeSnapshotOf(id())} />
            <IconButton
              kind="overlay"
              icon={props.camera.recording ? "square" : "circle"}
              label={props.camera.recording ? "Stop recording" : `Record ${name()}`}
              tint={props.camera.recording ? "destructive" : undefined}
              onPress={() => toggleRecordingOf(props.camera)}
            />
            <MenuOpener>
              {(open) => <IconButton kind="overlay" icon="ratio" label={`${name()}'s fit: ${fitTitle(props.fit.fit)}`} onPress={() => open(fitMenu(props.fit))} />}
            </MenuOpener>
            <IconButton
              kind="overlay"
              icon={props.camera.muted ? "volume-x" : "volume-2"}
              label={props.camera.muted ? `Unmute ${name()}` : `Mute ${name()}`}
              tint={props.camera.muted ? "destructive" : undefined}
              onPress={() => toggleMute(props.camera)}
            />
            {props.children}
          </If>
          <If when={props.zoom.scale() > 1}>
            <IconButton kind="overlay" icon="zoom-out" label={`Show all of ${name()}'s picture`} onPress={props.zoom.reset} />
          </If>
          {/* Apart from the rest: it stops the camera in every tile that shows it, not just this one. */}
          <If when={!props.compact}>
            <OverlaySeparator />
            <IconButton
              kind="overlay"
              icon={props.camera.connected ? "unplug" : "plug"}
              label={props.camera.connected ? `Disconnect ${name()}` : `Connect ${name()}`}
              onPress={() => toggleConnection(props.camera)}
            />
          </If>
          <MenuOpener>{(open) => <IconButton kind="overlay" icon="ellipsis" label={props.moreLabel} onPress={() => open(props.menu())} />}</MenuOpener>
        </OverlayToolbar>
      </Appear>
    </View>
  );
}

/**
 * One camera filling the stage, picking `stream` (`""` for Auto), with what a
 * view's tile has over its picture: the same badges, and the same toolbar
 * while the pointer is over it, whose stream picker picks the page's stream as
 * a tile's picks the tile's. Its menu, from the right button or the toolbar's
 * More, holds what the toolbar does. It is black in both appearances, as a
 * picture's surround should be, out to the island's edge and corners.
 */
function CameraStage(props: { camera: CameraState; stream: string }) {
  const toolbar = toolbarState(() => true);
  const zoom = pictureZoom();
  const menu = () => cameraPageMenu(props.camera.id, props.stream);
  const fit = () => pageFit(props.camera.id);
  const stream = () => pageStream(props.camera.id, props.stream);
  reportDrawn(() => pagePlace(props.camera.id), toolbar.size, zoom, () => fit().fit);
  return (
    <View
      accessibilityLabel={`Live picture of ${props.camera.name}`}
      class="flex-1 overflow-hidden bg-black"
      onContextMenu={(event) => popup(menu(), event.pagePosition)}
      onMouseEnter={() => toolbar.hovered.set(true)}
      onMouseLeave={() => toolbar.hovered.set(false)}
      onLayout={(event) => toolbar.size.set(event.bounds.size)}
    >
      {/* On Auto, nothing plays until the picture has a size to pick a stream by. */}
      <If when={stream().played !== undefined}>
        <Picture camera={props.camera.id} stream={stream().played ?? ""} fit={fit().fit} zoom={zoom} />
      </If>
      <Badges camera={props.camera} stream={stream()} zoom={zoom.scale()} narrow={toolbar.compact()} />
      <If when={toolbar.shown()}>
        <PictureToolbar
          camera={props.camera}
          stream={stream()}
          zoom={zoom}
          fit={fit()}
          menu={menu}
          moreLabel={`${props.camera.name}'s picture options`}
          compact={toolbar.compact()}
        />
      </If>
    </View>
  );
}

/** A tile's menu, from the right button or the toolbar's More: what it shows, then the view's Edit layout. */
function menuOfTile(view: ViewInfo, index: number): MenuEntries {
  return [...tileMenu(view, index), { type: "separator" as const }, { label: "Edit layout…", onSelect: () => startEditing(view) }];
}

/**
 * One tile of a view, at its `cell` of the view's grid. A press selects it,
 * which the header, the PTZ panel and the audio follow; a double click fills
 * the stage with it, and another puts it back. While the pointer is over a
 * filled tile (on a touch screen, while it is selected) the picture's toolbar
 * shows, with Open on its own; its menu, from the right button or the
 * toolbar's More, changes what it shows. A tile with a camera is black, as a
 * picture's surround should be; an empty one is on the page's color, as the
 * stage with no cameras is, since it has no picture to surround. The tiles
 * meet edge to edge and are square, the island rounding the stage's outer
 * corners: a hairline in the theme's line color parts two of them, on the left
 * or top of the tile right of it or below it, so each line between two tiles is
 * drawn once; along the stage's edge, the island's own edge is there. The
 * selected one has no mark of its own: the header names its camera, and the
 * PTZ panel and the sound follow it.
 */
function Tile(props: { view: ViewInfo; cell: CellInfo; index: number }) {
  const id = () => props.view.tiles[props.index] ?? "";
  const stream = (): StreamPick => ({
    current: props.view.streams[props.index] ?? "",
    played: playedOnStage()[props.index],
    pick: (picked) => setTile(props.view, props.index, id(), picked),
  });
  const fit = () => tileFit(props.view, props.index);
  const camera = () => cameraOf(id());
  const selected = () => selectedTile() === props.index;
  const zoomed = () => zoomedTile() === props.index;
  const hidden = () => zoomedTile() !== null && !zoomed();
  const number = () => props.index + 1;
  const toolbar = toolbarState(selected);
  const zoom = pictureZoom();
  reportDrawn(() => tilePlace(props.view.id, props.index), toolbar.size, zoom, () => fit().fit);

  const place = () =>
    zoomed()
      ? { left: 0, top: 0, width: "100%" as const, height: "100%" as const }
      : {
          left: percent(props.cell.column / grid.size),
          top: percent(props.cell.row / grid.size),
          width: percent(props.cell.columnSpan / grid.size),
          height: percent(props.cell.rowSpan / grid.size),
        };
  const edges = () => edgesOf(props.cell, zoomed());
  /** The line on the tile's left and top where they lie inside the grid; the tile to its left or above it draws none on that side. */
  const lines = () => ({ borderLeftWidth: edges().left ? 0 : hairline(), borderTopWidth: edges().top ? 0 : hairline() });

  return (
    <If when={!hidden()}>
      <View
        role="button"
        accessibilityLabel={`Tile ${number()}`}
        class={`overflow-hidden border-border ${camera() ? "bg-black" : ""}`}
        style={{ position: "absolute", ...place(), ...lines() }}
        onPress={(event) => {
          selectedTile.set(props.index);
          if (event.clickCount === 2) zoomedTile.set(zoomed() ? null : props.index);
        }}
        onContextMenu={(event) => {
          selectedTile.set(props.index);
          popup(menuOfTile(props.view, props.index), event.pagePosition);
        }}
        onMouseEnter={() => toolbar.hovered.set(true)}
        onMouseLeave={() => toolbar.hovered.set(false)}
        onLayout={(event) => toolbar.size.set(event.bounds.size)}
      >
        <If
          when={camera()}
          else={
            <View class="absolute inset-0 items-center justify-center p-2">
              <MenuOpener>
                {(open) => (
                  <Button
                    icon="plus"
                    label="Add camera"
                    accessibilityLabel={`Add a camera to tile ${number()}`}
                    onPress={() => {
                      // The button takes the press from the tile, so it selects the tile itself.
                      selectedTile.set(props.index);
                      addToTile(props.view, props.index, open);
                    }}
                  />
                )}
              </MenuOpener>
            </View>
          }
        >
          {(shown) => (
            <>
              {/* On Auto, nothing plays until the tile has a size to pick a stream by. */}
              <If when={stream().played !== undefined}>
                <Picture camera={shown().id} stream={stream().played ?? ""} fit={fit().fit} zoom={zoom} />
              </If>
              <Badges camera={shown()} stream={stream()} zoom={zoom.scale()} compact narrow={toolbar.compact()} />
            </>
          )}
        </If>

        <If when={toolbar.shown() ? camera() : undefined}>
          {(shown) => (
            <PictureToolbar
              camera={shown()}
              stream={stream()}
              zoom={zoom}
              fit={fit()}
              menu={() => menuOfTile(props.view, props.index)}
              moreLabel={`Tile ${number()} options`}
              compact={toolbar.compact()}
            >
              <IconButton kind="overlay" icon="maximize-2" label={`Open ${shown().name} on its own`} onPress={() => show({ kind: "camera", id: shown().id })} />
            </PictureToolbar>
          )}
        </If>
      </View>
    </If>
  );
}

/**
 * A saved view: its tiles, each showing a camera or offering to; or, while its
 * layout is edited, the editor in their place. Where no tile covers the grid,
 * and in a tile with no camera, the video's island shows.
 */
function ViewStage(props: { view: ViewInfo }) {
  const editing = () => draft()?.view === props.view.id;
  // A row per tile, kept by position, so changing a tile's camera or its place updates it in place.
  const tiles = () => props.view.cells.map((cell, index) => ({ cell, index }));
  return (
    <If when={!editing()} else={<LayoutEditor view={props.view} />}>
      <View class="flex-1">
        <Each of={tiles()} key={(tile) => String(tile.index)}>
          {(tile) => <Tile view={props.view} cell={tile().cell} index={tile().index} />}
        </Each>
      </View>
    </If>
  );
}

/** What the stage shows when there is nothing to show yet: on the video's island, as there is no picture to surround. */
function Empty() {
  return (
    <View class="flex-1">
      <EmptyState
        icon="video"
        title="No cameras yet"
        text="Add a camera to see its live picture here."
        action={<Button kind="primary" icon="plus" label="Add camera" onPress={() => newCamera()} />}
      />
    </View>
  );
}

/**
 * The video area: the selected view's tiles, or one camera on its own, in the
 * video's island, which has no outline (`edgeless`): it fills the island to its
 * edge, and the island clips it to its rounded corners, the live picture with
 * it, as nsh clips any view's content. In full screen the island fills the
 * window, square.
 */
export function Stage() {
  const shownCamera = () => {
    const shown = page();
    return shown.kind === "camera" ? cameraOf(shown.id) : undefined;
  };
  /** The stream the camera on its own picks: `""` for Auto. */
  const shownStream = () => {
    const shown = page();
    return shown.kind === "camera" ? (shown.stream ?? "") : "";
  };
  return (
    <View class="flex-1">
      <If when={currentView()} else={<If when={shownCamera()} else={<Empty />}>{(camera) => <CameraStage camera={camera()} stream={shownStream()} />}</If>}>
        {(view) => <ViewStage view={view()} />}
      </If>
    </View>
  );
}
