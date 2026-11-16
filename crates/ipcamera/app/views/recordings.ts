// What the videos page shows: the recordings from Rust, how they are filtered,
// sorted and grouped, and what can be done with them. Like `snapshots.ts`,
// these are module-level signals, so the header's menus and count, the page
// and the player share them.
import { computed, createAnimatedValue, signal } from "nsh";
import { type VideoInfo, Videos } from "ipcamera/native";
import { confirmDelete, notify } from "../../../dp/ui";
import { failed, type MenuEntries } from "./actions";
import { cameraMenuEntry, cameraName, countLabel, deleteEach, libraryOf, menuOf, sizeLabel } from "./library";
import { storage } from "./model";

/** Every recording in the folder, newest first, as Rust last read it. */
export const videos = signal<VideoInfo[]>([]);
/** Whether the first read has arrived, so the page doesn't say there are none while it reads. */
export const loaded = signal(false);
/** The recording open in the player, by its file name. */
export const playing = signal<string | null>(null);
/** How the page is narrowed, sorted and grouped: by day until the user picks. */
export const videoLibrary = libraryOf(videos, (video) => video.startedAt, "day");
export const { filtered, clearFilters, visible, sections, ordered } = videoLibrary;

/** The page's scroll offset, fed natively by its list: the totals fade with it, and the header's count fades in. */
export const videosScroll = createAnimatedValue(0);

let reading = 0;

/** Reads the folder again. A read that finishes after a newer one is dropped. */
export function reload() {
  const ticket = ++reading;
  Videos.list()
    .then((list) => {
      if (ticket !== reading) return;
      videos.set(list);
      loaded.set(true);
    })
    .catch(failed);
}

const longDate = new Intl.DateTimeFormat(undefined, { weekday: "long", month: "long", day: "numeric", year: "numeric", hour: "numeric", minute: "2-digit", second: "2-digit" });

/** When a recording started, as its tile says it: as much as its section's title doesn't. */
export const startedLabel = videoLibrary.timeLabel;

/** When a recording started, in full, for the player. */
export const startedInFull = (video: VideoInfo) => longDate.format(new Date(video.startedAt));

/** `83` → `"1:23"`, `3725` → `"1:02:05"`. */
export function lengthLabel(seconds: number): string {
  const whole = Math.max(0, Math.round(seconds));
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const rest = String(whole % 60).padStart(2, "0");
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${rest}` : `${minutes}:${rest}`;
}

/** `5400` → `"1 h 30 min"`, `95` → `"2 min"`, for totals. */
export function totalLabel(seconds: number): string {
  const minutes = Math.round(seconds / 60);
  if (minutes < 1) return `${Math.round(seconds)} s`;
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  return minutes % 60 === 0 ? `${hours} h` : `${hours} h ${minutes % 60} min`;
}

/** "1 video", "12 videos". */
export const videosLabel = (count: number) => `${countLabel(count)} ${count === 1 ? "video" : "videos"}`;

/** Opens a recording in the player. One still being written can't be played yet. */
export function play(video: VideoInfo) {
  if (video.recording) {
    notify("This video is still recording. Stop recording to play it.");
    return;
  }
  playing.set(video.name);
}

/** Asks, then deletes the recording for good. Resolves true once it is gone. */
export async function deleteVideo(video: VideoInfo): Promise<boolean> {
  const sure = await confirmDelete({
    title: "Delete this video?",
    description: `${cameraName(video.camera)}, ${startedInFull(video)}, ${lengthLabel(video.seconds)} long. It is deleted for good, not moved to the Recycle Bin.`,
    action: "Delete video",
  });
  if (!sure) return false;
  try {
    await Videos.delete(video.name);
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
 * why, and the rest still go. Those still recording can't be deleted, so they
 * stay, and the question says so. What was deleted leaves the selection.
 */
export async function deleteVideos(list: VideoInfo[]) {
  const done = list.filter((video) => !video.recording);
  const busy = list.length - done.length;
  if (done.length === 0) {
    if (busy > 0) notify(busy === 1 ? "This video is still recording. Stop recording to delete it." : "These videos are still recording. Stop recording to delete them.");
    return;
  }
  if (done.length === 1 && busy === 0) {
    if (await deleteVideo(done[0])) videoLibrary.clearSelection();
    return;
  }
  const staying = busy === 0 ? "" : busy === 1 ? " The one still recording stays." : ` The ${countLabel(busy)} still recording stay.`;
  const sure = await confirmDelete({
    title: `Delete ${videosLabel(done.length)}?`,
    description: `They are deleted for good, not moved to the Recycle Bin.${staying}`,
    action: "Delete videos",
  });
  if (!sure) return;
  const { gone, failure } = await deleteEach(
    done.map((video) => video.name),
    (name) => Videos.delete(name),
  );
  const deleted = new Set(gone);
  videoLibrary.selected.set(videoLibrary.selected().filter((name) => !deleted.has(name)));
  reload();
  if (failure !== undefined) failed(failure);
  if (gone.length > 0) notify(`Deleted ${videosLabel(gone.length)}.`, "ok");
}

export function revealVideo(video: VideoInfo) {
  Videos.reveal(video.name).catch(failed);
}

export function openVideosFolder() {
  try {
    Videos.openFolder();
  } catch (error) {
    failed(error);
  }
}

/** What a recording's menu offers, on the page and in the player. */
export function videoMenu(video: VideoInfo, links: MenuEntries = []): MenuEntries {
  return [
    { label: "Play", disabled: video.recording, onSelect: () => play(video) },
    ...links,
    { label: storage.revealLabel, onSelect: () => revealVideo(video) },
    { type: "separator" as const },
    cameraMenuEntry(videoLibrary, video),
    { type: "separator" as const },
    { label: "Delete video…", disabled: video.recording, onSelect: () => deleteVideo(video) },
  ];
}

/** A recording's right press on the page: the selection's menu when it is one of several selected, else its own, with `links` after Play. */
export const tileMenu = (video: VideoInfo, links: MenuEntries = []) => menuOf(videoLibrary, video, () => videoMenu(video, links), deleteVideos);

/** What the recordings the filters keep add up to, for the totals. */
export const totals = computed(() => {
  const list = visible();
  return {
    count: list.length,
    cameras: new Set(list.map((video) => video.camera)).size,
    seconds: list.reduce((sum, video) => sum + video.seconds, 0),
    onDisk: sizeLabel(list.reduce((sum, video) => sum + video.bytes, 0)),
    recording: list.filter((video) => video.recording).length,
  };
});

/** Whether any recording is still being written, shown or not. While one is, the page reads the folder again on a timer. */
export const anyRecording = computed(() => videos().some((video) => video.recording));
