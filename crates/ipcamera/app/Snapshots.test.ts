// The gallery's filtering, sorting, grouping, sizes and selection, on
// snapshots made up here. The videos and events pages share all of it
// (`views/library.ts`, `views/grid.tsx`).
//
// `App.test.tsx` covers the page around them, but it can only open the gallery
// empty: nsh grants a test no file system, so it cannot put PNGs where the app
// reads them, and widening the app's own grant for a test isn't worth it. These
// read `views/snapshots.ts` directly, which is all the logic the page shows.
import { expect, test } from "nsh/test";
import type { SnapshotInfo } from "ipcamera/native";
import { columnsFor } from "./views/grid";
import { rowsOf, sizeLabel } from "./views/library";
import { states } from "./views/model";
import {
  cameraFilter,
  clearFilters,
  filtered,
  grouping,
  order,
  ordered,
  period,
  sections,
  snapshotLibrary,
  snapshots,
  snapshotsLabel,
  tileMenu,
  visible,
} from "./views/snapshots";

const DAY = 86_400_000;

/** The camera the throwaway settings start with. */
const camera = () => states()[0].id;

function shot(name: string, takenAt: number, cameraId: string, tags: string[] = [], event = ""): SnapshotInfo {
  return { name, path: `C:\\snapshots\\${name}.png`, thumbnail: "", camera: cameraId, tags, event, takenAt, bytes: 1024, width: 1920, height: 1080 };
}

/** Noon today, so a few hours either way stays on the same day. */
function noon(): number {
  const now = new Date();
  return new Date(now.getFullYear(), now.getMonth(), now.getDate(), 12).getTime();
}

/** Four snapshots: two of the camera, one of a camera since deleted, one put in the folder by hand, which names no camera. */
function seed() {
  const today = noon();
  snapshots.set([
    shot("today", today, camera()),
    shot("yesterday", today - DAY, camera()),
    shot("months-ago", today - 60 * DAY, "gone"),
    shot("last-year", today - 400 * DAY, ""),
  ]);
  clearFilters();
  order.set("newest");
  grouping.set("month");
  snapshotLibrary.clearSelection();
  snapshotLibrary.collapsed.set([]);
}

/** A press with no key held. */
const plain = { shift: false, control: false, platform: false };

const names = (list: SnapshotInfo[]) => list.map((each) => each.name);
const month = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" });

test("snapshots fall under a heading for each month, newest first", () => {
  seed();
  const today = noon();
  const titles = sections().map((section) => section.title);
  const expected = [...new Set([today, today - DAY, today - 60 * DAY, today - 400 * DAY].map((ms) => month.format(new Date(ms))))];

  expect(titles).toEqual(expected);
  expect(names(ordered())).toEqual(["today", "yesterday", "months-ago", "last-year"]);
});

test("by day, today and yesterday say so", () => {
  seed();
  grouping.set("day");
  const titles = sections().map((section) => section.title);

  expect(titles.slice(0, 2)).toEqual(["Today", "Yesterday"]);
  expect(titles.length).toBe(4);
});

test("by camera, cameras go by name, with the snapshots no camera claims last", () => {
  seed();
  grouping.set("camera");

  expect(sections().map((section) => section.title)).toEqual(["Camera 1", "Deleted camera", "Unknown camera"]);
  expect(names(sections()[0].items)).toEqual(["today", "yesterday"]);
  // The viewer steps through them in the page's order, sections and all.
  expect(names(ordered())).toEqual(["today", "yesterday", "months-ago", "last-year"]);
});

test("ungrouped, there is one section with no heading, and no heading row", () => {
  seed();
  grouping.set("none");

  expect(sections().length).toBe(1);
  expect(sections()[0].title).toBe("");
  expect(rowsOf(sections(), 3).map((row) => row.kind)).toEqual(["tiles", "tiles"]);
});

test("oldest first turns every order around", () => {
  seed();
  order.set("oldest");

  expect(names(ordered())).toEqual(["last-year", "months-ago", "yesterday", "today"]);
  expect(month.format(new Date(noon() - 400 * DAY))).toBe(sections()[0].title);
});

test("the period and camera filters narrow, and clear together", () => {
  seed();
  expect(filtered()).toBe(false);

  period.set("today");
  expect(names(visible())).toEqual(["today"]);
  period.set("week");
  expect(names(visible())).toEqual(["today", "yesterday"]);
  period.set("year");
  expect(names(visible())).not.toContain("last-year");

  period.set("all");
  cameraFilter.set("");
  expect(names(visible())).toEqual(["last-year"]);
  expect(filtered()).toBe(true);

  period.set("week");
  cameraFilter.set("gone");
  expect(visible().length).toBe(0);
  expect(sections().length).toBe(0);

  clearFilters();
  expect(visible().length).toBe(4);
  expect(filtered()).toBe(false);
});

test("the Tag filter keeps those taken by hand, those events took, or one thing the camera saw", () => {
  seed();
  const today = noon();
  snapshots.set([
    ...snapshots(),
    shot("car", today - 1, camera(), ["vehicle"], "e1"),
    shot("car-and-driver", today - 2, camera(), ["person", "vehicle"], "e2"),
  ]);
  expect(snapshotLibrary.tags()).toEqual({
    hand: 4,
    event: 2,
    each: [
      { key: "person", title: "Person", count: 1 },
      { key: "vehicle", title: "Vehicle", count: 2 },
    ],
  });

  snapshotLibrary.tagFilter.set("vehicle");
  expect(names(visible())).toEqual(["car", "car-and-driver"]);
  expect(filtered()).toBe(true);
  expect(snapshotLibrary.summary()).toContain("Vehicle");
  snapshotLibrary.tagFilter.set("hand");
  expect(visible().length).toBe(4);
  snapshotLibrary.tagFilter.set("event");
  expect(names(visible())).toEqual(["car", "car-and-driver"]);

  // An event's Show in Snapshots narrows to what it saved.
  snapshotLibrary.tagFilter.set("all");
  snapshotLibrary.eventFilter.set("e2");
  expect(names(visible())).toEqual(["car-and-driver"]);
  clearFilters();
  expect(visible().length).toBe(6);
  expect(filtered()).toBe(false);
});

test("the camera filter offers only cameras with snapshots, by name, each with how many", () => {
  seed();
  expect(snapshotLibrary.cameras()).toEqual([
    { id: camera(), name: "Camera 1", count: 2 },
    { id: "gone", name: "Deleted camera", count: 1 },
    { id: "", name: "Unknown camera", count: 1 },
  ]);
});

test("rows hold as many pictures as fit, and each section starts with its heading", () => {
  seed();
  grouping.set("camera");
  const rows = rowsOf(sections(), 1);

  expect(rows.map((row) => (row.kind === "heading" ? `# ${row.title} (${row.count})` : names(row.items).join(" ")))).toEqual([
    "# Camera 1 (2)",
    "today",
    "yesterday",
    "# Deleted camera (1)",
    "months-ago",
    "# Unknown camera (1)",
    "last-year",
  ]);
  // Wider, a section's pictures share a row, and each row is known by its first picture.
  const wide = rowsOf(sections(), 4).filter((row) => row.kind === "tiles");
  expect(wide.map((row) => (row.kind === "tiles" ? row.items.length : 0))).toEqual([2, 1, 1]);
  expect(new Set(rows.map((row) => row.key)).size).toBe(rows.length);
});

test("a heading folds its section away, keeping the heading, and opens it again", () => {
  seed();
  grouping.set("camera");
  const camera1 = sections()[0].key;
  const shown = () =>
    rowsOf(sections(), 1, snapshotLibrary.isCollapsed).map((row) => (row.kind === "heading" ? `# ${row.title}${row.open ? "" : " (folded)"}` : names(row.items).join(" ")));

  snapshotLibrary.toggleCollapsed(camera1);
  expect(snapshotLibrary.isCollapsed(camera1)).toBe(true);
  expect(shown()).toEqual(["# Camera 1 (folded)", "# Deleted camera", "months-ago", "# Unknown camera", "last-year"]);
  // Folded, its snapshots are still the page's: the viewer steps through them, and a heading's checkbox selects them.
  expect(names(ordered())).toEqual(["today", "yesterday", "months-ago", "last-year"]);
  snapshotLibrary.toggleSection(camera1);
  expect(names(snapshotLibrary.chosen())).toEqual(["today", "yesterday"]);

  // Another grouping has sections of its own; back by camera, the section is folded as it was left.
  grouping.set("month");
  expect(shown().some((row) => row.endsWith("(folded)"))).toBe(false);
  grouping.set("camera");
  expect(shown()[0]).toBe("# Camera 1 (folded)");

  snapshotLibrary.toggleCollapsed(camera1);
  expect(shown()).toEqual(["# Camera 1", "today", "yesterday", "# Deleted camera", "months-ago", "# Unknown camera", "last-year"]);

  // Ungrouped, there is no heading to open it again by, so nothing folds.
  grouping.set("none");
  snapshotLibrary.toggleCollapsed(sections()[0].key);
  expect(shown()).toEqual(["today", "yesterday", "months-ago", "last-year"]);
});

test("a press opens a snapshot until one is selected, then picks as Explorer does", () => {
  seed();
  const [today, yesterday, , lastYear] = ordered();
  let opened = "";
  snapshotLibrary.press(today, plain, () => (opened = today.name));
  expect(opened).toBe("today");
  expect(snapshotLibrary.selecting()).toBe(false);

  // Control picks without opening, and from then on a plain press picks too.
  snapshotLibrary.press(yesterday, { ...plain, control: true }, () => (opened = "yesterday"));
  snapshotLibrary.press(lastYear, plain, () => (opened = "last-year"));
  expect(opened).toBe("today");
  expect(names(snapshotLibrary.chosen())).toEqual(["yesterday", "last-year"]);

  // Shift adds every one from the last picked, in the page's order; a plain press takes one out.
  snapshotLibrary.press(today, { ...plain, shift: true }, () => {});
  expect(names(snapshotLibrary.chosen())).toEqual(["today", "yesterday", "months-ago", "last-year"]);
  snapshotLibrary.press(yesterday, plain, () => {});
  expect(names(snapshotLibrary.chosen())).toEqual(["today", "months-ago", "last-year"]);

  // With nothing selected again, a press opens.
  snapshotLibrary.clearSelection();
  snapshotLibrary.press(lastYear, plain, () => (opened = "last-year"));
  expect(opened).toBe("last-year");
});

test("a heading's checkbox selects its section, shows when only some are, and takes them all out", () => {
  seed();
  grouping.set("camera");
  const camera1 = sections()[0].key;
  const heading = rowsOf(sections(), 4)[0];
  expect(heading.kind === "heading" ? heading.section : "").toBe(camera1);
  expect(snapshotLibrary.sectionPicked(camera1)).toBe("none");

  snapshotLibrary.pick("today", "toggle");
  expect(snapshotLibrary.sectionPicked(camera1)).toBe("some");

  snapshotLibrary.toggleSection(camera1);
  expect(snapshotLibrary.sectionPicked(camera1)).toBe("all");
  expect(names(snapshotLibrary.chosen())).toEqual(["today", "yesterday"]);
  expect(snapshotLibrary.sectionPicked(sections()[1].key)).toBe("none");

  snapshotLibrary.toggleSection(camera1);
  expect(snapshotLibrary.selecting()).toBe(false);
});

test("Select all takes what the filters keep, and the selection drops what they hide", () => {
  seed();
  snapshotLibrary.selectAll();
  expect(snapshotLibrary.chosen().length).toBe(4);

  cameraFilter.set("gone");
  snapshotLibrary.keepShown();
  expect(names(snapshotLibrary.chosen())).toEqual(["months-ago"]);

  // What a filter hid doesn't come back selected with it.
  clearFilters();
  expect(names(snapshotLibrary.chosen())).toEqual(["months-ago"]);
});

test("a right press on one of several selected offers to delete them all, and on any other its own menu", () => {
  seed();
  const [today, yesterday, , lastYear] = ordered();
  expect(tileMenu(today)[0]).toMatchObject({ label: "Open" });

  snapshotLibrary.pick("today", "toggle");
  expect(tileMenu(today)[0]).toMatchObject({ label: "Open" });
  snapshotLibrary.pick("yesterday", "toggle");
  expect(tileMenu(yesterday)[0]).toMatchObject({ label: "Delete (2)…" });
  expect(tileMenu(lastYear)[0]).toMatchObject({ label: "Open" });
});

test("each size fits its own number of pictures across, and a list one to a row", () => {
  expect(columnsFor(1100, "large")).toBe(3);
  expect(columnsFor(1100, "medium")).toBe(4);
  expect(columnsFor(1100, "small")).toBe(7);
  expect(columnsFor(1100, "list")).toBe(1);
  // Never none, and never more than a size holds however wide.
  expect(columnsFor(300, "large")).toBe(1);
  expect(columnsFor(4000, "small")).toBe(12);
  expect(snapshotLibrary.size()).toBe("medium");
});

test("sizes read as Explorer shows them, and counts as words", () => {
  expect(sizeLabel(512)).toBe("512 bytes");
  expect(sizeLabel(1536)).toBe("1.5 KB");
  expect(sizeLabel(3 * 1024 * 1024)).toBe("3.0 MB");
  expect(sizeLabel(412 * 1024 * 1024)).toBe("412 MB");
  expect(snapshotsLabel(1)).toBe("1 snapshot");
  expect(snapshotsLabel(1234)).toBe("1,234 snapshots");
});
