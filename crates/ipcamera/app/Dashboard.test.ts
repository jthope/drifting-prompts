import { expect, test } from "nsh/test";
import type { CameraState, DriveSpace, EventInfo, SnapshotInfo, VideoInfo } from "ipcamera/native";
import { kept, keptStreams, onScreen, pageFrom, pageKey, show, states, streamsOn, streamsOnScreen, views } from "./views/model";
import { agoLabel, daysOf, drivesOf, eventHours, hoursOf, spaceByCamera, summarize } from "./views/overview";

const DAY = 86_400_000;

function video(startedAt: number, seconds: number, bytes: number, camera = "cam"): VideoInfo {
  return { name: `v${startedAt}`, path: "", thumbnail: "", camera, tags: [], event: "", startedAt, seconds, bytes, recording: false };
}

function shot(takenAt: number, bytes: number, camera = "cam"): SnapshotInfo {
  return { name: `s${takenAt}`, path: "", thumbnail: "", camera, tags: [], event: "", takenAt, bytes, width: 1920, height: 1080 };
}

/** Noon today, so a few hours either way stays on the same day. */
function noon(): number {
  const now = new Date();
  return new Date(now.getFullYear(), now.getMonth(), now.getDate(), 12).getTime();
}

test("the totals count every file, and the cameras connected and recording", () => {
  const camera = states()[0];
  const cameras: CameraState[] = [
    { ...camera, id: "a", connected: true, recording: true },
    { ...camera, id: "b", connected: true, recording: false },
    { ...camera, id: "c", connected: false, recording: false },
  ];
  const sums = summarize(cameras, [shot(1, 100), shot(2, 50)], [video(1, 90, 4000), video(2, 30, 1000)]);

  expect(sums.cameras).toBe(3);
  expect(sums.connected).toBe(2);
  expect(sums.recording).toBe(1);
  expect(sums.snapshots).toBe(2);
  expect(sums.videos).toBe(2);
  expect(sums.seconds).toBe(120);
  expect(sums.snapshotBytes).toBe(150);
  expect(sums.videoBytes).toBe(5000);
});

test("the days run back from today, each with what was saved in it, older files left out", () => {
  const today = noon();
  const list = daysOf(
    [shot(today, 1), shot(today - 3_600_000, 1), shot(today - 2 * DAY, 1), shot(today - 20 * DAY, 1)],
    [video(today, 60, 1), video(today - DAY, 30, 1), video(today - 7 * DAY, 30, 1)],
    7,
    today,
  );

  expect(list.length).toBe(7);
  const last = list[6];
  expect(new Date(last.start).getDate()).toBe(new Date(today).getDate());
  expect(new Date(last.start).getHours()).toBe(0);
  expect([last.videos, last.snapshots, last.seconds]).toEqual([1, 2, 60]);
  expect([list[5].videos, list[5].snapshots]).toEqual([1, 0]);
  expect([list[4].videos, list[4].snapshots]).toEqual([0, 1]);
  expect(list.reduce((total, day) => total + day.videos, 0)).toBe(2);
  expect(list.reduce((total, day) => total + day.snapshots, 0)).toBe(3);
});

function event(startedAt: number): EventInfo {
  return {
    id: `e${startedAt}`,
    camera: "cam",
    startedAt,
    endedAt: startedAt + 1000,
    ongoing: false,
    motion: true,
    detected: [],
    sightings: 1,
    snapshot: "",
    picture: "",
    thumbnail: "",
    videos: [],
  };
}

test("the events add up by weekday and hour over the period, older and later ones left out", () => {
  const today = noon();
  const weekday = new Date(today).getDay();
  // Noon `back` days ago by the calendar, so a change of clocks between doesn't move the hour.
  const noonBefore = (back: number) => {
    const date = new Date(today);
    return new Date(date.getFullYear(), date.getMonth(), date.getDate() - back, 12).getTime();
  };
  const cells = eventHours(
    [event(today), event(today + 60_000), event(today - 3_600_000), event(noonBefore(7)), event(noonBefore(14)), event(noonBefore(-1))],
    14,
    today + 3_600_000,
  );

  expect(cells.length).toBe(7 * 24);
  expect([cells[0].weekday, cells[0].hour, cells[25].weekday, cells[25].hour]).toEqual([0, 0, 1, 1]);
  const at = (hour: number) => cells[weekday * 24 + hour].events;
  expect([at(12), at(11)]).toEqual([3, 1]);
  expect(cells.reduce((total, cell) => total + cell.events, 0)).toBe(4);

  const hours = hoursOf(cells);
  expect(hours.length).toBe(24);
  expect([hours[0].hour, hours[23].hour]).toEqual([0, 23]);
  expect([hours[11].events, hours[12].events, hours[13].events]).toEqual([1, 3, 0]);
});

test("how long ago an event was reads in minutes, then hours, then as its date", () => {
  const now = noon();
  expect(agoLabel(now - 20_000, now)).toBe("Just now");
  expect(agoLabel(now + 5_000, now)).toBe("Just now");
  expect(agoLabel(now - 12 * 60_000, now)).toBe("12 min ago");
  expect(agoLabel(now - 5 * 3_600_000 - 60_000, now)).toBe("5 h ago");
  expect(agoLabel(now - 2 * DAY, now)).not.toContain("ago");
});

test("each camera's files add up, largest first, the deleted cameras' together", () => {
  const names: Record<string, string> = { porch: "Porch", yard: "Yard" };
  const rows = spaceByCamera(
    [shot(1, 10, "porch"), shot(2, 20, "gone"), shot(3, 5, "")],
    [video(1, 1, 100, "yard"), video(2, 1, 40, "porch"), video(3, 1, 30, "lost")],
    (id) => names[id],
  );

  expect(rows.map((row) => row.name)).toEqual(["Yard", "Deleted cameras", "Porch", "Unknown camera"]);
  expect(rows.map((row) => row.total)).toEqual([100, 50, 50, 5]);
  const porch = rows.find((row) => row.key === "porch");
  expect([porch?.videos, porch?.snapshots]).toEqual([40, 10]);
});

test("both folders on one drive show it once; on two drives, each with its own files", () => {
  const d: DriveSpace = { drive: "D:", free: 100, total: 500 };
  const e: DriveSpace = { drive: "E:", free: 10, total: 50 };
  const sums = { videoBytes: 70, snapshotBytes: 3 };

  expect(drivesOf(d, d, sums)).toEqual([{ ...d, videos: 70, snapshots: 3 }]);
  expect(drivesOf(d, e, sums)).toEqual([
    { ...d, videos: 70, snapshots: 0 },
    { ...e, videos: 0, snapshots: 3 },
  ]);
});

test("the last view's cameras, and each camera opened on its own since, stay connected whatever the page", () => {
  const view = views()[0];
  const camera = states()[0];
  show({ kind: "view", id: view.id });
  const tiles = onScreen();
  expect(tiles).toContain(camera.id);
  expect(kept()).toEqual(tiles);

  show({ kind: "camera", id: camera.id });
  expect(onScreen()).toEqual([camera.id]);
  expect(kept()).toEqual(tiles);
  show({ kind: "camera", id: "elsewhere" });
  expect(onScreen()).toEqual(["elsewhere"]);
  expect(kept()).toEqual([...tiles, "elsewhere"]);

  for (const page of [{ kind: "dashboard" }, { kind: "events" }, { kind: "videos" }] as const) {
    show(page);
    expect(onScreen()).toEqual([]);
    expect(kept()).toEqual([...tiles, "elsewhere"]);
  }

  show({ kind: "view", id: view.id });
  expect(kept()).toEqual(tiles);
  expect(keptStreams()).toEqual(streamsOnScreen());
});

test("a camera on its own stays connected on the stream it was shown on last", () => {
  const view = views()[0];
  show({ kind: "view", id: view.id });
  const tiles = streamsOnScreen();

  show({ kind: "camera", id: "elsewhere", stream: "sub" });
  expect(streamsOnScreen()).toEqual([{ camera: "elsewhere", stream: "sub" }]);
  expect(keptStreams()).toEqual([...tiles, { camera: "elsewhere", stream: "sub" }]);

  show({ kind: "camera", id: "elsewhere" });
  expect(keptStreams()).toEqual([...tiles, { camera: "elsewhere", stream: "" }]);
  expect(kept()).toContain("elsewhere");
  show({ kind: "view", id: view.id });
});

test("each tile's stream is on screen once, and a camera on its own is on its own stream", () => {
  const view = { ...views()[0], tiles: ["a", "a", "a", "b", ""], streams: ["", "sub", "sub", "", ""] };
  expect(streamsOn({ kind: "view", id: view.id }, [view])).toEqual([
    { camera: "a", stream: "" },
    { camera: "a", stream: "sub" },
    { camera: "b", stream: "" },
  ]);
  expect(streamsOn({ kind: "camera", id: "a" }, [view])).toEqual([{ camera: "a", stream: "" }]);
  expect(streamsOn({ kind: "camera", id: "a", stream: "sub" }, [view])).toEqual([{ camera: "a", stream: "sub" }]);
  expect(streamsOn({ kind: "dashboard" }, [view])).toEqual([]);
});

test("a page is kept by its key and found again, unless it is gone", () => {
  const view = views()[0];
  const camera = states()[0];

  for (const page of [{ kind: "dashboard" }, { kind: "cameras" }, { kind: "groups" }, { kind: "videos" }, { kind: "snapshots" }, { kind: "events" }] as const) {
    expect(pageFrom(pageKey(page))).toEqual(page);
  }
  expect(pageKey({ kind: "view", id: view.id })).toBe(`view:${view.id}`);
  expect(pageFrom(`view:${view.id}`)).toEqual({ kind: "view", id: view.id });
  expect(pageFrom(`camera:${camera.id}`)).toEqual({ kind: "camera", id: camera.id });
  const onSub = { kind: "camera", id: camera.id, stream: "/cam/realmonitor?channel=1&subtype=1" } as const;
  expect(pageFrom(pageKey(onSub))).toEqual(onSub);
  expect(pageKey({ kind: "camera", id: camera.id, stream: "" })).toBe(`camera:${camera.id}`);

  expect(pageFrom("view:deleted")).toBe(undefined);
  expect(pageFrom("camera:deleted")).toBe(undefined);
  expect(pageFrom("")).toBe(undefined);
  expect(pageFrom("gallery")).toBe(undefined);
  expect(pageKey({ kind: "none" })).toBe("");
});
