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

/** Starts again the camera's streams that gave up. */
export function tryAgain(camera: CameraState) {
  Cameras.connect(camera.id).catch(failed);
}

/** Switches the camera's spotlight on or off, saying why when the camera won't. */
export function toggleSpotlight(camera: CameraState) {
  Cameras.setSpotlight(camera.id, !camera.spotlightOn).catch(failed);
}

/** Saves the picture of every connected camera on screen. */
export function takeSnapshots() {
  snapshot(onScreen());
}

/** Saves camera `camera`'s picture. */
export function takeSnapshotOf(camera: string) {
  snapshot([camera]);
}

function snapshot(cameras: string[]) {
  Cameras.snapshot(cameras)
    .then((count) => notify(count === 1 ? "Snapshot saved. Find it under Snapshots." : `Saved ${count} snapshots. Find them under Snapshots.`, "ok"))
    .catch(failed);
}

/** Records every connected camera on screen, or stops every recording. */
export function toggleRecording() {
  if (recording()) stopRecording();
  else record(onScreen());
}

/** Records `camera`, or while it records stops every recording. */
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

/** Puts a camera the settings dialog just added where it was asked for. */
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

/** Asks for a name and makes a group of `cameras`. */
export async function newGroup(...cameras: string[]) {
  const name = await askName({ title: "New group", label: "Name", action: "Create" });
  if (name === null) return;
  await attempt(() => Groups.add(name, cameras));
}

/** What a group's dialog edits: a saved group, or a new one (`group: null`). */
export type GroupEditing = { group: string | null };

/** The group the dialog is open for, or `null` while it is closed. */
export const groupEditing = signal<GroupEditing | null>(null);

/** Opens a group's dialog; `null` opens it for a new group. */
export function editGroup(id: string | null) {
  groupEditing.set({ group: id });
}

/** Shows a group the dialog just made: on the Groups page, it is selected. */
export function groupAdded(id: string) {
  refresh();
  if (page().kind === "groups") groupListing.pick(id, "only");
}

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

/** Adds a copy of each of `cameras` on the next channel its address doesn't use; stops at the first that can't be made. */
export async function duplicateCameras(cameras: { id: string }[]) {
  // The copies join as they are made, so two copies of one recorder take two channels.
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

/** The groups as a menu for `cameras`, each checked when all of them are in it, then New group. */
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

/** What the Cameras page's selection offers. */
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

/** Adds a copy of each of `list`, named with "copy"; stops at the first that can't be made. */
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

/** Moves camera `id` to `index` among the cameras; resolves with whether it moved. */
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

/** The cameras as a menu for groups `list`, each checked when every group holds it. */
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

/** What the Groups page's selection offers. */
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

/** Sets how tile `index`'s picture fills it: a key of `Views.fits`. */
export function setTileFit(view: ViewInfo, index: number, fit: string) {
  return attempt(() => Views.setTileFit(view.id, index, fit));
}

/** Shows `camera` in a tile on `stream`: `""` for Auto, `own` for its own, else a key of `Cameras.streamChoices`; a camera of `""` empties the tile. */
export function setTile(view: ViewInfo, index: number, camera: string, stream = "") {
  return attempt(() => Views.setTile(view.id, index, camera, stream));
}

/** An empty tile's Add camera: the cameras' menu, or with none yet the new camera's form. */
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

/** How a picture fills its place (a key of `Views.fits`), and what picking another does. */
export type FitChoice = { fit: string; pick: (fit: string) => void };

/** How tile `index` of `view` fills its tile, which the tile saves when another is picked. */
export function tileFit(view: ViewInfo, index: number): FitChoice {
  return { fit: view.fits[index] ?? view.fit, pick: (fit) => setTileFit(view, index, fit) };
}

/** How camera `camera` fills the stage on its own, until the app closes. */
export function pageFit(camera: string): FitChoice {
  return { fit: pageFitOf(camera), pick: (fit) => setPageFit(camera, fit) };
}

/** A picture's stream pick (`""` for Auto, `own` for its own), what it plays, and how to pick another. */
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

/** Every fit, the picture's own checked: those that keep its shape, then those that force one. */
export function fitMenu(choice: FitChoice): MenuEntries {
  const entry = (fit: FitInfo) => ({ label: fit.title, checked: fit.key === choice.fit, onSelect: () => choice.pick(fit.key) });
  return [...fits.filter((fit) => !fit.reshapes).map(entry), { type: "separator" as const }, ...fits.filter((fit) => fit.reshapes).map(entry)];
}

/** What a camera's live picture offers, on its own and in a view's tile. */
export function pictureMenu(camera: string, stream: StreamPick, fit: FitChoice): MenuEntries {
  const state = states().find((each) => each.id === camera);
  return [
    { label: "Stream", submenu: streamMenu(camera, stream) },
    { label: "Fit", submenu: fitMenu(fit) },
    state && { label: state.connected ? "Disconnect" : "Connect", onSelect: () => toggleConnection(state) },
    { label: "Edit camera…", onSelect: () => editCamera(camera) },
  ];
}

/** What camera `camera`'s picture offers where it fills the stage on its own. */
export function cameraPageMenu(camera: string, stream: string): MenuEntries {
  return pictureMenu(camera, pageStream(camera, stream), pageFit(camera));
}

/** Which of `camera`'s streams a picture on `stream` shows; `undefined` for one the camera no longer has. */
export function shownChoice(camera: string, stream: string): StreamChoice | undefined {
  const key = Cameras.choiceKey(camera, stream);
  return Cameras.streamChoices(camera).find((choice) => choice.key === key);
}

/** What a picture's stream picker shows for a stream it is on: its size, else its name, else the key it was saved under. */
export function streamLabel(camera: string, stream: string): string {
  const shown = shownChoice(camera, stream);
  return shown?.size || shown?.name || stream;
}

/** What a picture's stream picker shows for its pick: `Auto · 896×512` on Auto, else `streamLabel`'s. */
export function pickLabel(camera: string, stream: StreamPick): string {
  if (stream.current !== "") return streamLabel(camera, stream.current);
  return stream.played === undefined ? "Auto" : `Auto · ${streamLabel(camera, stream.played)}`;
}

/** Every stream a picture of `camera` can pick, Auto first, then each under its lens; one it no longer has stays listed. */
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

/** What a tile's menu offers. */
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

/** What a camera's menu offers; `group` is the group it is listed under, `canMove` adds moving it. */
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

/** What a group's menu offers; `canMove` adds moving it in the sidebar's order. */
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

/** Opens a menu of `items` at `at`, an event's `pagePosition`. */
export function popup(items: MenuEntries, at?: { x: number; y: number }) {
  popupMenu(items, at);
}

/** Whether `page` shows `id`. */
export function showing(kind: "view" | "camera", id: string) {
  const shown = page();
  return shown.kind === kind && shown.id === id;
}
