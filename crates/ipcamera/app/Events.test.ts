import { expect, test } from "nsh/test";
import type { VideoInfo } from "ipcamera/native";
import { current, eventLibrary, eventMenu, type EventItem, events, eventsLabel, lastedLabel, previewed, savedLabel, stepPreview, tileMenu, viewing } from "./views/eventlog";
import { rowsOf } from "./views/library";
import { eventLinks, eventVideos, fileLinks, openEvent, showEventsSnapshot } from "./views/linked";
import { page, states } from "./views/model";
import { playing, videoLibrary, videos } from "./views/recordings";
import { snapshotLibrary } from "./views/snapshots";

const camera = () => states()[0].id;

function event(id: string, startedAt: number, ongoing = false, saved: Partial<EventItem> = {}): EventItem {
  return {
    id,
    name: id,
    event: id,
    camera: camera(),
    startedAt,
    endedAt: ongoing ? 0 : startedAt + 4_000,
    ongoing,
    motion: false,
    detected: ["person"],
    tags: ["person"],
    sightings: 1,
    snapshot: "",
    picture: "",
    thumbnail: "",
    videos: [],
    ...saved,
  };
}

const SAVED: Partial<EventItem> = {
  snapshot: "snapshot-1-cam.person.png",
  picture: "C:\\snapshots\\snapshot-1-cam.person.png",
  thumbnail: "C:\\snapshots\\thumbnails\\snapshot-1-cam.person.png",
  videos: ["video-1-cam.person.mp4", "video-60-cam.person.mp4"],
  sightings: 2,
};

const label = (entry: unknown) => (entry && typeof entry === "object" && "label" in entry ? entry.label : undefined);

test("the page opens as a table, a row each under a heading for each day, which folds", () => {
  // At noon, so both fall on today whenever the test runs.
  const noon = new Date().setHours(12, 0, 0, 0);
  events.set([event("a", noon), event("b", noon - 60_000)]);
  expect(eventLibrary.size()).toBe("list");
  expect(eventLibrary.grouping()).toBe("day");

  const today = eventLibrary.sections()[0].key;
  const rows = () => rowsOf(eventLibrary.sections(), 1, eventLibrary.isCollapsed).map((row) => (row.kind === "heading" ? `# ${row.title}` : row.items[0].name));
  expect(rows()).toEqual(["# Today", "a", "b"]);
  eventLibrary.toggleCollapsed(today);
  expect(rows()).toEqual(["# Today"]);
  eventLibrary.toggleCollapsed(today);
  expect(rows()).toEqual(["# Today", "a", "b"]);
});

test("the preview shows the row picked, else the first, and Up and Down step through the rows the table shows", () => {
  const noon = new Date().setHours(12, 0, 0, 0);
  const day = 86_400_000;
  events.set([event("a", noon), event("b", noon - 60_000), event("c", noon - day)]);
  current.set(null);
  expect(previewed()?.name).toBe("a");

  current.set("b");
  expect(previewed()?.name).toBe("b");
  expect(stepPreview(1)).toBe("c");
  expect(stepPreview(1)).toBe(null);
  expect(current()).toBe("c");

  const today = eventLibrary.sections()[0].key;
  eventLibrary.toggleCollapsed(today);
  expect(stepPreview(-1)).toBe(null);
  current.set("a");
  expect(previewed()?.name).toBe("c");
  eventLibrary.toggleCollapsed(today);
  expect(previewed()?.name).toBe("a");

  events.set([]);
  expect(previewed()).toBe(null);
  current.set(null);
});

test("how long the camera saw an event, in seconds, minutes and hours", () => {
  const lasted = (seconds: number) => lastedLabel(event("a", 0, false, { endedAt: seconds * 1000 }));
  expect(lasted(38)).toBe("38 s");
  expect(lasted(120)).toBe("2 min");
  expect(lasted(168)).toBe("2 min 48 s");
  expect(lasted(3900)).toBe("1 h 5 min");
  expect(lastedLabel(event("a", 0, true))).toBe("Going on now");
});

test("an event's videos are found in the videos folder, and it opens its picture, else its videos", () => {
  const clip = (name: string): VideoInfo => ({ name, path: `C:\\videos\\${name}`, thumbnail: "", camera: camera(), tags: ["person"], event: "a", startedAt: 1, seconds: 30, bytes: 1, recording: false });
  videos.set([clip("video-60-cam.person.mp4"), clip("video-1-cam.person.mp4")]);
  const seen = event("a", Date.now(), false, { ...SAVED, videos: [...(SAVED.videos ?? []), "video-90-cam.person.mp4"] });
  expect(eventVideos(seen).map((video) => video.name)).toEqual(["video-1-cam.person.mp4", "video-60-cam.person.mp4"]);

  openEvent(seen);
  expect(viewing()).toBe("a");
  viewing.set(null);

  openEvent(event("a", Date.now(), false, { videos: ["video-1-cam.person.mp4"] }));
  expect(page()).toEqual({ kind: "videos" });
  expect(videoLibrary.eventFilter()).toBe("a");
  expect(playing()).toBe("video-1-cam.person.mp4");
  playing.set(null);
  videoLibrary.clearFilters();
  videos.set([]);
});

test("an event's menu opens its picture first, then what it saved, then its camera", () => {
  const seen = event("a", Date.now(), false, SAVED);
  events.set([seen]);
  const menu = eventMenu(seen, eventLinks(seen)).filter(Boolean);
  expect(menu.map(label)).toEqual(["Open picture", "Play 2 videos", "Show in Snapshots", "Show in Videos", undefined, "Show camera", undefined, `Only ${states()[0].name}`, undefined, "Delete event…"]);

  const open = menu[0];
  if (open && "onSelect" in open) open.onSelect?.();
  expect(viewing()).toBe("a");
  viewing.set(null);

  const bare = event("b", Date.now());
  const plain = eventMenu(bare, eventLinks(bare)).filter(Boolean);
  expect(plain[0]).toMatchObject({ label: "Open picture", disabled: true });
  expect(plain.map(label)).not.toContain("Show in Snapshots");
});

test("deleting an event names what it saved, which goes with it", () => {
  expect(savedLabel(event("a", 1, false, SAVED))).toBe("its picture and its 2 videos");
  expect(savedLabel(event("a", 1, false, { snapshot: "s.png" }))).toBe("its picture");
  expect(savedLabel(event("a", 1, false, { videos: ["v.mp4"] }))).toBe("its video");
  expect(savedLabel(event("a", 1))).toBe("");
});

test("an event's files open on their page narrowed to it, and a file leads back to its event", () => {
  const seen = event("a", Date.now(), false, SAVED);
  showEventsSnapshot(seen);
  expect(page()).toEqual({ kind: "snapshots" });
  expect(snapshotLibrary.eventFilter()).toBe("a");
  expect(snapshotLibrary.filtered()).toBe(true);
  snapshotLibrary.clearFilters();
  expect(snapshotLibrary.eventFilter()).toBe(null);

  expect(fileLinks({ event: "" })).toEqual([]);
  const back = fileLinks({ event: "a" })[0];
  expect(label(back)).toBe("Show event");
  if (back && "onSelect" in back) back.onSelect?.();
  expect(page()).toEqual({ kind: "events" });
  expect(eventLibrary.eventFilter()).toBe("a");
  eventLibrary.clearFilters();
});

test("the Tag filter keeps what the camera saw", () => {
  const now = Date.now();
  events.set([event("a", now), event("b", now - 1, false, { detected: ["vehicle"], tags: ["vehicle"] }), event("c", now - 2, false, { detected: [], tags: ["motion"] })]);
  expect(eventLibrary.tags().each.map((tag) => tag.title)).toEqual(["Person", "Vehicle", "Motion"]);
  eventLibrary.tagFilter.set("vehicle");
  expect(eventLibrary.visible().map((each) => each.name)).toEqual(["b"]);
  eventLibrary.clearFilters();
  expect(eventLibrary.visible().length).toBe(3);
});

test("one the camera still sees can't be deleted, and several selected are deleted together", () => {
  const now = Date.now();
  events.set([event("a", now), event("b", now - 60_000), event("c", now - 120_000, true)]);
  eventLibrary.clearSelection();
  expect(eventMenu(event("c", now, true)).find((entry) => label(entry) === "Delete event")).toMatchObject({ disabled: true });

  eventLibrary.pick("a", "toggle");
  eventLibrary.pick("b", "toggle");
  expect(label(tileMenu(events()[0])[0])).toBe("Delete (2)…");
  expect(label(tileMenu(events()[2])[0])).toBe("Open picture");
  expect(eventsLabel(1)).toBe("1 event");
  expect(eventsLabel(1234)).toBe("1,234 events");
  eventLibrary.clearSelection();
});
