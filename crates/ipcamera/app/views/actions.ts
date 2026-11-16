// What the buttons and menus do, shared by every place that offers them.
import { signal } from "nsh";
import { Cameras, type CameraState, type FitInfo, type GroupInfo, Groups, type StreamChoice, type ViewInfo, Views } from "ipcamera/native";
import { askName, confirmDelete, type MenuEntries, notify, popupMenu } from "../../../dp/ui";
import { listing as groupListing } from "./grouplist";
import { copyName } from "./listing";
import { fits, groups, onScreen, page, pageFitOf, playedIn, playedOnItsOwn, recording, refresh, setPageFit, show, states, views } from "./model";
import { clearFilters as clearCameraFilters, copyOf, groupFilter, pick, selectOnly, type Taken } from "./roster";

export type { MenuEntries };

/** Shows why something failed. */
export function failed(error: unknown) {
  const message = (error as { message?: string })?.message;
  notify(String(message ?? error), "warn");
}

/**
 * Makes a change, saying why when it fails, then reads the library again so it
 * shows at once. Resolves with whether it was made. Changes are saved where the
 * cameras are kept, a file or the server, so they always take a moment.
 */
async function attempt(change: () => Promise<unknown>): Promise<boolean> {
  try {
    await change();
    return true;
  } catch (error) {
    failed(error);
    return false;
  } finally {
    refresh();
  }
}

/** Connects a stopped camera, or disconnects a running one. */
export function toggleConnection(camera: CameraState) {
  (camera.connected ? Cameras.disconnect(camera.id) : Cameras.connect(camera.id)).catch(failed);
}

/** Starts again the camera's streams that gave up, which nothing else tries again. */
export function tryAgain(camera: CameraState) {
  Cameras.connect(camera.id).catch(failed);
}

/** Switches the camera's spotlight on or off, saying why when the camera won't. */
export function toggleSpotlight(camera: CameraState) {
  Cameras.setSpotlight(camera.id, !camera.spotlightOn).catch(failed);
}

/**
 * Saves the picture of every connected camera on screen, as Record records
 * them, and says how that went.
 */
export function takeSnapshots() {
  snapshot(onScreen());
}

/** Saves camera `camera`'s picture, from the toolbar over it. */
export function takeSnapshotOf(camera: string) {
  snapshot([camera]);
}

/** Saves the pictures of `cameras` that are connected, and says how that went. */
function snapshot(cameras: string[]) {
  Cameras.snapshot(cameras)
    // The files' names carry the cameras' ids, which say nothing to the user: the gallery is
    // where they see them.
    .then((count) => notify(count === 1 ? "Snapshot saved. Find it under Snapshots." : `Saved ${count} snapshots. Find them under Snapshots.`, "ok"))
    .catch(failed);
}

/**
 * Records every connected camera on screen, or stops every recording. One
 * switch for them all, so a recording can always be stopped from where the
 * user is, even after leaving the cameras it records.
 */
export function toggleRecording() {
  if (recording()) stopRecording();
  else record(onScreen());
}

/**
 * Records `camera`, from the toolbar over its picture, or while it records
 * stops every recording: the same one switch as Record's, for one camera.
 */
export function toggleRecordingOf(camera: CameraState) {
  if (camera.recording) stopRecording();
  else record([camera.id]);
}

function record(cameras: string[]) {
  Cameras.record(cameras)
    .then((count) => notify(count === 1 ? "Recording." : `Recording ${count} cameras.`, "ok"))
    .catch(failed);
}

function stopRecording() {
  Cameras.stopRecording()
    .then((count) => notify(count === 1 ? "Recording stopped. Find it under Videos." : "Recordings stopped. Find them under Videos.", "ok"))
    .catch(failed);
}

/** Mutes the camera, or unmutes it, at the volume it has. */
export function toggleMute(camera: CameraState) {
  Cameras.setAudio(camera.id, !camera.muted, camera.volume);
}

/** Where a camera added from a tile's menu goes. */
export type TileTarget = { view: string; index: number };

/** What a camera's settings dialog edits: a saved camera, or a new one (`camera: null`). */
export type Editing = { camera: string | null; tile?: TileTarget };

/** The camera the settings dialog is open for, or `null` while it is closed. */
export const editing = signal<Editing | null>(null);

/** Opens the settings for a new camera; `tile` is where it goes once added. */
export function newCamera(tile?: TileTarget) {
  editing.set({ camera: null, tile });
}

export function editCamera(id: string) {
  editing.set({ camera: id });
}

/** The cameras the editor for several at once is open for, or `null` while it is closed. */
export const bulkEditing = signal<string[] | null>(null);

/** Opens the settings for `ids`: a camera's own dialog for one, the editor for several at once for more. */
export function editCameras(ids: string[]) {
  if (ids.length === 1) editCamera(ids[0]);
  else if (ids.length > 1) bulkEditing.set(ids);
}

/**
 * Puts a camera the settings dialog just added where it was asked for. Added
 * from the Cameras page, it stays there, selected; from anywhere else, it
 * shows on its own.
 */
export function cameraAdded(id: string, tile?: TileTarget) {
  refresh();
  if (tile) attempt(() => Views.setTile(tile.view, tile.index, id, ""));
  else if (page().kind === "cameras") pick(id, "only");
  else show({ kind: "camera", id });
}

export async function deleteCamera(camera: CameraState) {
  const count = Cameras.groupsOf(camera.id).length;
  const alsoGroups = count === 0 ? "" : ` It also leaves ${count === 1 ? "the group" : `the ${count} groups`} it is in.`;
  const sure = await confirmDelete({
    title: `Delete ${camera.name}?`,
    description: `Its settings are removed, and any tile showing it is emptied.${alsoGroups}`,
    action: "Delete camera",
  });
  if (!sure) return;
  Cameras.delete(camera.id)
    .then(refresh)
    .catch(failed);
}

/** Asks for a name and makes a group, with `cameras` in it, in order, in one save. */
export async function newGroup(...cameras: string[]) {
  const name = await askName({ title: "New group", label: "Name", action: "Create" });
  if (name === null) return;
  await attempt(() => Groups.add(name, cameras));
}

/** What a group's dialog edits: a saved group, or a new one (`group: null`). */
export type GroupEditing = { group: string | null };

/** The group the dialog is open for, or `null` while it is closed. */
export const groupEditing = signal<GroupEditing | null>(null);

/** Opens a group's dialog, with its name and its cameras; `null` opens it for a new group. */
export function editGroup(id: string | null) {
  groupEditing.set({ group: id });
}

/** Shows a group the dialog just made: on the Groups page, it is selected. */
export function groupAdded(id: string) {
  refresh();
  if (page().kind === "groups") groupListing.pick(id, "only");
}

/** "1 camera", "3 cameras". */
const cameraCount = (count: number) => (count === 1 ? "1 camera" : `${count.toLocaleString()} cameras`);

/** "1 group", "3 groups". */
export const groupCount = (count: number) => (count === 1 ? "1 group" : `${count.toLocaleString()} groups`);

/** Deletes several cameras after one question, one after another, stopping at the first that can't be. */
export async function deleteCameras(cameras: CameraState[]) {
  if (cameras.length === 1) return deleteCamera(cameras[0]);
  if (cameras.length === 0) return;
  const sure = await confirmDelete({
    title: `Delete ${cameraCount(cameras.length)}?`,
    description: "Their settings are removed, any tile showing them is emptied, and they leave every group they are in.",
    action: `Delete ${cameraCount(cameras.length)}`,
  });
  if (!sure) return;
  let deleted = 0;
  try {
    for (const camera of cameras) {
      await Cameras.delete(camera.id);
      deleted += 1;
    }
    notify(`Deleted ${cameraCount(deleted)}.`, "ok");
  } catch (error) {
    failed(error);
  } finally {
    refresh();
  }
}

/**
 * Adds a copy of each of `cameras` (`copyOf`): the same settings and groups
 * on the next channel its address doesn't use, which is how a recorder's
 * cameras are set up one after another. Made on the Cameras page, the copies
 * are selected, ready to rename or change together; anywhere else the page
 * stays as it is. Stops at the first that can't be made.
 */
export async function duplicateCameras(cameras: { id: string }[]) {
  // Every camera as it is now, the copies joining as they are made, so two copies of one recorder take two channels.
  const taken: Taken[] = states().flatMap((camera) => {
    try {
      return [Cameras.form(camera.id)];
    } catch {
      return [];
    }
  });
  const made: { id: string; name: string; channel: string }[] = [];
  try {
    for (const camera of cameras) {
      const copy = copyOf(Cameras.form(camera.id), taken);
      const id = await Cameras.add(copy);
      for (const group of Cameras.groupsOf(camera.id)) await Groups.addCamera(group, id);
      taken.push(copy);
      made.push({ id, name: copy.name, channel: copy.channel });
    }
  } catch (error) {
    failed(error);
  } finally {
    refresh();
  }
  if (made.length === 0) return;
  notify(made.length === 1 ? `Added ${made[0].name}, on channel ${made[0].channel}.` : `Added ${cameraCount(made.length)}.`, "ok");
  if (page().kind === "cameras") selectOnly(made.map((each) => each.id));
}

/** Puts every one of `cameras` in `group`, or, when `out`, takes them all out of it. */
export function setGroup(cameras: { id: string }[], group: GroupInfo, out: boolean) {
  return attempt(async () => {
    for (const camera of cameras) {
      if (out) await Groups.removeCamera(group.id, camera.id);
      else await Groups.addCamera(group.id, camera.id);
    }
  });
}

/**
 * The groups, each checked when every one of `cameras` is in it, and saying
 * how many are when only some are: choosing one puts them all in it, or takes
 * them all out when they all were. Then a way to make a group of them.
 */
export function groupChoices(cameras: { id: string }[]): MenuEntries {
  const all = cameras.length;
  const choices = groups().map((group) => {
    const count = cameras.filter((camera) => group.cameras.includes(camera.id)).length;
    return {
      label: count > 0 && count < all ? `${group.name}  (${count.toLocaleString()} of ${all.toLocaleString()})` : group.name,
      checked: count === all,
      onSelect: () => setGroup(cameras, group, count === all),
    };
  });
  return [...choices, choices.length > 0 && { type: "separator" as const }, { label: "New group…", onSelect: () => newGroup(...cameras.map((camera) => camera.id)) }];
}

/** What the Cameras page's selection offers, in its bar's menu and on a selected row's right press. */
export function selectionMenu(cameras: CameraState[]): MenuEntries {
  return [
    { label: `Edit ${cameraCount(cameras.length)}…`, onSelect: () => editCameras(cameras.map((camera) => camera.id)) },
    { label: `Duplicate ${cameraCount(cameras.length)}`, onSelect: () => duplicateCameras(cameras) },
    { label: "Groups", submenu: groupChoices(cameras) },
    { type: "separator" as const },
    { label: `Delete ${cameraCount(cameras.length)}…`, onSelect: () => deleteCameras(cameras) },
  ];
}

export async function renameGroup(group: GroupInfo) {
  const name = await askName({ title: "Rename group", label: "Name", action: "Rename", value: group.name });
  if (name !== null) await attempt(() => Groups.rename(group.id, name));
}

export async function deleteGroup(group: GroupInfo) {
  const sure = await confirmDelete({
    title: `Delete the group ${group.name}?`,
    description: "Its cameras stay, under All cameras and in any other group.",
    action: "Delete group",
  });
  if (sure) await attempt(() => Groups.remove(group.id));
}

/** Deletes several groups after one question, in one save: all of them or, when one can't be, none. */
export async function deleteGroups(list: GroupInfo[]) {
  if (list.length === 1) return deleteGroup(list[0]);
  if (list.length === 0) return;
  const sure = await confirmDelete({
    title: `Delete ${groupCount(list.length)}?`,
    description: "Their cameras stay, under All cameras and in any other group.",
    action: `Delete ${groupCount(list.length)}`,
  });
  if (sure && (await attempt(() => Groups.removeMany(list.map((group) => group.id))))) notify(`Deleted ${groupCount(list.length)}.`, "ok");
}

/**
 * Adds a copy of each of `list`: the same cameras, in the same order, named
 * after it with "copy" ("Outside copy", "Outside copy 2"), at the end. Made on
 * the Groups page, the copies are selected, ready to rename or change. Stops
 * at the first that can't be made.
 */
export async function duplicateGroups(list: GroupInfo[]) {
  const taken = groups().map((group) => group.name);
  const made: { id: string; name: string }[] = [];
  try {
    for (const group of list) {
      const name = copyName(group.name, taken);
      made.push({ id: await Groups.add(name, group.cameras), name });
      taken.push(name);
    }
  } catch (error) {
    failed(error);
  } finally {
    refresh();
  }
  if (made.length === 0) return;
  notify(made.length === 1 ? `Added ${made[0].name}.` : `Added ${groupCount(made.length)}.`, "ok");
  if (page().kind === "groups") groupListing.selectOnly(made.map((each) => each.id));
}

/** Moves `group` to `index` among the groups, as the sidebar lists them. */
export function moveGroup(group: GroupInfo, index: number) {
  return attempt(() => Groups.moveTo(group.id, index));
}

/**
 * Moves camera `id` to `index` among the cameras: the order All cameras shows,
 * and the one every list of cameras starts from. Resolves with whether it
 * moved, once the list shows it.
 */
export function moveCamera(id: string, index: number) {
  return attempt(() => Cameras.moveTo(id, index));
}

/** Moves camera `id` one place up (`by` -1) or down (1) among the cameras, when there is room that way. */
export function nudgeCamera(id: string, by: -1 | 1) {
  const list = states();
  const index = list.findIndex((each) => each.id === id);
  const to = index + by;
  if (index < 0 || to < 0 || to >= list.length) return;
  void moveCamera(id, to);
}

/** Shows the Cameras page narrowed to `group`'s cameras, with nothing else left out. */
export function showGroupCameras(group: GroupInfo) {
  clearCameraFilters();
  groupFilter.set(group.id);
  show({ kind: "cameras" });
}

/** Puts `camera` in every one of `list`, or, when `out`, takes it out of every one. */
export function setMember(list: GroupInfo[], camera: string, out: boolean) {
  return attempt(async () => {
    for (const group of list) {
      if (out) await Groups.removeCamera(group.id, camera);
      else await Groups.addCamera(group.id, camera);
    }
  });
}

/**
 * The cameras, each checked when every one of `list` holds it, and saying in
 * how many when only some do: choosing one puts it in them all, or takes it
 * out of them all when they all held it. The other way round from
 * `groupChoices`.
 */
export function memberChoices(list: GroupInfo[]): MenuEntries {
  const all = list.length;
  if (states().length === 0) return [{ label: "No cameras yet", disabled: true }];
  return states().map((camera) => {
    const count = list.filter((group) => group.cameras.includes(camera.id)).length;
    return {
      label: count > 0 && count < all ? `${camera.name}  (${count.toLocaleString()} of ${all.toLocaleString()})` : camera.name,
      checked: count === all,
      onSelect: () => setMember(list, camera.id, count === all),
    };
  });
}

/** What the Groups page's selection offers on a selected row's right press, as its bar does. */
export function groupSelectionMenu(list: GroupInfo[]): MenuEntries {
  return [
    { label: "Cameras", submenu: memberChoices(list) },
    { label: `Duplicate ${groupCount(list.length)}`, onSelect: () => duplicateGroups(list) },
    { type: "separator" as const },
    { label: `Delete ${groupCount(list.length)}…`, onSelect: () => deleteGroups(list) },
  ];
}

/** Makes a 2 × 2 view with the next free name, and shows it. */
export async function newView() {
  const taken = new Set(views().map((view) => view.name));
  let number = views().length + 1;
  while (taken.has(`View ${number}`)) number += 1;

  let id = "";
  const made = await attempt(async () => {
    id = await Views.add(`View ${number}`, "4");
  });
  if (made) show({ kind: "view", id });
}

export async function renameView(view: ViewInfo) {
  const name = await askName({ title: "Rename view", label: "Name", action: "Rename", value: view.name });
  if (name !== null) await attempt(() => Views.rename(view.id, name));
}

export async function deleteView(view: ViewInfo) {
  const sure = await confirmDelete({
    title: `Delete the view ${view.name}?`,
    description: "Only the view goes: its cameras are not touched.",
    action: "Delete view",
  });
  if (sure) await attempt(() => Views.remove(view.id));
}

export function setLayout(view: ViewInfo, layout: string) {
  return attempt(() => Views.setLayout(view.id, layout));
}

/** Sets how tile `index`'s picture fills it: a key of `Views.fits`. The tile keeps it as its camera or stream changes. */
export function setTileFit(view: ViewInfo, index: number, fit: string) {
  return attempt(() => Views.setTileFit(view.id, index, fit));
}

/** Shows `camera` in a tile on `stream`: `""` for Auto, `own` for its own, else a key of `Cameras.streamChoices`; a camera of `""` empties the tile. */
export function setTile(view: ViewInfo, index: number, camera: string, stream = "") {
  return attempt(() => Views.setTile(view.id, index, camera, stream));
}

/**
 * An empty tile's Add camera: the cameras to show in it, or with none yet, the
 * new camera's form at once, rather than a menu holding only "New camera…".
 * `open` shows the menu (`MenuOpener`'s).
 */
export function addToTile(view: ViewInfo, index: number, open: (items: MenuEntries) => void) {
  if (states().length === 0) newCamera({ view: view.id, index });
  else open(cameraChoices(view, index));
}

/** The cameras to put in a tile, with the one it shows checked, then a way to add another. */
export function cameraChoices(view: ViewInfo, index: number): MenuEntries {
  const current = view.tiles[index] ?? "";
  return [
    ...states().map((camera) => ({ label: camera.name, checked: camera.id === current, onSelect: () => setTile(view, index, camera.id) })),
    states().length > 0 && { type: "separator" as const },
    { label: "New camera…", onSelect: () => newCamera({ view: view.id, index }) },
  ];
}

/** How a picture fills its place (`fit`, a key of `Views.fits`), and what picking another does: a tile's `tileFit`, or the page's `pageFit`. */
export type FitChoice = { fit: string; pick: (fit: string) => void };

/** How tile `index` of `view` fills its tile, which the tile saves when another is picked. */
export function tileFit(view: ViewInfo, index: number): FitChoice {
  return { fit: view.fits[index] ?? view.fit, pick: (fit) => setTileFit(view, index, fit) };
}

/** How camera `camera` fills the stage on its own, until the app closes. */
export function pageFit(camera: string): FitChoice {
  return { fit: pageFitOf(camera), pick: (fit) => setPageFit(camera, fit) };
}

/**
 * A picture's stream as its menus pick it: what it picks (`current`, `""` for
 * Auto, `own` for its camera's own), what it plays (`played`, a key of
 * `Cameras.streamChoices`, `undefined` while Auto has yet to pick), and what
 * picking another does: a tile's `tileStream`, or the page's `pageStream`.
 */
export type StreamPick = { current: string; played: string | undefined; pick: (stream: string) => void };

/** The stream tile `index` of `view` picks and plays, which the tile saves when another is picked. */
export function tileStream(view: ViewInfo, index: number): StreamPick {
  const camera = view.tiles[index] ?? "";
  return { current: view.streams[index] ?? "", played: playedIn(view)[index], pick: (stream) => setTile(view, index, camera, stream) };
}

/** The stream camera `camera` picks on its own (`current`) and plays, which picking another shows it on. */
export function pageStream(camera: string, current: string): StreamPick {
  return { current, played: playedOnItsOwn(camera, current), pick: (stream) => show({ kind: "camera", id: camera, stream }) };
}

/**
 * Every fit, the picture's own checked (`choice.fit`): those that work from the
 * picture's own shape (Original, Fill, Stretch), then, apart, those that force
 * a shape on it. Picking one goes to `choice.pick`.
 */
export function fitMenu(choice: FitChoice): MenuEntries {
  const entry = (fit: FitInfo) => ({ label: fit.title, checked: fit.key === choice.fit, onSelect: () => choice.pick(fit.key) });
  return [...fits.filter((fit) => !fit.reshapes).map(entry), { type: "separator" as const }, ...fits.filter((fit) => fit.reshapes).map(entry)];
}

/**
 * What a camera's live picture offers, wherever the stage shows it: on its own,
 * and in a view's tile, whose menu has these among what the tile itself offers.
 * `stream` is the stream it picks and plays, and how it picks another: the
 * page's for a camera on its own, the tile's in a view; `fit`, how it fills its
 * place, the same way. Connect or Disconnect is the toolbar's, for a narrow
 * picture whose toolbar leaves it out. Something a picture's menu should do
 * goes here, so both get it.
 */
export function pictureMenu(camera: string, stream: StreamPick, fit: FitChoice): MenuEntries {
  const state = states().find((each) => each.id === camera);
  return [
    { label: "Stream", submenu: streamMenu(camera, stream) },
    { label: "Fit", submenu: fitMenu(fit) },
    state && { label: state.connected ? "Disconnect" : "Connect", onSelect: () => toggleConnection(state) },
    { label: "Edit camera…", onSelect: () => editCamera(camera) },
  ];
}

/** What camera `camera`'s picture offers where it fills the stage on its own, picking `stream`: `pictureMenu`, whose Stream and Fit change the page's. */
export function cameraPageMenu(camera: string, stream: string): MenuEntries {
  return pictureMenu(camera, pageStream(camera, stream), pageFit(camera));
}

/**
 * Which of camera `camera`'s streams (`Cameras.streamChoices`) a picture on
 * `stream` (`""` for the camera's own) shows, whichever way `stream` reaches it;
 * `undefined` for one the camera no longer has.
 */
export function shownChoice(camera: string, stream: string): StreamChoice | undefined {
  const key = Cameras.choiceKey(camera, stream);
  return Cameras.streamChoices(camera).find((choice) => choice.key === key);
}

/** What a picture's stream picker shows for a stream it is on: its size, else its name, else the key it was saved under. */
export function streamLabel(camera: string, stream: string): string {
  const shown = shownChoice(camera, stream);
  return shown?.size || shown?.name || stream;
}

/** What a picture's stream picker shows for its pick (`StreamPick`): on Auto, `Auto` and what it plays (`Auto · 896×512`), else the stream it picked (`streamLabel`). */
export function pickLabel(camera: string, stream: StreamPick): string {
  if (stream.current !== "") return streamLabel(camera, stream.current);
  return stream.played === undefined ? "Auto" : `Auto · ${streamLabel(camera, stream.played)}`;
}

/**
 * Every way camera `camera`'s picture can pick its stream: Auto first, with
 * what it plays while it is on it, then each of the camera's streams
 * (`Cameras.streamChoices`) once with its size, its codec and its name,
 * largest first, its own marked as its default; under a heading for each lens
 * where it has more than one. The picture's pick (`stream.current`) is
 * checked, and `stream.pick` takes the one chosen: `""` for Auto, `own` for the
 * camera's own. A stream it picked that the camera no longer has stays listed,
 * checked.
 */
export function streamMenu(camera: string, stream: StreamPick): MenuEntries {
  const choices = Cameras.streamChoices(camera);
  const auto = stream.current === "";
  const shown = auto ? undefined : Cameras.choiceKey(camera, stream.current);
  const lenses = new Set(choices.map((choice) => choice.lens)).size > 1;
  const entries: MenuEntries[number][] = [{ label: auto ? pickLabel(camera, stream) : "Auto", checked: auto, onSelect: () => stream.pick("") }];
  let lens: string | undefined;
  for (const choice of choices) {
    if (lens === undefined || (lenses && choice.lens !== lens)) {
      entries.push({ type: "separator" as const });
      if (lenses) entries.push({ label: choice.lens || "Main lens", disabled: true });
      lens = choice.lens;
    }
    entries.push({
      label: choice.own ? `${choice.title} (default)` : choice.title,
      checked: choice.key === shown,
      onSelect: () => stream.pick(choice.own ? "own" : choice.key),
    });
  }
  if (shown !== undefined && !choices.some((choice) => choice.key === shown)) entries.push({ label: stream.current, checked: true, onSelect: () => {} });
  return entries;
}

/** What a tile's menu offers: what it shows, then what its camera's picture offers anywhere (`pictureMenu`), the tile's stream among it. */
export function tileMenu(view: ViewInfo, index: number): MenuEntries {
  const current = view.tiles[index] ?? "";
  const filled = current !== "";
  return [
    { label: "Show camera", submenu: cameraChoices(view, index) },
    filled && { label: "Open on its own", onSelect: () => show({ kind: "camera", id: current }) },
    ...(filled ? pictureMenu(current, tileStream(view, index), tileFit(view, index)) : []),
    filled && { type: "separator" as const },
    filled && { label: "Empty tile", onSelect: () => setTile(view, index, "") },
  ];
}

/**
 * What a camera's menu offers, wherever it is listed. `group` is the group it
 * is listed under. Moving it is in the cameras' own order, which only the
 * sidebar's All cameras shows, so only that list asks for it (`canMove`).
 */
export function cameraMenu(camera: CameraState, group?: GroupInfo, canMove = false): MenuEntries {
  const index = states().findIndex((each) => each.id === camera.id);
  const last = states().length - 1;
  return [
    { label: "Edit camera…", onSelect: () => editCamera(camera.id) },
    { label: "Duplicate", onSelect: () => duplicateCameras([camera]) },
    { label: "Groups", submenu: groupChoices([camera]) },
    group && { label: `Remove from ${group.name}`, onSelect: () => attempt(() => Groups.removeCamera(group.id, camera.id)) },
    canMove && { type: "separator" as const },
    canMove && { label: "Move up", shortcut: "alt+up", disabled: index <= 0, onSelect: () => moveCamera(camera.id, index - 1) },
    canMove && { label: "Move down", shortcut: "alt+down", disabled: index < 0 || index >= last, onSelect: () => moveCamera(camera.id, index + 1) },
    canMove && { label: "Move to top", disabled: index <= 0, onSelect: () => moveCamera(camera.id, 0) },
    canMove && { label: "Move to bottom", disabled: index < 0 || index >= last, onSelect: () => moveCamera(camera.id, last) },
    { type: "separator" as const },
    { label: "Delete camera…", onSelect: () => deleteCamera(camera) },
  ];
}

/**
 * What a group's menu offers, in the sidebar and on the Groups page. Moving
 * it is in the sidebar's order, so the page leaves it out (`canMove`) while
 * its table is sorted otherwise.
 */
export function groupMenu(group: GroupInfo, canMove = true): MenuEntries {
  const index = groups().findIndex((each) => each.id === group.id);
  return [
    { label: "Edit group…", onSelect: () => editGroup(group.id) },
    { label: "Rename group…", onSelect: () => renameGroup(group) },
    { label: "Cameras", submenu: memberChoices([group]) },
    { label: "Show on the Cameras page", onSelect: () => showGroupCameras(group) },
    { label: "Duplicate", onSelect: () => duplicateGroups([group]) },
    canMove && { type: "separator" as const },
    canMove && { label: "Move up", disabled: index <= 0, onSelect: () => moveGroup(group, index - 1) },
    canMove && { label: "Move down", disabled: index < 0 || index >= groups().length - 1, onSelect: () => moveGroup(group, index + 1) },
    { type: "separator" as const },
    { label: "Delete group…", onSelect: () => deleteGroup(group) },
  ];
}

export function viewMenu(view: ViewInfo): MenuEntries {
  return [
    { label: "Rename view…", onSelect: () => renameView(view) },
    { label: "Delete view…", onSelect: () => deleteView(view) },
  ];
}

/** Opens a menu of `items` at `at`, an event's `pagePosition`: the system's, or a drawn one in a browser (`popupMenu`). A button's menu opens with `MenuOpener` instead. */
export function popup(items: MenuEntries, at?: { x: number; y: number }) {
  popupMenu(items, at);
}

/** Whether `page` shows `id`. */
export function showing(kind: "view" | "camera", id: string) {
  const shown = page();
  return shown.kind === kind && shown.id === id;
}
