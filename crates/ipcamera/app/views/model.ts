import { computed, effect, pixelRatio, signal, untracked } from "nsh";
import {
  Cameras,
  type CameraState,
  type CellInfo,
  type DrawnPicture,
  type GroupInfo,
  Groups,
  Library,
  Preferences,
  type RunningStream,
  Server,
  type ShownStream,
  Storage,
  type ViewInfo,
  Views,
} from "ipcamera/native";

/** Every camera's state, kept up to date from Rust. */
export const states = signal<CameraState[]>(Cameras.states());
export const groups = signal<GroupInfo[]>(Groups.list());
export const views = signal<ViewInfo[]>(Views.list());

/** Bumped on every library change, for computeds that read the library through calls. */
const changes = signal(0);

/** Whether the server wants its password before it answers. */
export const signInNeeded = signal(false);

/** The ways to divide a view and to fit a picture. */
export const layouts = Views.layouts();
export const fits = Views.fits();

/** A fit's title, by its key. */
export const fitTitle = (key: string) => fits.find((fit) => fit.key === key)?.title ?? key;

const pageFits = signal<Record<string, string>>({});

/** How camera `camera` fills the stage on its own. */
export const pageFitOf = (camera: string) => pageFits()[camera] ?? "original";

/** Sets how camera `camera` fills the stage on its own, until the app closes. */
export function setPageFit(camera: string, fit: string) {
  pageFits.set({ ...pageFits(), [camera]: fit });
}

/** What can be done with the folders the snapshots and videos are in. */
export const storage = Storage.abilities();

/** What the window shows; a camera's `stream` is `""` or none for Auto, `own` for its own. */
export type Page =
  | { kind: "view"; id: string }
  | { kind: "camera"; id: string; stream?: string }
  | { kind: "dashboard" }
  | { kind: "cameras" }
  | { kind: "groups" }
  | { kind: "snapshots" }
  | { kind: "videos" }
  | { kind: "events" }
  | { kind: "none" };

const OWN_PAGES = ["dashboard", "cameras", "groups", "snapshots", "videos", "events"] as const;

/** Whether `shown` is one of the app's own pages rather than live video. */
export const replacesStage = (shown: Page) => (OWN_PAGES as readonly string[]).includes(shown.kind);

/** The chart color index (0 for `chart-1`) each kind of thing takes wherever it shows. */
export const CHART = { videos: 0, snapshots: 1, cameras: 2, disk: 3 };

/** The key `settings.json` keeps a page under; `""` for nothing. */
export function pageKey(shown: Page): string {
  if (shown.kind === "camera" && shown.stream) return `camera:${shown.id}${STREAM_MARK}${shown.stream}`;
  if (shown.kind === "view" || shown.kind === "camera") return `${shown.kind}:${shown.id}`;
  return shown.kind === "none" ? "" : shown.kind;
}

/** Separates a camera page's id from its stream: neither can hold a line break. */
const STREAM_MARK = "\n";

/** The page `key` names, unless its view or camera has since been deleted. */
export function pageFrom(key: string): Page | undefined {
  const own = OWN_PAGES.find((kind) => kind === key);
  if (own) return { kind: own };
  const colon = key.indexOf(":");
  const kind = key.slice(0, colon);
  const [id, stream] = key.slice(colon + 1).split(STREAM_MARK);
  if (kind === "view" && untracked(views).some((view) => view.id === id)) return { kind, id };
  if (kind === "camera" && untracked(states).some((camera) => camera.id === id)) return stream ? { kind, id, stream } : { kind, id };
  return undefined;
}

function firstPage(): Page {
  const view = untracked(views)[0];
  if (view) return { kind: "view", id: view.id };
  const camera = untracked(states)[0];
  return camera ? { kind: "camera", id: camera.id } : { kind: "none" };
}

/** The page the window closed on, else the first view. */
export const page = signal<Page>(pageFrom(Preferences.lastPage()) ?? firstPage());
/** The selected tile of the view on the stage, from 0; `null` while none is. */
export const selectedTile = signal<number | null>(null);
/** The tile filling the stage after a double click, if one does. */
export const zoomedTile = signal<number | null>(null);

/** Reads the cameras, groups and views again. */
export function refresh() {
  groups.set(Groups.list());
  views.set(Views.list());
  states.set(Cameras.states());
  changes.set(untracked(changes) + 1);
}

/** Follows Rust's changes, and saves the page each time it changes. */
export function listen() {
  Cameras.onStates((next) => states.set(next));
  Server.onSignInNeeded((needed) => signInNeeded.set(needed));
  Library.onChange(() => {
    groups.set(Groups.list());
    views.set(Views.list());
    changes.set(untracked(changes) + 1);
  });
  // A camera's state changes every second; the computed key saves only when the page changes.
  let saved = pageKey(untracked(page));
  const key = computed(() => pageKey(page()));
  effect(() => {
    if (key() === saved) return;
    saved = key();
    Preferences.savePage(saved);
  });
}

export function cameraOf(id: string): CameraState | undefined {
  return states().find((state) => state.id === id);
}

/** The grid every view's tiles sit on, and the smallest a tile is. */
export const grid = Views.grid();

/** The room between two tiles of a view, in points. */
export const TILE_GAP = 2;

/** The padding a tile at `cell` keeps inside its cells; none when it `fills` the stage. */
export function roomOf(cell: CellInfo, fills = false): { paddingLeft: number; paddingTop: number; paddingRight: number; paddingBottom: number } {
  const half = fills ? 0 : TILE_GAP / 2;
  return {
    paddingLeft: cell.column === 0 ? 0 : half,
    paddingTop: cell.row === 0 ? 0 : half,
    paddingRight: cell.column + cell.columnSpan === grid.size ? 0 : half,
    paddingBottom: cell.row + cell.rowSpan === grid.size ? 0 : half,
  };
}

/** The view on the stage, if the stage shows one. */
export const currentView = computed(() => {
  const shown = page();
  return shown.kind === "view" ? views().find((view) => view.id === shown.id) : undefined;
});

/** The camera the header, PTZ and audio follow: the one on its own or in the selected tile; `""` for none. */
export const focusedId = computed(() => {
  const shown = page();
  if (shown.kind === "camera") return shown.id;
  const tile = selectedTile();
  return tile === null ? "" : (currentView()?.tiles[tile] ?? "");
});

/** Where tile `index` of view `view` is, as Auto keeps the size it is drawn at. */
export const tilePlace = (view: string, index: number) => `view:${view}:${index}`;

/** Where camera `camera` on its own is, as Auto keeps the size it is drawn at. */
export const pagePlace = (camera: string) => `camera:${camera}`;

/** Each picture's last drawn size by place; kept across pages so the last view's tiles keep their streams. */
const drawn = signal<Record<string, DrawnPicture>>({});

const SETTLE_MS = 500;
const settling = new Map<string, number>();

/** Records the picture at `place` drawn at `size`; a change counts once it holds for `SETTLE_MS`, so a drag doesn't switch streams. */
export function drawnAt(place: string, size: { width: number; height: number }, zoom: number, fit: string) {
  const scale = pixelRatio() * zoom;
  const picture = { width: Math.round(size.width * scale), height: Math.round(size.height * scale), fit };
  if (picture.width <= 0 || picture.height <= 0) return;
  clearTimeout(settling.get(place));
  settling.delete(place);
  const before = untracked(drawn)[place];
  if (before && before.width === picture.width && before.height === picture.height && before.fit === picture.fit) return;
  const keep = () => drawn.set({ ...untracked(drawn), [place]: picture });
  if (!before) return keep();
  settling.set(
    place,
    setTimeout(() => {
      settling.delete(place);
      keep();
    }, SETTLE_MS),
  );
}

/** The stream a picture of `camera` picking `saved` plays; `undefined` while Auto has nothing drawn to pick for. */
export function playedStream(camera: string, saved: string, pictures: DrawnPicture[]): string | undefined {
  changes();
  if (saved !== "") return Cameras.choiceKey(camera, saved);
  if (!Cameras.autoPicks(camera)) return "";
  return pictures.length > 0 ? Cameras.autoStream(camera, pictures) : undefined;
}

/** The stream each of `view`'s tiles plays; a camera's Auto tiles share the one its largest calls for. */
export function playedIn(view: ViewInfo): (string | undefined)[] {
  const sizes = drawn();
  const onAuto = (camera: string) =>
    view.tiles.flatMap((each, index) => {
      const picture = each === camera && (view.streams[index] ?? "") === "" ? sizes[tilePlace(view.id, index)] : undefined;
      return picture ? [picture] : [];
    });
  return view.tiles.map((camera, index) => (camera === "" ? undefined : playedStream(camera, view.streams[index] ?? "", onAuto(camera))));
}

/** The stream camera `camera` plays on its own, picking `saved`. */
export function playedOnItsOwn(camera: string, saved: string): string | undefined {
  const picture = drawn()[pagePlace(camera)];
  return playedStream(camera, saved, picture ? [picture] : []);
}

/** The stream each tile of the view on the stage plays. */
export const playedOnStage = computed(() => {
  const view = currentView();
  return view ? playedIn(view) : [];
});

/** The stream the focused picture plays; `""` for the camera's own, before Auto picks, or with no tile selected. */
export const focusedStream = computed(() => {
  const shown = page();
  if (shown.kind === "camera") return playedOnItsOwn(shown.id, shown.stream ?? "") ?? "";
  const tile = selectedTile();
  if (shown.kind !== "view" || tile === null) return "";
  return playedOnStage()[tile] ?? "";
});

/** What the menus call stream `key` of `camera`; `""` for Auto and for the camera's own. */
export function streamName(camera: string, key: string): string {
  const shown = key === "" ? "" : Cameras.choiceKey(camera, key);
  if (shown === "") return "";
  return Cameras.streamChoices(camera).find((choice) => choice.key === shown)?.name ?? key;
}

/** `list` with each camera's stream once, in its first place. */
export function eachOnce(list: ShownStream[]): ShownStream[] {
  const seen = new Set<string>();
  return list.filter((shown) => {
    const key = `${shown.camera}\n${shown.stream}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

const playing = (camera: string, stream: string | undefined): ShownStream[] => (stream === undefined ? [] : [{ camera, stream }]);

/** The streams `shown` puts on the stage, each once; none for a picture Auto has yet to pick for. */
export function streamsOn(shown: Page, list: ViewInfo[]): ShownStream[] {
  if (shown.kind === "camera") return playing(shown.id, playedOnItsOwn(shown.id, shown.stream ?? ""));
  if (shown.kind !== "view") return [];
  const view = list.find((view) => view.id === shown.id);
  if (!view) return [];
  const played = playedIn(view);
  return eachOnce(view.tiles.flatMap((camera, index) => (camera === "" ? [] : playing(camera, played[index]))));
}

/** The ids of the cameras `shown` puts on the stage. */
export function camerasOn(shown: Page, list: ViewInfo[]): string[] {
  if (shown.kind === "camera") return [shown.id];
  if (shown.kind !== "view") return [];
  const view = list.find((view) => view.id === shown.id);
  return [...new Set((view?.tiles ?? []).filter((id) => id !== ""))];
}

/** The ids of the cameras on screen. */
export const onScreen = computed(() => camerasOn(page(), views()));

/** The streams on screen, each once. */
export const streamsOnScreen = computed(() => streamsOn(page(), views()));

const lastView = signal<Page>(untracked(page).kind === "view" ? untracked(page) : { kind: "none" });

const openedSince = signal<ShownStream[]>(
  (() => {
    const shown = untracked(page);
    return shown.kind === "camera" ? [{ camera: shown.id, stream: shown.stream ?? "" }] : [];
  })(),
);

/** The cameras that stay connected whatever the page: the last view's, and every one opened on its own since. */
export const kept = computed(() => [...new Set([...camerasOn(lastView(), views()), ...openedSince().map((shown) => shown.camera)])]);

/** The streams of `kept`. */
export const keptStreams = computed(() =>
  eachOnce([...streamsOn(lastView(), views()), ...openedSince().flatMap((shown) => playing(shown.camera, playedOnItsOwn(shown.camera, shown.stream)))]),
);

/** Whether any camera is recording, on screen or not. */
export const recording = computed(() => states().some((camera) => camera.recording));

const DETECTIONS = new Map(Cameras.detections().map((detection) => [detection.key, detection.title]));

/** What a camera's AI told apart, in words: `Person, Vehicle`. */
export const detectionsLabel = (keys: string[]) => keys.map((key) => DETECTIONS.get(key) ?? key).join(", ");

/** What a watched camera sees now, in words; `""` when nothing moves. */
export function activityOf(camera: CameraState): string {
  if (camera.detected.length > 0) return detectionsLabel(camera.detected);
  return camera.motion ? "Motion" : "";
}

/** What covers a camera's picture while it isn't live. */
export type PictureNoticeInfo = {
  kind: "off" | "reconnecting" | "stopped" | "refused";
  title: string;
  text: string;
  detail?: string;
};

const pictureTime = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit", second: "2-digit" });

/** What covers `camera`'s picture on `stream`; nothing while pictures come or while it first connects. */
export function pictureNotice(camera: CameraState, stream: RunningStream | undefined): PictureNoticeInfo | undefined {
  if (!camera.connected) return { kind: "off", title: "Not connected", text: camera.status };
  if (!stream || stream.trouble === "fine") return undefined;
  const pictured = stream.lastPictureAt > 0;
  const detail = pictured ? `Last picture ${pictureTime.format(new Date(stream.lastPictureAt))}` : undefined;
  switch (stream.trouble) {
    case "reconnecting":
      return { kind: "reconnecting", title: pictured ? "Connection lost" : "Can't reach the camera", text: stream.problem, detail };
    case "stopped":
      return { kind: "stopped", title: "Stream stopped", text: stream.problem, detail };
    case "refused":
      return { kind: "refused", title: "Login refused", text: stream.problem, detail };
  }
}

/** Shows a page, with no tile selected. */
export function show(next: Page) {
  if (next.kind === "view") {
    lastView.set(next);
    openedSince.set([]);
  } else if (next.kind === "camera") {
    const stream = next.stream ?? "";
    const opened = untracked(openedSince);
    if (!opened.some((shown) => shown.camera === next.id && shown.stream === stream)) {
      openedSince.set([...opened.filter((shown) => shown.camera !== next.id), { camera: next.id, stream }]);
    }
  }
  page.set(next);
  selectedTile.set(null);
  zoomedTile.set(null);
}

/** What the window's title calls the page. */
export const pageName = computed(() => {
  const shown = page();
  if (shown.kind === "view") return currentView()?.name ?? "";
  if (shown.kind === "camera") return cameraOf(shown.id)?.name ?? "";
  if (shown.kind === "dashboard") return "Dashboard";
  if (shown.kind === "cameras") return "Cameras";
  if (shown.kind === "groups") return "Groups";
  if (shown.kind === "snapshots") return "Snapshots";
  if (shown.kind === "videos") return "Videos";
  if (shown.kind === "events") return "Events";
  return "";
});
