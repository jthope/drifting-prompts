// What every part of the window reads: the cameras, groups and views from Rust,
// and what the stage is showing. Like `notify` and the toasts, these are
// module-level signals, so the sidebar, the header and the stage share them
// without passing them down through every layer.
import { computed, effect, pixelRatio, signal, untracked } from "nsh";
import {
  Cameras,
  type CameraState,
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

/** Every camera's state, kept up to date from Rust, cameras added and deleted included. */
export const states = signal<CameraState[]>(Cameras.states());
export const groups = signal<GroupInfo[]>(Groups.list());
export const views = signal<ViewInfo[]>(Views.list());

/** Counts the library's changes, a camera's streams among them, for what reads it through calls rather than these signals. */
const changes = signal(0);

/**
 * Whether the server wants its password before it answers, when the sign-in
 * page has the window in place of the app. Never on the desktop, which has no
 * server.
 */
export const signInNeeded = signal(false);

/** The ways to divide a view and to fit a picture: fixed, so read once. */
export const layouts = Views.layouts();
export const fits = Views.fits();

/** What a fit's menu calls it, by its key: `"fill"` is Fill. */
export const fitTitle = (key: string) => fits.find((fit) => fit.key === key)?.title ?? key;

/**
 * How each camera fills the stage where it shows on its own, by camera id, as
 * its toolbar's Fit picked it: kept until the app closes, as the stream picked
 * there is, and not saved. A view's tiles save theirs (`ViewInfo.fits`).
 */
const pageFits = signal<Record<string, string>>({});

/** How camera `camera` fills the stage on its own: as picked there, else its whole picture in its own shape. */
export const pageFitOf = (camera: string) => pageFits()[camera] ?? "original";

/** Fills the stage with camera `camera` on its own as `fit` says, until the app closes. */
export function setPageFit(camera: string, fit: string) {
  pageFits.set({ ...pageFits(), [camera]: fit });
}

/**
 * What can be done with the folders the snapshots and videos are in: on the
 * desktop they are on this PC; on the web they are the server's, which a
 * browser can neither open nor choose. Fixed, so read once.
 */
export const storage = Storage.abilities();

/**
 * What the window shows: a saved view, one camera on its own, the dashboard,
 * the cameras' table, the groups' table, the snapshots gallery, the recorded
 * videos, what the cameras saw, or nothing when there is no view or camera
 * yet. Settings opens over whichever it is, in a dialog (`showSettings`). A
 * camera on its own shows the stream its `stream` picks, as a tile's does
 * (`ViewInfo.streams`): `""` or none for Auto, `own` for the camera's own.
 */
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

/** The pages that are the app's own, in place of the live video. */
const OWN_PAGES = ["dashboard", "cameras", "groups", "snapshots", "videos", "events"] as const;

/** Whether `page` is one of the app's own pages, in place of the live video: the dashboard, the cameras, the groups, the snapshots, the videos or the events. */
export const replacesStage = (shown: Page) => (OWN_PAGES as readonly string[]).includes(shown.kind);

/**
 * The chart color each thing takes, wherever it shows, as an index into the
 * theme's chart colors (0 for `chart-1`): its tile's chip on the dashboard, its
 * series in a chart and its legend, and the badge in the header of the page
 * that holds it. The dashboard's badge takes the cameras' color, as its first
 * tile does, and so does the Cameras page's, which that tile opens. The Groups
 * page's takes the cameras' too: no tile opens it, and its groups are of
 * cameras. The disk's is the drive's, in the storage ring and the status bar.
 */
export const CHART = { videos: 0, snapshots: 1, cameras: 2, disk: 3 };

/** `page` as `settings.json` keeps it: `dashboard`, `cameras`, `groups`, `view:<id>`, `camera:<id>`; `""` for nothing. */
export function pageKey(shown: Page): string {
  if (shown.kind === "camera" && shown.stream) return `camera:${shown.id}${STREAM_MARK}${shown.stream}`;
  if (shown.kind === "view" || shown.kind === "camera") return `${shown.kind}:${shown.id}`;
  return shown.kind === "none" ? "" : shown.kind;
}

/** What parts a camera page's key from the stream it shows, when not its own: a line break, which neither an id nor a stream's path has. */
const STREAM_MARK = "\n";

/** The page `key` names, if it still can be shown: a view or a camera since deleted can't. */
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

/** The first view, else the first camera, else nothing. */
function firstPage(): Page {
  const view = untracked(views)[0];
  if (view) return { kind: "view", id: view.id };
  const camera = untracked(states)[0];
  return camera ? { kind: "camera", id: camera.id } : { kind: "none" };
}

/** The page the window closed on, else the first view. */
export const page = signal<Page>(pageFrom(Preferences.lastPage()) ?? firstPage());
/** The selected tile of the view on the stage, from 0. */
export const selectedTile = signal(0);
/** The tile filling the stage after a double click, if one does. */
export const zoomedTile = signal<number | null>(null);

/** Reads the groups and views again: after a change made here, so it shows at once. */
export function refresh() {
  groups.set(Groups.list());
  views.set(Views.list());
  states.set(Cameras.states());
  changes.set(untracked(changes) + 1);
}

/** Follows Rust from now on, and saves the page each time it changes, to open on next time. Called once, by the app. */
export function listen() {
  Cameras.onStates((next) => states.set(next));
  Server.onSignInNeeded((needed) => signInNeeded.set(needed));
  Library.onChange(() => {
    groups.set(Groups.list());
    views.set(Views.list());
    changes.set(untracked(changes) + 1);
  });
  // A camera's state changes every second, so the key keeps this to a page of its own.
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

/** The grid every view's tiles sit on (12 × 12), and the smallest a tile is. */
export const grid = Views.grid();

/** The view on the stage, if the stage shows one. */
export const currentView = computed(() => {
  const shown = page();
  return shown.kind === "view" ? views().find((view) => view.id === shown.id) : undefined;
});

/**
 * The camera the header, the PTZ panel and the audio follow: the camera on
 * its own, or the one in the selected tile. `""` when that tile is empty.
 */
export const focusedId = computed(() => {
  const shown = page();
  if (shown.kind === "camera") return shown.id;
  return currentView()?.tiles[selectedTile()] ?? "";
});

/** Where a picture is on the stage, as Auto keeps the size it is drawn at: tile `index` of view `view`. */
export const tilePlace = (view: string, index: number) => `view:${view}:${index}`;

/** Where a picture is on the stage, as Auto keeps the size it is drawn at: camera `camera` on its own. */
export const pagePlace = (camera: string) => `camera:${camera}`;

/**
 * How each picture on the stage was drawn last, by its place (`tilePlace`,
 * `pagePlace`): what Auto picks a stream by. Kept when the page changes, so
 * the last view's tiles stay connected on the streams they played.
 */
const drawn = signal<Record<string, DrawnPicture>>({});

/** How long a picture's new size holds before Auto follows it. */
const SETTLE_MS = 500;
const settling = new Map<string, number>();

/**
 * Says the picture at `place` is drawn `size` points large, `zoom` times
 * zoomed in, filling it as `fit` says. Its first size counts at once, so the
 * picture starts on the stream it needs; a change counts once it has held for
 * `SETTLE_MS`, so a drag of the window's edge or a turn of the wheel doesn't
 * switch streams at every step. A picture laid out with no size yet, or
 * hidden, says nothing.
 */
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

/**
 * The stream a picture of camera `camera` that picks `saved` plays, as
 * `StreamChoice.key` has it (`""` for the camera's own): the one it picks, or
 * on Auto (`""`), the one Auto picks for `pictures`, every picture of the
 * camera on Auto with it. `undefined` while Auto has a choice to make and none
 * of them has been drawn yet, so nothing connects to a stream only to leave it.
 */
export function playedStream(camera: string, saved: string, pictures: DrawnPicture[]): string | undefined {
  changes();
  if (saved !== "") return Cameras.choiceKey(camera, saved);
  if (!Cameras.autoPicks(camera)) return "";
  return pictures.length > 0 ? Cameras.autoStream(camera, pictures) : undefined;
}

/**
 * The stream each of `view`'s tiles plays (`playedStream`), in the order of
 * its tiles: a camera's tiles on Auto play one stream between them, the one
 * the largest calls for. `undefined` for an empty tile.
 */
export function playedIn(view: ViewInfo): (string | undefined)[] {
  const sizes = drawn();
  const onAuto = (camera: string) =>
    view.tiles.flatMap((each, index) => {
      const picture = each === camera && (view.streams[index] ?? "") === "" ? sizes[tilePlace(view.id, index)] : undefined;
      return picture ? [picture] : [];
    });
  return view.tiles.map((camera, index) => (camera === "" ? undefined : playedStream(camera, view.streams[index] ?? "", onAuto(camera))));
}

/** The stream camera `camera` plays on its own, picking `saved` (`playedStream`), as it was drawn there last. */
export function playedOnItsOwn(camera: string, saved: string): string | undefined {
  const picture = drawn()[pagePlace(camera)];
  return playedStream(camera, saved, picture ? [picture] : []);
}

/** The stream each tile of the view on the stage plays (`playedIn`); none while the stage shows no view. */
export const playedOnStage = computed(() => {
  const view = currentView();
  return view ? playedIn(view) : [];
});

/**
 * The stream the selected tile plays, or the camera on its own, as
 * `StreamChoice.key` has it: `""` for the camera's own, and while Auto has
 * yet to pick.
 */
export const focusedStream = computed(() => {
  const shown = page();
  if (shown.kind === "camera") return playedOnItsOwn(shown.id, shown.stream ?? "") ?? "";
  if (shown.kind !== "view") return "";
  return playedOnStage()[selectedTile()] ?? "";
});

/**
 * What the menus call stream `key` of camera `camera`, as a tile picks it: its
 * name among `Cameras.streamChoices`, else the key itself; `""` for Auto and
 * for the camera's own.
 */
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

/** Camera `camera` on `stream`, as `ShownStream` has it, where it plays one yet. */
const playing = (camera: string, stream: string | undefined): ShownStream[] => (stream === undefined ? [] : [{ camera, stream }]);

/**
 * The streams `shown` puts on the stage, each once: a camera on its own, on the
 * stream the page plays, or each filled tile's of a view in `list`, on the
 * stream the tile plays (`playedStream`); none for any other page, and none
 * for a picture Auto has yet to pick for.
 */
export function streamsOn(shown: Page, list: ViewInfo[]): ShownStream[] {
  if (shown.kind === "camera") return playing(shown.id, playedOnItsOwn(shown.id, shown.stream ?? ""));
  if (shown.kind !== "view") return [];
  const view = list.find((view) => view.id === shown.id);
  if (!view) return [];
  const played = playedIn(view);
  return eachOnce(view.tiles.flatMap((camera, index) => (camera === "" ? [] : playing(camera, played[index]))));
}

/** The ids of the cameras `shown` puts on the stage: a camera's own, or the tiles' of a view in `list`; none for any other page. */
export function camerasOn(shown: Page, list: ViewInfo[]): string[] {
  if (shown.kind === "camera") return [shown.id];
  if (shown.kind !== "view") return [];
  const view = list.find((view) => view.id === shown.id);
  return [...new Set((view?.tiles ?? []).filter((id) => id !== ""))];
}

/** The ids of the cameras on screen. */
export const onScreen = computed(() => camerasOn(page(), views()));

/** The streams on screen, each once: what `Cameras.setOnScreen` is told the tiles show. */
export const streamsOnScreen = computed(() => streamsOn(page(), views()));

/** The view the stage showed last, set by `show`. */
const lastView = signal<Page>(untracked(page).kind === "view" ? untracked(page) : { kind: "none" });

/** The cameras opened on their own since `lastView`, each with the stream it picked there last (`""` for Auto), set by `show`. */
const openedSince = signal<ShownStream[]>(
  (() => {
    const shown = untracked(page);
    return shown.kind === "camera" ? [{ camera: shown.id, stream: shown.stream ?? "" }] : [];
  })(),
);

/**
 * The ids of the cameras that stay connected whatever the page: the last
 * view's, and every camera opened on its own since. A camera no tile shows
 * keeps its status current and its picture comes back at once, but nothing of
 * it is decoded or drawn meanwhile. Opening another view starts over with its
 * cameras.
 */
export const kept = computed(() => [...new Set([...camerasOn(lastView(), views()), ...openedSince().map((shown) => shown.camera)])]);

/** The streams of `kept`: each of the last view's tiles on the stream it plays, and each camera opened on its own on the stream it played there last. */
export const keptStreams = computed(() =>
  eachOnce([...streamsOn(lastView(), views()), ...openedSince().flatMap((shown) => playing(shown.camera, playedOnItsOwn(shown.camera, shown.stream)))]),
);

/** Whether any camera is recording, on screen or not. */
export const recording = computed(() => states().some((camera) => camera.recording));

/** What the app calls each thing a camera can tell apart in its picture, by its key. */
const DETECTIONS = new Map(Cameras.detections().map((detection) => [detection.key, detection.title]));

/** What a camera's AI told apart, by `DetectionInfo` key, in words: `Person, Vehicle`. */
export const detectionsLabel = (keys: string[]) => keys.map((key) => DETECTIONS.get(key) ?? key).join(", ");

/**
 * What a watched camera sees now, in words for its tile: what its AI tells
 * apart (`Person, Vehicle`), else `Motion`; `""` when nothing moves.
 */
export function activityOf(camera: CameraState): string {
  if (camera.detected.length > 0) return detectionsLabel(camera.detected);
  return camera.motion ? "Motion" : "";
}

/**
 * What covers a camera's picture while it isn't live, as its tiles and its own
 * page draw it (`PictureNotice`): why (`kind`, which picks the icon and the way
 * forward), in a title and a sentence, and when the picture still drawn under
 * it came (`detail`), where there is one.
 */
export type PictureNoticeInfo = {
  /** Not connected; lost the camera and reconnects; gave up; or the camera refused its login. */
  kind: "off" | "reconnecting" | "stopped" | "refused";
  title: string;
  text: string;
  detail?: string;
};

const pictureTime = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit", second: "2-digit" });

/**
 * What covers `camera`'s picture, on `stream`, the stream it plays while that
 * runs: why it isn't connected, or what its stream says while it lost the
 * camera or gave up. Nothing while pictures come, nor while it connects for the
 * first time, which its numbers say.
 */
export function pictureNotice(camera: CameraState, stream: RunningStream | undefined): PictureNoticeInfo | undefined {
  if (!camera.connected) return { kind: "off", title: "Not connected", text: camera.status };
  if (!stream || stream.trouble === "fine") return undefined;
  const pictured = stream.lastPictureAt > 0;
  const detail = pictured ? `Last picture ${pictureTime.format(new Date(stream.lastPictureAt))}` : undefined;
  switch (stream.trouble) {
    case "reconnecting":
      // One that never had a picture never had a connection to lose.
      return { kind: "reconnecting", title: pictured ? "Connection lost" : "Can't reach the camera", text: stream.problem, detail };
    case "stopped":
      return { kind: "stopped", title: "Stream stopped", text: stream.problem, detail };
    case "refused":
      return { kind: "refused", title: "Login refused", text: stream.problem, detail };
  }
}

/** Shows a page, starting on its first tile. */
export function show(next: Page) {
  // A view starts over with its own cameras; a camera opened on its own joins them, on the
  // stream it picks, which takes the place of the one it picked before.
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
  selectedTile.set(0);
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
