import { expect, test } from "nsh/test";
import type { FileEntry, HeadingInfo, MatchInfo } from "mdviewer/native";
import { followLink } from "./views/actions";
import { listing, projects, tabSet } from "./views/model";
import { fromPage, headingTops, keystrokeOf, lineAtPlace, normalized, type PageBlock, pageFind, readingAt, syncPlaces } from "./views/pagemessages";
import { pagePattern } from "./views/search";
import { NO_TABS, open } from "./views/tabs";

const block = (line: number, top: number, kind = "p", depth = 0, endLine = line, height = 20): PageBlock => ({ line, endLine, top, height, kind, depth });

const heading = (line: number, title = ""): HeadingInfo => ({ level: 1, title, line });

test("a message from the page is checked field by field", () => {
  expect(fromPage("not JSON")).toBe(null);
  expect(fromPage("[]")).toBe(null);
  expect(fromPage('{"type":"navigate"}')).toBe(null);
  expect(fromPage('{"type":"link","url":"docs/a.md","control":"yes"}')).toEqual({ type: "link", url: "docs/a.md", control: false });
  expect(fromPage('{"type":"scroll","y":"12","viewport":600,"content":2000}')).toEqual({ type: "scroll", y: 0, viewport: 600, content: 2000 });
  expect(fromPage('{"type":"found","count":3,"places":[40,"x",900]}')).toEqual({ type: "found", count: 3, places: [40, 900] });
  expect(fromPage('{"type":"layout","height":900,"blocks":[{"line":3,"endLine":4,"top":40,"height":20,"kind":"h1","depth":0},7]}')).toEqual({
    type: "layout",
    height: 900,
    blocks: [{ line: 3, endLine: 4, top: 40, height: 20, kind: "h1", depth: 0 }],
  });
});

test("an outline heading is the top-level heading block on its line", () => {
  const blocks = [block(0, 24, "h1"), block(2, 70), block(4, 120, "h2", 1), block(6, 160, "h2"), block(6, 190, "p")];
  expect(headingTops([heading(0), heading(4), heading(6), heading(9)], blocks)).toEqual([24, undefined, 160, undefined]);
});

test("the reader is under the last heading above the top of the view", () => {
  const tops = [24, undefined, 400, 900];
  expect(readingAt(tops, 0, 24)).toBe(0);
  expect(readingAt(tops, 0, 0)).toBe(-1);
  expect(readingAt(tops, 380, 24)).toBe(2);
  expect(readingAt(tops, 5000, 24)).toBe(3);
});

test("a key from the page is spelled as the app binds it", () => {
  const key = (code: string, mods: Partial<{ control: boolean; shift: boolean; alt: boolean }> = {}) =>
    keystrokeOf({ key: "", code, control: false, shift: false, alt: false, ...mods });
  expect(key("KeyW", { control: true })).toBe(normalized("mod+w"));
  expect(key("KeyT", { control: true, shift: true })).toBe(normalized("mod+shift+t"));
  expect(key("Tab", { control: true, shift: true })).toBe(normalized("ctrl+shift+tab"));
  expect(key("F3", { shift: true })).toBe(normalized("shift+f3"));
  expect(key("Escape")).toBe(normalized("escape"));
  expect(key("Equal", { control: true })).toBe(normalized("mod+="));
  expect(normalized("Shift+Mod+T")).toBe("ctrl+shift+t");
});

test("the line at a place on the page is as far into the innermost block there as the place is into the block", () => {
  // A list on lines 4 to 7, 100 pixels tall from 200, its second item on lines 6 and 7.
  const blocks = [block(0, 24, "h1"), block(4, 200, "ul", 0, 7, 100), block(4, 200, "li", 1, 5, 50), block(6, 250, "li", 1, 7, 50)];
  expect(lineAtPlace(blocks, 276)).toBe(7);
  expect(lineAtPlace(blocks, 210)).toBe(4);
  // Between blocks, the last line above.
  expect(lineAtPlace(blocks, 100)).toBe(0);
  expect(lineAtPlace(blocks, 0)).toBe(0);
});

const entry = (path: string): FileEntry => ({ path, name: path.split("/").pop() ?? path, folder: path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "" });

test("a link pressed on the page opens its file as the preview, or with Ctrl in a tab of its own", () => {
  projects.set({ projects: [{ path: "Notes", name: "Notes", lastFile: "", tabs: [] }], selected: "Notes" });
  listing.set({ files: ["README.md", "docs/sensors.md", "docs/setup.md"].map(entry), truncated: false, problem: "" });
  tabSet.set(open(NO_TABS, "README.md", "preview", "rendered"));
  const shown = () => tabSet().tabs.map((tab) => `${tab.path}${tab.preview ? "?" : ""}`);

  followLink("docs/sensors.md");
  expect(shown()).toEqual(["docs/sensors.md?"]);
  followLink("setup.md#first-steps", true);
  expect(shown()).toEqual(["docs/sensors.md?", "docs/setup.md"]);
  expect(tabSet().front).toBe("docs/setup.md");
  // The page goes to its own anchors; one it couldn't find goes nowhere.
  followLink("#missing");
  followLink("../../outside.md");
  expect(tabSet().front).toBe("docs/setup.md");
  projects.set({ projects: [], selected: "" });
});

const match = (line: number): MatchInfo => ({ startLine: line, startColumn: 0, endLine: line, endColumn: 4, excerpt: "", excerptStart: 0, excerptEnd: 4 });

test("the page's search is the query as a JavaScript pattern, with the line of each of the app's matches", () => {
  const find = pageFind(pagePattern({ text: "a.b", caseSensitive: true, wholeWord: true, regex: false }), true, [match(2), match(9)], 1, 4, 160);
  expect(find).toEqual({ source: "a\\.b", flags: "gmu", wholeWord: true, lines: [2, 9], current: 1, reveal: 4, above: 160 });
  // A pattern JavaScript can't read finds nothing on the page.
  expect(pageFind(pagePattern({ text: "(?<x", caseSensitive: false, wholeWord: false, regex: true }), false, [], -1, 0, 160).source).toBe("");
});

test("Split lines up at the first line of each block, every heading kept when there are too many", () => {
  // A list and its first item start on one line: one place.
  const blocks = [block(0, 24, "h1"), block(2, 70), block(4, 120, "ul", 0, 6), block(4, 120, "li", 1), block(5, 146, "li", 1), block(6, 172, "li", 1), block(8, 220, "h2")];
  expect(syncPlaces(blocks, 100)).toEqual([
    { line: 0, top: 24 },
    { line: 2, top: 70 },
    { line: 4, top: 120 },
    { line: 5, top: 146 },
    { line: 6, top: 172 },
    { line: 8, top: 220 },
  ]);
  expect(syncPlaces(blocks, 3).map((place) => place.line)).toEqual([0, 2, 8]);
});
