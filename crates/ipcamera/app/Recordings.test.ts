// The videos page's filtering, grouping, totals and labels, on recordings made up here.
//
// As with `Snapshots.test.ts`, `App.test.tsx` can only open the page empty,
// because a test gets no file system to put recordings where the app reads
// them. These read `views/recordings.ts` directly instead, and
// `src/camera/recording.rs` tests the files.
import { expect, test } from "nsh/test";
import type { VideoInfo } from "ipcamera/native";
import { rowsOf } from "./views/library";
import { states } from "./views/model";
import {
  clearFilters,
  filtered,
  lengthLabel,
  ordered,
  sections,
  startedLabel,
  totalLabel,
  totals,
  videoLibrary,
  videoMenu,
  videos,
  videosLabel,
  visible,
} from "./views/recordings";
import { snapshotLibrary } from "./views/snapshots";

const DAY = 86_400_000;

/** The camera the throwaway settings start with. */
const camera = () => states()[0].id;

function video(name: string, startedAt: number, seconds: number, recording = false, cameraId = "cam"): VideoInfo {
  return { name, path: `C:\\videos\\${name}.mp4`, thumbnail: "", camera: cameraId, tags: [], event: "", startedAt, seconds, bytes: 2048, recording };
}

/** Noon today, so a few hours either way stays on the same day. */
function noon(): number {
  const now = new Date();
  return new Date(now.getFullYear(), now.getMonth(), now.getDate(), 12).getTime();
}

/** The page as it first opens: newest first, by day, nothing filtered. */
function fresh() {
  clearFilters();
  videoLibrary.order.set("newest");
  videoLibrary.grouping.set("day");
}

const names = (list: VideoInfo[]) => list.map((each) => each.name);

test("recordings fall under a heading for each day, newest first", () => {
  fresh();
  const today = noon();
  videos.set([video("now", today + 3_600_000, 60, true), video("morning", today - 3_600_000, 90), video("yesterday", today - DAY, 30)]);

  expect(sections().map((section) => section.title)).toEqual(["Today", "Yesterday"]);
  expect(names(sections()[0].items)).toEqual(["now", "morning"]);
  expect(names(ordered())).toEqual(["now", "morning", "yesterday"]);

  const rows = rowsOf(sections(), 1);
  expect(rows.map((row) => row.kind)).toEqual(["heading", "tiles", "tiles", "heading", "tiles"]);
});

test("the camera and date filters narrow the videos, and the totals follow", () => {
  fresh();
  const today = noon();
  videos.set([
    video("porch-today", today, 60, false, camera()),
    video("gone-today", today - 60_000, 120, false, "gone"),
    video("porch-old", today - 60 * DAY, 30, false, camera()),
  ]);
  expect(filtered()).toBe(false);
  expect(totals().count).toBe(3);

  videoLibrary.cameraFilter.set(camera());
  expect(names(visible())).toEqual(["porch-today", "porch-old"]);
  expect(totals().cameras).toBe(1);
  expect(totalLabel(totals().seconds)).toBe("2 min");

  videoLibrary.period.set("week");
  expect(names(visible())).toEqual(["porch-today"]);
  expect(filtered()).toBe(true);
  expect(videoLibrary.summary()).toContain("Camera 1, Last 7 days");
  // The player steps through what the page shows, and nothing else.
  expect(names(ordered())).toEqual(["porch-today"]);

  videoLibrary.cameraFilter.set("nobody");
  expect(sections().length).toBe(0);
  expect(totals().count).toBe(0);

  clearFilters();
  expect(visible().length).toBe(3);
  expect(filtered()).toBe(false);
});

test("the camera filter offers only cameras with videos, by name, each with how many", () => {
  fresh();
  videos.set([video("a", noon(), 1, false, camera()), video("b", noon(), 1, false, camera()), video("c", noon(), 1, false, "")]);

  expect(videoLibrary.cameras()).toEqual([
    { id: camera(), name: "Camera 1", count: 2 },
    { id: "", name: "Unknown camera", count: 1 },
  ]);
});

test("oldest first and by camera, the tiles say the date their heading no longer does", () => {
  fresh();
  const today = noon();
  videos.set([video("new", today, 1, false, camera()), video("old", today - 3 * DAY, 1, false, camera())]);
  // Under a day's heading, the time is enough.
  expect(startedLabel(videos()[0])).toBe(new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(new Date(today)));

  videoLibrary.order.set("oldest");
  videoLibrary.grouping.set("camera");

  expect(sections().map((section) => section.title)).toEqual(["Camera 1"]);
  expect(names(ordered())).toEqual(["old", "new"]);
  const day = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" }).format(new Date(today));
  expect(startedLabel(videos()[0])).toContain(day);
});

test("filtering the videos leaves the snapshots as they were", () => {
  fresh();
  snapshotLibrary.cameraFilter.set(null);
  videoLibrary.cameraFilter.set(camera());
  videoLibrary.grouping.set("none");

  expect(snapshotLibrary.cameraFilter()).toBe(null);
  expect(snapshotLibrary.grouping()).toBe("month");
});

test("a video's menu narrows the page to its camera, and back", () => {
  fresh();
  const shown = video("porch", noon(), 10, false, camera());
  const entry = (label: string) => videoMenu(shown).find((each) => each && "label" in each && each.label === label);

  const only = entry("Only Camera 1");
  expect(only).toBeTruthy();
  if (only && "onSelect" in only) only.onSelect?.();
  expect(videoLibrary.cameraFilter()).toBe(camera());

  const every = entry("Show every camera");
  if (every && "onSelect" in every) every.onSelect?.();
  expect(videoLibrary.cameraFilter()).toBe(null);
});

test("the totals add up what the page shows, and say what is still recording", () => {
  fresh();
  const today = noon();
  videos.set([video("a", today, 3_600, true), video("b", today - 60_000, 1_800)]);

  expect(totals().count).toBe(2);
  expect(totals().recording).toBe(1);
  expect(totalLabel(totals().seconds)).toBe("1 h 30 min");
  expect(totals().onDisk).toBe("4.0 KB");
  expect(videosLabel(1)).toBe("1 video");
  expect(videosLabel(2)).toBe("2 videos");
});

test("lengths read as a player shows them", () => {
  expect(lengthLabel(0)).toBe("0:00");
  expect(lengthLabel(83)).toBe("1:23");
  expect(lengthLabel(3_725)).toBe("1:02:05");
  expect(totalLabel(20)).toBe("20 s");
  expect(totalLabel(95)).toBe("2 min");
  expect(totalLabel(7_200)).toBe("2 h");
});

test("a recording still being written can't be played or deleted from its menu", () => {
  const entries = videoMenu(video("busy", noon(), 10, true));
  const disabled = (label: string) => entries.find((entry) => entry && "label" in entry && entry.label === label);

  expect(disabled("Play")).toMatchObject({ disabled: true });
  expect(disabled("Delete video…")).toMatchObject({ disabled: true });
  expect(disabled("Show in Explorer")).toMatchObject({ label: "Show in Explorer" });
  expect(videoMenu(video("done", noon(), 10))[0]).toMatchObject({ label: "Play", disabled: false });
});
