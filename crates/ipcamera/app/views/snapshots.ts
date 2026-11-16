// What the gallery shows: the snapshots from Rust, and how they are filtered,
// sorted and grouped. Like `model.ts`, these are module-level signals, so the
// header's menus, the page and the viewer share them.
import { createAnimatedValue, signal } from "nsh";
import { type SnapshotInfo, Snapshots } from "ipcamera/native";
import { confirmDelete, notify } from "../../../dp/ui";
import { failed, type MenuEntries } from "./actions";
import { cameraMenuEntry, cameraName, countLabel, deleteEach, libraryOf, menuOf, sizeLabel } from "./library";
import { storage } from "./model";

/** Every snapshot in the folder, newest first, as Rust last read it. */
export const snapshots = signal<SnapshotInfo[]>([]);
/** Whether the first read has arrived, so the page doesn't say there are none while it reads. */
export const loaded = signal(false);
/** How the gallery is narrowed, sorted and grouped: by month until the user picks. */
export const snapshotLibrary = libraryOf(snapshots, (shot) => shot.takenAt, "month");
export const { order, grouping, cameraFilter, period, filtered, clearFilters, visible, sections, ordered } = snapshotLibrary;
/** The snapshot open in the viewer, by its file name. */
export const viewing = signal<string | null>(null);

/**
 * The gallery's scroll offset, fed natively by its list: the hero fades and
 * slides with it, and the header's count fades in, with no JS per frame.
 */
export const galleryScroll = createAnimatedValue(0);

let reading = 0;

/** Reads the folder again. A read that finishes after a newer one is dropped. */
export function reload() {
  const ticket = ++reading;
  Snapshots.list()
    .then((list) => {
      if (ticket !== reading) return;
      snapshots.set(list);
      loaded.set(true);
    })
    .catch(failed);
}

const longDate = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric", year: "numeric", hour: "numeric", minute: "2-digit", second: "2-digit" });

/** When a snapshot was taken, as its tile says it: as much as its section's title doesn't. */
export const takenLabel = snapshotLibrary.timeLabel;

/** When a snapshot was taken, in full, for the viewer. */
export function takenInFull(shot: SnapshotInfo): string {
  return longDate.format(new Date(shot.takenAt));
}

/** "1 snapshot", "12 snapshots". */
export const snapshotsLabel = (count: number) => `${countLabel(count)} ${count === 1 ? "snapshot" : "snapshots"}`;

/** Asks, then deletes the snapshot for good. Resolves true once it is gone. */
export async function deleteSnapshot(shot: SnapshotInfo): Promise<boolean> {
  const sure = await confirmDelete({
    title: "Delete this snapshot?",
    description: `${cameraName(shot.camera)}, ${takenInFull(shot)}. It is deleted for good, not moved to the Recycle Bin.`,
    action: "Delete snapshot",
  });
  if (!sure) return false;
  try {
    await Snapshots.delete(shot.name);
    return true;
  } catch (error) {
    failed(error);
    return false;
  } finally {
    reload();
  }
}

/**
 * Asks, then deletes `list` for good, one after another: one that fails says
 * why, and the rest still go. What was deleted leaves the selection; what
 * failed stays in it. One alone is asked about as its own Delete asks.
 */
export async function deleteSnapshots(list: SnapshotInfo[]) {
  if (list.length === 0) return;
  if (list.length === 1) {
    if (await deleteSnapshot(list[0])) snapshotLibrary.clearSelection();
    return;
  }
  const sure = await confirmDelete({
    title: `Delete ${snapshotsLabel(list.length)}?`,
    description: "They are deleted for good, not moved to the Recycle Bin.",
    action: "Delete snapshots",
  });
  if (!sure) return;
  const { gone, failure } = await deleteEach(
    list.map((shot) => shot.name),
    (name) => Snapshots.delete(name),
  );
  const deleted = new Set(gone);
  snapshotLibrary.selected.set(snapshotLibrary.selected().filter((name) => !deleted.has(name)));
  reload();
  if (failure !== undefined) failed(failure);
  if (gone.length > 0) notify(`Deleted ${snapshotsLabel(gone.length)}.`, "ok");
}

export function revealSnapshot(shot: SnapshotInfo) {
  Snapshots.reveal(shot.name).catch(failed);
}

export function openSnapshotsFolder() {
  try {
    Snapshots.openFolder();
  } catch (error) {
    failed(error);
  }
}

/** What a picture's menu offers, in the grid and in the viewer. */
export function snapshotMenu(shot: SnapshotInfo, links: MenuEntries = []): MenuEntries {
  return [
    { label: "Open", onSelect: () => viewing.set(shot.name) },
    ...links,
    { label: storage.revealLabel, onSelect: () => revealSnapshot(shot) },
    { type: "separator" as const },
    cameraMenuEntry(snapshotLibrary, shot),
    { type: "separator" as const },
    { label: "Delete snapshot…", onSelect: () => deleteSnapshot(shot) },
  ];
}

/** A picture's right press in the grid: the selection's menu when it is one of several selected, else its own, with `links` after Open. */
export const tileMenu = (shot: SnapshotInfo, links: MenuEntries = []) => menuOf(snapshotLibrary, shot, () => snapshotMenu(shot, links), deleteSnapshots);

/** What a list's row says at its end: the picture's size in pixels, and on disk. */
export const snapshotExtra = (shot: SnapshotInfo) => [shot.width > 0 ? `${shot.width} × ${shot.height}` : "", sizeLabel(shot.bytes)].filter((part) => part !== "").join("  ·  ");
