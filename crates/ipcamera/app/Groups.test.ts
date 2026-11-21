import { expect, test } from "nsh/test";
import type { CameraState, GroupInfo } from "ipcamera/native";
import {
  cameraFilter,
  chosen,
  clearFilters,
  clearSelection,
  emptyCount,
  filtered,
  type GroupRow,
  grouping,
  membersOf,
  pageSize,
  pick,
  query,
  selectAll,
  shown,
  sort,
  sortBy,
  statusFilter,
  statusOf,
  summary,
  turnTo,
} from "./views/grouplist";
import { copyName } from "./views/listing";
import { groups, states } from "./views/model";

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

function seed() {
  states.set([
    state("a", "Camera 10", { connected: true, tone: "ok" }),
    state("b", "Camera 2"),
    state("c", "Driveway", { connected: true, tone: "ok", recording: true }),
    state("d", "Porch", { tone: "warn", ready: false }),
    state("e", "Shed"),
  ]);
  const made: GroupInfo[] = [
    { id: "outside", name: "Outside", cameras: ["c", "d"] },
    { id: "recorder", name: "Recorder 10", cameras: ["a", "b"] },
    { id: "spare", name: "Spare", cameras: [] },
    { id: "front", name: "Recorder 2", cameras: ["gone", "c"] },
  ];
  groups.set(made);
  clearFilters();
  sort.set(null);
  clearSelection();
  pageSize.set(25);
  turnTo(0);
}

const names = (list: GroupRow[]) => list.map((row) => row.name);

test("every group shows in the sidebar's order, with its cameras by name and what they are doing", () => {
  seed();
  expect(names(shown())).toEqual(["Outside", "Recorder 10", "Spare", "Recorder 2"]);
  expect(membersOf(shown()[0])).toBe("Driveway, Porch");
  expect(membersOf(shown()[3])).toBe("Driveway");
  expect(shown()[3].cameras.length).toBe(1);

  expect(statusOf(shown()[0])).toEqual({ text: "1 has a problem", tone: "warn" });
  expect(statusOf(shown()[1])).toEqual({ text: "1 of 2 connected", tone: "ok" });
  expect(statusOf(shown()[2])).toEqual({ text: "No cameras", tone: "normal" });
  expect(shown()[0].recording).toBe(1);
  expect(filtered()).toBe(false);
});

test("the header counts the cameras in a group, those in none, and the empty groups", () => {
  seed();
  expect(grouping()).toEqual({ grouped: 4, ungrouped: 1 });
  expect(emptyCount()).toBe(1);
});

test("the search wants every word, in a group's name or its cameras' names", () => {
  seed();
  query.set("recorder");
  expect(names(shown())).toEqual(["Recorder 10", "Recorder 2"]);
  query.set("recorder  DRIVEWAY");
  expect(names(shown())).toEqual(["Recorder 2"]);
  query.set("porch");
  expect(names(shown())).toEqual(["Outside"]);
  expect(filtered()).toBe(true);
  expect(summary()).toBe("“porch”");
});

test("the filters narrow by camera and by status, and clear together", () => {
  seed();
  cameraFilter.set("c");
  expect(names(shown())).toEqual(["Outside", "Recorder 2"]);
  expect(summary()).toBe("With Driveway");

  statusFilter.set("recording");
  expect(names(shown())).toEqual(["Outside", "Recorder 2"]);
  statusFilter.set("problem");
  expect(names(shown())).toEqual(["Outside"]);
  expect(summary()).toBe("With Driveway, Has a problem");
  cameraFilter.set(null);

  statusFilter.set("password");
  expect(names(shown())).toEqual(["Outside"]);
  statusFilter.set("empty");
  expect(names(shown())).toEqual(["Spare"]);

  clearFilters();
  expect(shown().length).toBe(4);
  expect(filtered()).toBe(false);
});

test("a column title sorts one way, then the other, then back to the sidebar's order", () => {
  seed();
  sortBy("name");
  expect(names(shown())).toEqual(["Outside", "Recorder 2", "Recorder 10", "Spare"]);
  sortBy("name");
  expect(names(shown())).toEqual(["Spare", "Recorder 10", "Recorder 2", "Outside"]);
  sortBy("name");
  expect(sort()).toBe(null);
  expect(names(shown())).toEqual(["Outside", "Recorder 10", "Spare", "Recorder 2"]);

  sortBy("cameras");
  expect(names(shown())).toEqual(["Spare", "Recorder 2", "Outside", "Recorder 10"]);
});

test("the status puts the groups with a problem first, and an empty group last whichever way", () => {
  seed();
  sortBy("status");
  expect(names(shown())).toEqual(["Outside", "Recorder 10", "Recorder 2", "Spare"]);
  sortBy("status");
  expect(names(shown())).toEqual(["Recorder 2", "Recorder 10", "Outside", "Spare"]);

  sortBy("members");
  expect(names(shown())).toEqual(["Recorder 10", "Recorder 2", "Outside", "Spare"]);
});

test("groups are selected as the cameras are, and the filters drop what they hide", () => {
  seed();
  pick("outside", "only");
  pick("spare", "range");
  expect(names(chosen())).toEqual(["Outside", "Recorder 10", "Spare"]);
  pick("recorder", "toggle");
  expect(names(chosen())).toEqual(["Outside", "Spare"]);

  selectAll();
  statusFilter.set("empty");
  expect(names(chosen())).toEqual(["Spare"]);
});

test("a copy is named after the original, numbered when that name is taken", () => {
  const taken = ["Outside", "Outside copy", "Garage"];
  expect(copyName("Outside", taken)).toBe("Outside copy 2");
  expect(copyName("Outside copy", taken)).toBe("Outside copy 2");
  expect(copyName(" Garage ", taken)).toBe("Garage copy");
});
