import { effect, type JSX, platform, signal, Text, View, type ViewStyle } from "nsh";
import { Cameras, type CameraState, type CellInfo, type RunningStream, type ViewInfo } from "ipcamera/native";
import {
  Appear,
  Button,
  EmptyState,
  Frame,
  Icon,
  IconButton,
  type IconName,
  look,
  type MenuEntries,
  MenuOpener,
  OverlayChip,
  OverlaySeparator,
  OverlayToolbar,
  PictureNotice,
  RecMark,
  solid,
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
import { Picture, type PictureZoom, pictureZoom, type Size } from "./picture";
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
  roomOf,
  selectedTile,
  show,
  streamName,
  tilePlace,
  zoomedTile,
} from "./model";

const percent = (part: number): `${number}%` => `${part * 100}%`;

const NOTICE_ICONS: Record<PictureNoticeInfo["kind"], IconName> = { off: "video-off", reconnecting: "wifi-off", stopped: "triangle-alert", refused: "key-round" };

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

function Badges(props: { camera: CameraState; stream: StreamPick; zoom: number; compact?: boolean; narrow?: boolean }) {
  const stream = () => streamName(props.camera.id, props.stream.current);
  const running = () => props.camera.streams.find((each) => each.stream === props.stream.played);
  const numbers = () => {
    const shown = running();
    if (!shown) return "";
    if (shown.waiting) return `Waiting for ${streamLabel(props.camera.id, shown.stream)}…`;
    return props.narrow ? shown.brief : shown.stats;
  };
  const dim = "text-xs text-neutral-300";
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

const FULL_TOOLBAR = 340;

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

function reportDrawn(place: () => string, size: () => { width: number; height: number } | undefined, zoom: PictureZoom, fit: () => string) {
  effect(() => {
    const laid = size();
    if (laid) drawnAt(place(), laid, zoom.scale(), fit());
  });
}

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

/** Where a picture of `shape` shows in `room`, centered as `src/video.rs`'s `contain` places it; all of `room` while either is unknown. */
function fitted(room: Size | undefined, shape: Size | null): ViewStyle {
  if (!room || !shape) return { left: 0, top: 0, right: 0, bottom: 0 };
  const scale = Math.min(room.width / shape.width, room.height / shape.height);
  const width = shape.width * scale;
  const height = shape.height * scale;
  return { left: (room.width - width) / 2, top: (room.height - height) / 2, width, height };
}

function CameraStage(props: { camera: CameraState; stream: string }) {
  const toolbar = toolbarState(() => true);
  const zoom = pictureZoom();
  const room = signal<Size | undefined>(undefined);
  const shape = signal<Size | null>(null);
  const menu = () => cameraPageMenu(props.camera.id, props.stream);
  const fit = () => pageFit(props.camera.id);
  const stream = () => pageStream(props.camera.id, props.stream);
  reportDrawn(() => pagePlace(props.camera.id), room, zoom, () => fit().fit);
  return (
    <Frame class="flex-1">
      <View class="flex-1" onLayout={(event) => room.set(event.bounds.size)}>
        <View
          accessibilityLabel={`Live picture of ${props.camera.name}`}
          class="absolute overflow-hidden rounded-lg bg-black"
          style={fitted(room(), shape())}
          onContextMenu={(event) => popup(menu(), event.pagePosition)}
          onMouseEnter={() => toolbar.hovered.set(true)}
          onMouseLeave={() => toolbar.hovered.set(false)}
          onLayout={(event) => toolbar.size.set(event.bounds.size)}
        >
          {/* On Auto, nothing plays until the picture has a size to pick a stream by. */}
          <If when={stream().played !== undefined}>
            <Picture camera={props.camera.id} stream={stream().played ?? ""} fit={fit().fit} zoom={zoom} onShape={(next) => shape.set(next)} />
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
      </View>
    </Frame>
  );
}

function menuOfTile(view: ViewInfo, index: number): MenuEntries {
  return [...tileMenu(view, index), { type: "separator" as const }, { label: "Edit layout…", onSelect: () => startEditing(view) }];
}

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
  const room = signal<Size | undefined>(undefined);
  const shape = signal<Size | null>(null);
  reportDrawn(() => tilePlace(props.view.id, props.index), room, zoom, () => fit().fit);

  const place = () =>
    zoomed()
      ? { left: 0, top: 0, width: "100%" as const, height: "100%" as const }
      : {
          left: percent(props.cell.column / grid.size),
          top: percent(props.cell.row / grid.size),
          width: percent(props.cell.columnSpan / grid.size),
          height: percent(props.cell.rowSpan / grid.size),
        };
  return (
    <If when={!hidden()}>
      <View style={{ position: "absolute", ...place(), ...roomOf(props.cell, zoomed()) }}>
        {/* Absolute views ignore padding, so the picture's room is a view of its own inside the gap. */}
        <View class="flex-1" onLayout={(event) => room.set(event.bounds.size)}>
          <View
            role="button"
            accessibilityLabel={`Tile ${number()}`}
            accessibilityState={{ selected: selected() }}
            class={`absolute overflow-hidden rounded-lg ${camera() ? "bg-black" : ""}`}
            style={{ ...fitted(room(), camera() ? shape() : null), ...(camera() ? {} : { backgroundImage: solid(look().tiers.document) }) }}
            onPress={(event) => {
              // A double click's second press selects it again, after its first unselected a selected tile.
              if (event.clickCount === 2) {
                selectedTile.set(props.index);
                zoomedTile.set(zoomed() ? null : props.index);
              } else selectedTile.set(selected() ? null : props.index);
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
                    <Picture camera={shown().id} stream={stream().played ?? ""} fit={fit().fit} zoom={zoom} onShape={(next) => shape.set(next)} />
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

            <If when={selected()}>
              <View class="absolute inset-0 rounded-lg border-2 border-ring" style={{ pointerEvents: "none" }} />
            </If>
          </View>
        </View>
      </View>
    </If>
  );
}

function ViewStage(props: { view: ViewInfo }) {
  const editing = () => draft()?.view === props.view.id;
  // Keyed by position, so a tile's new camera or place updates it in place.
  const tiles = () => props.view.cells.map((cell, index) => ({ cell, index }));
  return (
    <If when={!editing()} else={<LayoutEditor view={props.view} />}>
      <Frame class="flex-1">
        <Each of={tiles()} key={(tile) => String(tile.index)}>
          {(tile) => <Tile view={props.view} cell={tile().cell} index={tile().index} />}
        </Each>
      </Frame>
    </If>
  );
}

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

/** The video area: the selected view's tiles, or one camera on its own. */
export function Stage() {
  const shownCamera = () => {
    const shown = page();
    return shown.kind === "camera" ? cameraOf(shown.id) : undefined;
  };
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
