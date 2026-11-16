// The menus in Settings' Automation section (`views/automation.tsx`): what each
// event saves, how long it stays open, how its videos are made
// (`views/eventvideo.ts`, which a camera's Events part shares), and the storage
// limits; what each choice reads as, and a hand-edited number kept among them.
import { expect, test } from "nsh/test";
import { Automation } from "ipcamera/native";
import { CAPTURES, limitItems, limitLabel, windowItems, windowLabel } from "./views/automation";
import { beforeLabel, cameraStreams, KIND_COLUMNS, KINDS, longestLabel, numberItems, secondsLabel, withKind } from "./views/eventvideo";
import { NEAR_LIMIT, shareOfLimit } from "./views/statusbar";

test("event videos keep thirty seconds after, start ten before and split at ten minutes until the user picks", () => {
  const video = Automation.settings().eventVideo;
  expect(video.afterSeconds).toBe(30);
  expect(video.beforeSeconds).toBe(10);
  expect(video.longestMinutes).toBe(10);
  expect(video.motionContinues).toBe(true);
  expect(video.stream).toBe("own");
  expect(video.kinds).toEqual(KINDS.map((kind) => kind.key));
});

test("event video choices read in seconds and minutes, with Off and No limit for none", () => {
  expect(secondsLabel(10)).toBe("10 seconds");
  expect(secondsLabel(60)).toBe("1 minute");
  expect(secondsLabel(90)).toBe("90 seconds");
  expect(secondsLabel(120)).toBe("2 minutes");
  expect(beforeLabel(0)).toBe("Off");
  expect(beforeLabel(5)).toBe("5 seconds");
  expect(longestLabel(0)).toBe("No limit");
  expect(longestLabel(1)).toBe("1 minute");
  expect(numberItems([10, 30], 25, secondsLabel).map((item) => item.value)).toEqual(["10", "25", "30"]);
});

test("the kinds that save a video stay in the app's order, and a camera offers the streams it listed", () => {
  const video = Automation.settings().eventVideo;
  const animals = withKind({ ...video, kinds: [] }, "animal", true);
  expect(animals.kinds).toEqual(["animal"]);
  expect(withKind(animals, "person", true).kinds).toEqual(["person", "animal"]);
  expect(withKind(animals, "animal", false).kinds).toEqual([]);
  // Save videos of lists every kind once, people over animals, vehicles over faces, then packages.
  expect(KIND_COLUMNS.map((column) => column.map((kind) => kind.title))).toEqual([["People", "Animals"], ["Vehicles", "Faces"], ["Packages"]]);
  expect(KIND_COLUMNS.flat().length).toBe(KINDS.length);

  // A camera that listed none: its own, or its brand's main or sub stream.
  expect(cameraStreams({ streams: [], eventVideo: video }).map((each) => each.value)).toEqual(["own", "main", "sub"]);
  const listed = { name: "main", path: "/live/1", detail: "", title: "1920×1080 · H.264 · main", lens: "" };
  // One that listed some offers those, and keeps a saved one it no longer lists.
  expect(cameraStreams({ streams: [listed], eventVideo: { ...video, stream: "/gone" } }).map((each) => each.value)).toEqual(["own", "/live/1", "/gone"]);
});

test("an event saves a picture and groups within five minutes until the user picks", () => {
  const settings = Automation.settings();
  expect(settings.eventCapture).toBe("picture");
  expect(settings.eventWindowMinutes).toBe(5);
  expect(CAPTURES.map((each) => each.title)).toEqual(["A picture", "A video", "Picture and video", "Nothing"]);
});

test("a window reads in minutes, 0 groups nothing, and one set by hand stays among them", () => {
  expect(windowLabel(0)).toBe("Don't group");
  expect(windowLabel(1)).toBe("1 minute");
  expect(windowLabel(5)).toBe("5 minutes");
  expect(windowItems(5).map((item) => item.title)).toEqual(["Don't group", "1 minute", "2 minutes", "5 minutes", "10 minutes", "15 minutes", "30 minutes"]);
  expect(windowItems(7).map((item) => item.value)).toContain("7");
});

test("a limit reads as the system counts sizes, and 0 is none", () => {
  expect(limitLabel(0)).toBe("No limit");
  expect(limitLabel(250)).toBe("250 MB");
  expect(limitLabel(5 * 1024)).toBe("5 GB");
  expect(limitLabel(1536)).toBe("1.5 GB");
  expect(limitLabel(2 * 1024 * 1024)).toBe("2 TB");
});

test("a limit set by hand stays among the choices, in order", () => {
  expect(limitItems([0, 100, 250], 250).map((item) => item.title)).toEqual(["No limit", "100 MB", "250 MB"]);
  expect(limitItems([0, 100, 250], 300).map((item) => item.value)).toEqual(["0", "100", "250", "300"]);
});

test("the status bar's gauge fills with the media against their limit, and holds at full", () => {
  const usage = { videos: 3 * 1024, snapshots: 1024, free: 0, drive: "", limit: 8 * 1024, makeRoom: false };
  expect(shareOfLimit(usage)).toBe(0.5);
  expect(shareOfLimit({ ...usage, videos: 7.5 * 1024 }) >= NEAR_LIMIT).toBe(true);
  expect(shareOfLimit({ ...usage, videos: 100 * 1024 })).toBe(1);
  expect(shareOfLimit({ ...usage, limit: 0 })).toBe(0);
});
