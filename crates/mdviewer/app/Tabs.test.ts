// The tabs as plain values (`views/tabs.ts`): opening, the preview, pinning,
// closing, moving, each tab's mode, what the project saves, and the way to the
// file in front that the breadcrumbs show.
import { expect, test } from "nsh/test";
import {
  close,
  closeOthers,
  closeToTheRight,
  closeUnpinned,
  keep,
  label,
  move,
  movesWithin,
  NO_TABS,
  open,
  openAll,
  pathSteps,
  prune,
  restore,
  saved,
  setMode,
  setPinned,
  step,
  type TabSet,
} from "./views/tabs";

const paths = (set: TabSet) => set.tabs.map((tab) => `${tab.pinned ? "^" : ""}${tab.path}${tab.preview ? "?" : ""}`);

/** README pinned, then a.md, b.md and c.md kept, b.md in front. */
const FOUR: TabSet = {
  tabs: [
    { path: "README.md", pinned: true, preview: false, mode: "rendered" },
    { path: "a.md", pinned: false, preview: false, mode: "rendered" },
    { path: "b.md", pinned: false, preview: false, mode: "raw" },
    { path: "c.md", pinned: false, preview: false, mode: "rendered" },
  ],
  front: "b.md",
};

test("a single press opens a preview, and the next one takes its place", () => {
  let set = open(NO_TABS, "a.md", "preview", "rendered");
  expect(paths(set)).toEqual(["a.md?"]);
  set = open(set, "b.md", "preview", "raw");
  expect(paths(set)).toEqual(["b.md?"]);
  expect(set.front).toBe("b.md");
  expect(set.tabs[0].mode).toBe("raw");
});

test("a kept file opens after the one in front, and keeping the preview keeps its place", () => {
  let set = open(NO_TABS, "a.md", "keep", "rendered");
  set = open(set, "b.md", "preview", "rendered");
  set = open(set, "c.md", "keep", "rendered");
  expect(paths(set)).toEqual(["a.md", "b.md?", "c.md"]);
  set = open(set, "b.md", "keep", "rendered");
  expect(paths(set)).toEqual(["a.md", "b.md", "c.md"]);
  expect(set.front).toBe("b.md");
  expect(keep(set, "b.md")).toBe(set);
});

test("an open file comes to the front, and nothing else changes", () => {
  const set = open(FOUR, "a.md", "preview", "split");
  expect(set.tabs).toBe(FOUR.tabs);
  expect(set.front).toBe("a.md");
  expect(open(FOUR, "b.md", "preview", "split")).toBe(FOUR);
});

test("new tabs open past the pinned ones", () => {
  const set = open({ ...FOUR, front: "README.md" }, "new.md", "keep", "rendered");
  expect(paths(set)).toEqual(["^README.md", "new.md", "a.md", "b.md", "c.md"]);
});

test("several files open kept, in their order, the first in front", () => {
  const set = openAll(NO_TABS, ["x.md", "y.md", "z.md"], "rendered");
  expect(paths(set)).toEqual(["x.md", "y.md", "z.md"]);
  expect(set.front).toBe("x.md");
  expect(openAll(FOUR, [], "rendered")).toBe(FOUR);
});

test("closing the tab in front brings the next one forward, else the one before", () => {
  let set = close(FOUR, "b.md");
  expect(paths(set)).toEqual(["^README.md", "a.md", "c.md"]);
  expect(set.front).toBe("c.md");
  set = close(set, "c.md");
  expect(set.front).toBe("a.md");
  set = close(close(set, "a.md"), "README.md");
  expect(set).toEqual(NO_TABS);
  expect(close(FOUR, "gone.md")).toBe(FOUR);
});

test("closing others, to the right and all but pinned leaves the pinned tabs open", () => {
  expect(paths(closeOthers(FOUR, "c.md"))).toEqual(["^README.md", "c.md"]);
  expect(closeOthers(FOUR, "c.md").front).toBe("c.md");
  expect(paths(closeToTheRight(FOUR, "a.md"))).toEqual(["^README.md", "a.md"]);
  expect(closeToTheRight(FOUR, "a.md").front).toBe("a.md");
  const left = closeUnpinned(FOUR);
  expect(paths(left)).toEqual(["^README.md"]);
  expect(left.front).toBe("README.md");
});

test("pinning keeps a tab and moves it last of the pinned; unpinning moves it first of the others", () => {
  const withPreview = open(FOUR, "p.md", "preview", "rendered");
  let set = setPinned(withPreview, "p.md", true);
  expect(paths(set)).toEqual(["^README.md", "^p.md", "a.md", "b.md", "c.md"]);
  set = setPinned(set, "README.md", false);
  expect(paths(set)).toEqual(["^p.md", "README.md", "a.md", "b.md", "c.md"]);
  expect(setPinned(set, "p.md", true)).toBe(set);
});

test("a tab moves among the pinned or the others, never across", () => {
  expect(movesWithin(FOUR.tabs, 0)).toEqual([0, 0]);
  expect(movesWithin(FOUR.tabs, 2)).toEqual([1, 3]);
  expect(paths(move(FOUR, 3, 1))).toEqual(["^README.md", "c.md", "a.md", "b.md"]);
  expect(paths(move(FOUR, 1, 9))).toEqual(["^README.md", "b.md", "c.md", "a.md"]);
  expect(move(FOUR, 1, 0)).toBe(FOUR);
  expect(move(FOUR, 0, 3)).toBe(FOUR);
});

test("each tab keeps its own mode", () => {
  const set = setMode(FOUR, "a.md", "split");
  expect(set.tabs.map((tab) => tab.mode)).toEqual(["rendered", "split", "raw", "rendered"]);
  expect(setMode(set, "a.md", "split")).toBe(set);
});

test("tabs whose files are gone close, and the next one comes forward", () => {
  const set = prune(FOUR, (path) => path !== "b.md");
  expect(paths(set)).toEqual(["^README.md", "a.md", "c.md"]);
  expect(set.front).toBe("c.md");
  expect(prune(FOUR, () => true)).toBe(FOUR);
});

test("Ctrl+Tab goes round the tabs", () => {
  expect(step(FOUR, 1)).toBe("c.md");
  expect(step({ ...FOUR, front: "c.md" }, 1)).toBe("README.md");
  expect(step({ ...FOUR, front: "README.md" }, -1)).toBe("c.md");
  expect(step(NO_TABS, 1)).toBe("");
});

test("two files of one name are told apart by their folders", () => {
  const set = openAll(NO_TABS, ["README.md", "docs/api/README.md", "docs/setup.md"], "rendered");
  expect(label(set.tabs, set.tabs[0])).toEqual({ name: "README.md", folder: "" });
  expect(label(set.tabs, set.tabs[1])).toEqual({ name: "README.md", folder: "docs/api" });
  expect(label(set.tabs, set.tabs[2])).toEqual({ name: "setup.md", folder: "" });
});

test("the way to a file is its project, each folder down to it, and the file", () => {
  expect(pathSteps("notes", "docs/api/README.md")).toEqual([
    { kind: "project", label: "notes", folder: "" },
    { kind: "folder", label: "docs", folder: "docs" },
    { kind: "folder", label: "api", folder: "docs/api" },
    { kind: "file", label: "README.md", folder: "docs/api" },
  ]);
  expect(pathSteps("notes", "README.md")).toEqual([
    { kind: "project", label: "notes", folder: "" },
    { kind: "file", label: "README.md", folder: "" },
  ]);
});

test("the tabs read back as saved, pinned first, one preview at most", () => {
  expect(restore(saved(FOUR), "b.md")).toEqual(FOUR);
  const odd = restore(
    [
      { file: "a.md", pinned: false, preview: true, mode: "rendered" },
      { file: "b.md", pinned: true, preview: false, mode: "raw" },
      { file: "c.md", pinned: false, preview: true, mode: "rendered" },
    ],
    "gone.md",
  );
  expect(paths(odd)).toEqual(["^b.md", "a.md?", "c.md"]);
  expect(odd.front).toBe("b.md");
});
