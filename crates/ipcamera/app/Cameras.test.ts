// The Cameras page's search, filters, order, selection and edits of several
// cameras at once, on cameras and groups made up here.
//
// `App.test.tsx` drives the page itself, over the one camera a test run starts
// with and the few it adds. These read `views/roster.ts` directly, which is all
// the logic the table shows, so they can have as many cameras as they like.
import { expect, test } from "nsh/test";
import { Cameras, type CameraState, type GroupInfo } from "ipcamera/native";
import { drawnAt, groups, playedIn, states, streamsOn, tilePlace, views } from "./views/model";
import { streamChoices } from "./views/streams";
import { pickLabel, streamLabel, streamMenu } from "./views/actions";
import {
  type CameraRow,
  changesToSave,
  chosen,
  clearFilters,
  clearSelection,
  commonOf,
  copyOf,
  currentPage,
  filtered,
  groupFilter,
  keepShown,
  onPage,
  pageCount,
  pageSize,
  pageSpan,
  pick,
  pickOf,
  query,
  type Saved,
  saved,
  selectAll,
  selected,
  shown,
  sort,
  sortBy,
  statusFilter,
  streamFilter,
  summary,
  togglePage,
  turnTo,
} from "./views/roster";

function state(id: string, name: string, more: Partial<CameraState> = {}): CameraState {
  return {
    id,
    name,
    streamLabel: "",
    address: "",
    streams: [],
    connected: false,
    ready: true,
    status: "Idle.",
    tone: "normal",
    stats: "",
    ptzStatus: "",
    muted: false,
    volume: 80,
    recording: false,
    recordProblem: "",
    motion: false,
    detected: [],
    spotlight: false,
    spotlightOn: false,
    watchStatus: "",
    ...more,
  };
}

function settings(id: string, name: string, more: Partial<Saved> = {}): Saved {
  return {
    id,
    name,
    host: "10.0.0.1",
    user: "admin",
    hasPassword: true,
    rtspPort: "554",
    channel: "1",
    subStream: 0,
    rtspPath: "",
    streams: [],
    useTcp: true,
    autoReconnect: true,
    onvifPort: "80",
    dvripPort: "37777",
    onvifPath: "/onvif/device_service",
    brand: "dahua",
    apiPort: "80",
    apiHttps: false,
    httpApi: false,
    onvifEvents: false,
    onvifRefused: false,
    logEvents: true,
    ownEventVideo: false,
    eventVideo: { afterSeconds: 30, beforeSeconds: 10, longestMinutes: 10, motionContinues: true, stream: "own", kinds: ["person", "vehicle", "animal", "face", "package"] },
    ...more,
  };
}

/**
 * Five cameras, in the order they were added: two on one recorder that differ
 * only by channel, one on the sub stream over UDP, one waiting for its password
 * and in trouble, one recording; and two groups.
 */
function seed() {
  const list: [CameraState, Saved][] = [
    [state("a", "Camera 10"), settings("a", "Camera 10", { host: "10.0.0.5", channel: "2" })],
    [state("b", "Camera 2"), settings("b", "Camera 2", { host: "10.0.0.5", channel: "1" })],
    [state("c", "Driveway"), settings("c", "Driveway", { host: "10.0.0.20", user: "viewer", subStream: 1, useTcp: false })],
    [state("d", "Porch", { tone: "warn", ready: false }), settings("d", "Porch", { host: "", hasPassword: false })],
    [state("e", "Yard", { connected: true, recording: true }), settings("e", "Yard", { host: "10.0.0.9", rtspPort: "8554" })],
  ];
  states.set(list.map(([each]) => each));
  saved.set(list.map(([, each]) => each));
  const made: GroupInfo[] = [
    { id: "outside", name: "Outside", cameras: ["c", "d", "e"] },
    { id: "recorder", name: "Recorder", cameras: ["a", "b"] },
  ];
  groups.set(made);
  clearFilters();
  sort.set(null);
  clearSelection();
  pageSize.set(25);
  turnTo(0);
}

const names = (list: CameraRow[]) => list.map((row) => row.name);

test("every camera shows in the order it was added, with its groups by name", () => {
  seed();
  expect(names(shown())).toEqual(["Camera 10", "Camera 2", "Driveway", "Porch", "Yard"]);
  expect(shown()[2].groups).toEqual(["Outside"]);
  expect(filtered()).toBe(false);
});

test("the search wants every word, in a name, an address, a username or a group", () => {
  seed();
  query.set("10.0.0.5");
  expect(names(shown())).toEqual(["Camera 10", "Camera 2"]);
  query.set("viewer");
  expect(names(shown())).toEqual(["Driveway"]);
  query.set("outside  YARD");
  expect(names(shown())).toEqual(["Yard"]);
  // The port is part of the address.
  query.set(":8554");
  expect(names(shown())).toEqual(["Yard"]);
  expect(filtered()).toBe(true);
  expect(summary()).toBe("“:8554”");
});

test("the filters narrow by group, by status and by stream, and clear together", () => {
  seed();
  groupFilter.set("recorder");
  expect(names(shown())).toEqual(["Camera 10", "Camera 2"]);
  // Every camera is in a group.
  groupFilter.set("");
  expect(names(shown())).toEqual([]);
  groupFilter.set(null);

  statusFilter.set("password");
  expect(names(shown())).toEqual(["Porch"]);
  statusFilter.set("problem");
  expect(names(shown())).toEqual(["Porch"]);
  statusFilter.set("recording");
  expect(names(shown())).toEqual(["Yard"]);
  statusFilter.set("all");

  streamFilter.set("sub");
  groupFilter.set("outside");
  expect(names(shown())).toEqual(["Driveway"]);
  expect(summary()).toBe("Outside, Sub stream");

  clearFilters();
  expect(shown().length).toBe(5);
  expect(filtered()).toBe(false);
});

test("a column title sorts one way, then the other, then back to the order they were added", () => {
  seed();
  sortBy("name");
  // Numbers in names go by their value.
  expect(names(shown())).toEqual(["Camera 2", "Camera 10", "Driveway", "Porch", "Yard"]);
  sortBy("name");
  expect(names(shown())).toEqual(["Yard", "Porch", "Driveway", "Camera 10", "Camera 2"]);
  sortBy("name");
  expect(sort()).toBe(null);
  expect(names(shown())).toEqual(["Camera 10", "Camera 2", "Driveway", "Porch", "Yard"]);
});

test("a camera with no address sorts last whichever way, and the login puts those with no password first", () => {
  seed();
  sortBy("address");
  // Addresses go by their numbers: 10.0.0.9 before 10.0.0.20. Equal ones keep the order they were added.
  expect(names(shown())).toEqual(["Camera 10", "Camera 2", "Yard", "Driveway", "Porch"]);
  sortBy("address");
  expect(names(shown())).toEqual(["Driveway", "Yard", "Camera 10", "Camera 2", "Porch"]);

  sortBy("login");
  expect(names(shown())).toEqual(["Porch", "Camera 10", "Camera 2", "Yard", "Driveway"]);
  sortBy("stream");
  // By channel, then the main stream before the sub.
  expect(names(shown())).toEqual(["Camera 2", "Porch", "Yard", "Driveway", "Camera 10"]);
});

test("a press selects one, Control adds or drops one, and Shift adds a run", () => {
  seed();
  pick("b", pickOf({ shift: false, control: false, platform: false }));
  expect(selected()).toEqual(["b"]);
  pick("d", pickOf({ shift: false, control: true, platform: false }));
  expect(names(chosen())).toEqual(["Camera 2", "Porch"]);
  pick("d", "toggle");
  expect(names(chosen())).toEqual(["Camera 2"]);

  // From the last row pressed to this one, added to what is selected.
  pick("e", "toggle");
  pick("c", pickOf({ shift: true, control: false, platform: false }));
  expect(names(chosen())).toEqual(["Camera 2", "Driveway", "Porch", "Yard"]);

  pick("a", "only");
  expect(names(chosen())).toEqual(["Camera 10"]);
});

test("the title's checkbox selects every camera on the page, then none", () => {
  seed();
  query.set("10.0.0");
  togglePage();
  expect(names(chosen())).toEqual(["Camera 10", "Camera 2", "Driveway", "Yard"]);
  togglePage();
  expect(chosen()).toEqual([]);
});

test("rows come a page at a time, and the footer says which", () => {
  seed();
  pageSize.set(2);
  expect(pageCount()).toBe(3);
  expect(names(onPage())).toEqual(["Camera 10", "Camera 2"]);
  expect(pageSpan()).toEqual({ from: 1, to: 2, total: 5 });

  turnTo(2);
  expect(names(onPage())).toEqual(["Yard"]);
  expect(pageSpan()).toEqual({ from: 5, to: 5, total: 5 });
  // Turning past either end stays at it.
  turnTo(9);
  expect(currentPage()).toBe(2);
  turnTo(-1);
  expect(currentPage()).toBe(0);

  // Nothing matching is one empty page.
  query.set("nothing like it");
  expect(pageCount()).toBe(1);
  expect(pageSpan()).toEqual({ from: 0, to: 0, total: 0 });
});

test("a new search, filter, order or page size starts again at the first page, and deleting never leaves it past the last", () => {
  seed();
  pageSize.set(2);
  turnTo(2);
  sortBy("name");
  expect(currentPage()).toBe(0);

  turnTo(1);
  query.set("10.0.0");
  expect(currentPage()).toBe(0);
  query.set("");
  turnTo(2);
  pageSize.set(1);
  expect(currentPage()).toBe(0);

  // Four cameras left take two pages of two: the third page turned to is now the last there is.
  pageSize.set(2);
  turnTo(2);
  states.set(states().slice(0, 4));
  expect(currentPage()).toBe(1);
  expect(names(onPage())).toEqual(["Driveway", "Porch"]);
});

test("the title's checkbox leaves what is selected on other pages alone", () => {
  seed();
  pageSize.set(2);
  // Yard, on the last page.
  pick("e", "only");
  togglePage();
  expect(names(chosen())).toEqual(["Camera 10", "Camera 2", "Yard"]);
  togglePage();
  expect(names(chosen())).toEqual(["Yard"]);
});

test("a camera the filters hide leaves the selection, so nothing unseen is changed", () => {
  seed();
  selectAll();
  statusFilter.set("recording");
  keepShown();
  expect(selected()).toEqual(["e"]);
  // Clearing the filter doesn't bring them back.
  statusFilter.set("all");
  expect(names(chosen())).toEqual(["Yard"]);
});

test("several cameras show what they share, and what is saved leaves out what they already have", () => {
  seed();
  const recorder = saved().filter((each) => each.id === "a" || each.id === "b");
  const common = commonOf(recorder);
  expect(common.host).toBe("10.0.0.5");
  expect(common.user).toBe("admin");
  expect(common.useTcp).toBe(true);
  // Names and channels are each camera's own, so they are never shared.
  expect("name" in common).toBe(false);
  expect("channel" in common).toBe(false);

  const differ = commonOf(saved());
  expect(differ.host).toBeUndefined();
  expect(differ.subStream).toBeUndefined();
  expect(differ.dvripPort).toBe("37777");

  expect(changesToSave({ host: "10.0.0.5", user: "viewer", password: "", subStream: 1 }, common)).toEqual({ user: "viewer", subStream: 1 });
  expect(changesToSave({ password: "s3cret" }, common)).toEqual({ password: "s3cret" });
  expect(changesToSave({ useTcp: true }, common)).toEqual({});
});

test("a copy takes the next channel its address doesn't use, and a name of its own", () => {
  // A recorder with channels 1, 2 and 4, a camera elsewhere on channel 3, and a camera with no channel written.
  const taken = [
    { name: "Dock", host: "192.0.2.50", channel: "1" },
    { name: "Dock copy", host: "192.0.2.50", channel: "2" },
    { name: "Gate", host: " 192.0.2.50 ", channel: "4" },
    { name: "Porch", host: "192.0.2.23", channel: "3" },
    { name: "Shed", host: "SHED.local", channel: "" },
  ];

  // Channel 3 is the next free on this address, whatever another address uses.
  expect(copyOf(taken[0], taken)).toEqual({ name: "Dock copy 2", host: "192.0.2.50", channel: "3" });
  // After 4, past the ones taken.
  expect(copyOf(taken[2], taken).channel).toBe("5");
  // A copy of a copy is named after the original.
  expect(copyOf(taken[1], taken).name).toBe("Dock copy 2");
  // A blank channel is channel 1, and the address matches whatever its case.
  expect(copyOf(taken[4], [...taken, { name: "Shed 2", host: "shed.local", channel: "2" }])).toEqual({ name: "Shed copy", host: "SHED.local", channel: "3" });
});

test("the stream choice offers what the camera listed, and nothing for a camera that listed none", () => {
  expect(streamChoices({ streams: [], rtspPath: "/typed" })).toEqual([]);

  // As the bridge hands them over: titled and in the lists' order.
  const streams = [
    { name: "main", path: "/a", detail: "1920×1080, H264", title: "1920×1080 · H.264 · main", lens: "" },
    { name: "second", path: "/b?x=1", detail: "", title: "second", lens: "" },
    { name: "autotrack_sub", path: "flv:channel0_autotrack_sub.bcs", detail: "896×512, H264", title: "896×512 · H.264 · autotrack_sub", lens: "Tracking lens" },
  ];
  expect(streamChoices({ streams, rtspPath: "" })).toEqual([
    { value: "/a", title: "1920×1080 · H.264 · main" },
    { value: "/b?x=1", title: "second" },
    { value: "flv:channel0_autotrack_sub.bcs", title: "896×512 · H.264 · autotrack_sub · Tracking lens" },
  ]);
  // A path typed by hand that the camera didn't list stays a choice, so it isn't lost.
  expect(streamChoices({ streams, rtspPath: "/own" }).at(-1)).toEqual({ value: "/own", title: "Saved path: /own" });
  expect(streamChoices({ streams, rtspPath: "/a" })).toHaveLength(3);
});

/** Adds a camera as Connect saves one with a tracking lens: ONVIF's three, and one it sends only over FLV. On its tracking lens's main stream. */
function addYard() {
  const listed = (name: string, path: string, detail: string) => ({ name, path, detail, title: "", lens: "" });
  return Cameras.add({
    ...Cameras.withBrand({ ...Cameras.blankForm(), name: "Yard", host: "10.0.0.64", user: "admin" }, "reolink"),
    rtspPath: "/Preview_01_autotrack",
    streams: [
      listed("Profile001_SubStream", "/Preview_01_sub", "896×512, H264"),
      listed("Profile000_MainStream", "/Preview_01_main", "3840×2160, H265"),
      listed("Profile003_AutotrackStream", "/Preview_01_autotrack", "1920×1080, H264"),
      listed("autotrack_sub", "flv:channel0_autotrack_sub.bcs", "896×512, H264"),
    ],
  });
}

test("a picture's stream menu has Auto, then every stream once, sized and largest first, its default marked, each lens under its own heading", async () => {
  const yard = await addYard();
  try {
    const picks: string[] = [];
    const menu = (current: string, played?: string) => streamMenu(yard, { current, played, pick: (stream) => picks.push(stream) });
    const rows = (current: string, played?: string) =>
      menu(current, played).map((entry) => (!entry ? "" : "label" in entry ? `${entry.checked ? "✓ " : ""}${entry.label}` : "—"));
    expect(rows("", "flv:channel0_autotrack_sub.bcs")).toEqual([
      "✓ Auto · 896×512",
      "—",
      "Main lens",
      "3840×2160 · H.265 · Profile000_MainStream",
      "896×512 · H.264 · Profile001_SubStream",
      "—",
      "Tracking lens",
      "1920×1080 · H.264 · Profile003_AutotrackStream (default)",
      "896×512 · H.264 · autotrack_sub",
    ]);
    // Before its picture has a size, Auto has picked nothing yet.
    expect(rows("")[0]).toBe("✓ Auto");
    // The default, picked, is the camera's own whichever it is then.
    expect(rows("own")[7]).toBe("✓ 1920×1080 · H.264 · Profile003_AutotrackStream (default)");
    // A tile on the brand's sub stream, saved by either name, is on the listed one.
    expect(rows("/Preview_01_sub")[4]).toBe("✓ 896×512 · H.264 · Profile001_SubStream");
    expect(rows("sub")[4]).toBe("✓ 896×512 · H.264 · Profile001_SubStream");
    const select = (index: number) => {
      const entry = menu("sub")[index];
      if (entry && "onSelect" in entry) entry.onSelect?.();
    };
    select(0);
    select(7);
    select(8);
    expect(picks).toEqual(["", "own", "flv:channel0_autotrack_sub.bcs"]);
    // The picker says what is on by its size, and on Auto that it is.
    expect(streamLabel(yard, "")).toBe("1920×1080");
    expect(streamLabel(yard, "flv:channel0_autotrack_sub.bcs")).toBe("896×512");
    expect(pickLabel(yard, { current: "", played: "", pick: () => {} })).toBe("Auto · 1920×1080");
    expect(pickLabel(yard, { current: "own", played: "", pick: () => {} })).toBe("1920×1080");
  } finally {
    await Cameras.delete(yard);
  }
});

test("a camera's tiles on Auto play one stream of its lens, the one its largest calls for, once it has a size", async () => {
  const yard = await addYard();
  try {
    expect(Cameras.autoPicks(yard)).toBe(true);
    const view = { ...views()[0], id: "auto-test", tiles: [yard, yard, yard], streams: ["", "", "sub"] };
    // Until a tile is drawn, Auto has picked nothing, so nothing connects only to leave.
    expect(playedIn(view)).toEqual([undefined, undefined, "sub"]);
    expect(streamsOn({ kind: "view", id: view.id }, [view])).toEqual([{ camera: yard, stream: "sub" }]);

    // A small tile: the tracking lens's small stream, for both tiles on Auto, at any screen density.
    drawnAt(tilePlace(view.id, 0), { width: 200, height: 112 }, 1, "original");
    expect(playedIn(view)).toEqual(["flv:channel0_autotrack_sub.bcs", "flv:channel0_autotrack_sub.bcs", "sub"]);
    // A large one too: the stream that one calls for, for both.
    drawnAt(tilePlace(view.id, 1), { width: 1600, height: 900 }, 1, "original");
    expect(playedIn(view)).toEqual(["", "", "sub"]);
    expect(streamsOn({ kind: "view", id: view.id }, [view])).toEqual([
      { camera: yard, stream: "" },
      { camera: yard, stream: "sub" },
    ]);

    // A camera with no sizes to go by plays its own on Auto at once.
    const plain = states().find((camera) => camera.id !== yard)?.id ?? "";
    if (plain !== "" && !Cameras.autoPicks(plain)) expect(playedIn({ ...view, tiles: [plain], streams: [""] })).toEqual([""]);
  } finally {
    await Cameras.delete(yard);
  }
});
