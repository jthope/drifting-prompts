// The links between an event and the snapshot and videos it saved: an event's
// way to its files on their own pages, and a file's way back to its event. It
// joins `eventlog.ts`, `snapshots.ts` and `recordings.ts`, so none of those
// needs another; the pages put what it gives in their menus, and the events
// page's preview plays an event's videos from what it finds here.
import { computed } from "nsh";
import type { VideoInfo } from "ipcamera/native";
import { notify } from "../../../dp/ui";
import type { MenuEntries } from "./actions";
import { type EventItem, eventLibrary, viewing } from "./eventlog";
import { show } from "./model";
import { playing, videoLibrary, videos } from "./recordings";
import { snapshotLibrary } from "./snapshots";

/** `1 video`, `6 videos`: an event's clips, as its menu and its row say them. */
export const clipsLabel = (count: number) => (count === 1 ? "1 video" : `${count} videos`);

/** The recordings in the videos folder, by file name. */
const videosByName = computed(() => new Map(videos().map((video) => [video.name, video])));

/** The recordings `event` saved, oldest first, as the videos folder last listed them: one not listed yet is left out. */
export const eventVideos = (event: EventItem): VideoInfo[] => event.videos.flatMap((name) => videosByName().get(name) ?? []);

/**
 * Opens what `event` saved, as large as the app shows it: its picture in the
 * viewer, else its videos on the videos page; one that saved neither says so.
 */
export function openEvent(event: EventItem) {
  if (event.picture !== "") viewing.set(event.name);
  else if (event.videos.length > 0) playEventsVideos(event);
  else notify("This event saved no picture or video.");
}

/** Opens the snapshots page narrowed to what `event` saved there. */
export function showEventsSnapshot(event: EventItem) {
  snapshotLibrary.clearFilters();
  snapshotLibrary.eventFilter.set(event.id);
  show({ kind: "snapshots" });
}

/** Opens the videos page narrowed to `event`'s recordings, its first playing: the player's arrows step through the rest. */
export function playEventsVideos(event: EventItem) {
  showEventsVideos(event);
  playing.set(event.videos[0] ?? null);
}

/** Opens the videos page narrowed to `event`'s recordings. */
export function showEventsVideos(event: EventItem) {
  videoLibrary.clearFilters();
  videoLibrary.eventFilter.set(event.id);
  show({ kind: "videos" });
}

/** Opens the events page narrowed to the event `id`, which saved a file. */
export function showEvent(id: string) {
  eventLibrary.clearFilters();
  eventLibrary.eventFilter.set(id);
  show({ kind: "events" });
}

/** What an event's menu adds for what it saved: playing its videos, and its files on their pages. */
export function eventLinks(event: EventItem): MenuEntries {
  const videos = event.videos.length > 0;
  return [
    videos && { label: `Play ${clipsLabel(event.videos.length)}`, onSelect: () => playEventsVideos(event) },
    event.snapshot !== "" && { label: "Show in Snapshots", onSelect: () => showEventsSnapshot(event) },
    videos && { label: "Show in Videos", onSelect: () => showEventsVideos(event) },
  ];
}

/** What a snapshot's or a video's menu adds when an event saved it: the way back to that event. */
export function fileLinks(file: { event: string }): MenuEntries {
  return file.event === "" ? [] : [{ label: "Show event", onSelect: () => showEvent(file.event) }];
}
