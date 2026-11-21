import { expect, test } from "nsh/test";
import { TextView, type TextViewInline } from "nsh/base";
import type { HeadingInfo, MarkInfo, MatchInfo, SearchResults } from "mdviewer/native";
import { MARKDOWN } from "./views/document";
import {
  countIn,
  excerptParts,
  headingCounts,
  highlighted,
  inProject,
  lineMarks,
  lineParts,
  matchMarks,
  matchOf,
  matchRanges,
  nearestMatch,
  onThePage,
  ordinal,
  pageOffset,
  pagePattern,
  placesFor,
  resultItems,
  resultTree,
  rowCount,
  rowId,
  rowOf,
  searchSummary,
  stepFrom,
} from "./views/search";

const at = (line: number, start: number, end: number, excerpt = ""): MatchInfo => ({
  startLine: line,
  startColumn: start,
  endLine: line,
  endColumn: end,
  excerpt,
  excerptStart: start,
  excerptEnd: end,
});

const heading = (level: number, title: string, line: number): HeadingInfo => ({ level, title, line });

const NOTES = "D:\\notes";

function results(counts: Record<string, number>, project = NOTES): SearchResults {
  const files = Object.entries(counts).map(([path, count]) => ({ project, path, matches: Array.from({ length: count }, (_, index) => at(index, 0, 1)) }));
  return { files, total: files.reduce((sum, file) => sum + file.matches.length, 0), truncated: false, problem: "" };
}

function both(one: SearchResults, two: SearchResults): SearchResults {
  return { ...one, files: [...one.files, ...two.files], total: one.total + two.total };
}

test("each scope looks where it says: the file in front, the tabs, the project, every project with the open one first", () => {
  expect(placesFor("file", NOTES, "docs/setup.md", [], [])).toEqual([{ project: NOTES, everyFile: false, files: ["docs/setup.md"] }]);
  expect(placesFor("file", NOTES, "", [], [])).toEqual([]);
  expect(placesFor("tabs", NOTES, "", ["README.md", "docs/setup.md"], [])).toEqual([{ project: NOTES, everyFile: false, files: ["README.md", "docs/setup.md"] }]);
  expect(placesFor("project", NOTES, "README.md", [], [])).toEqual([{ project: NOTES, everyFile: true, files: [] }]);
  expect(placesFor("all", NOTES, "", [], ["D:\\blog", NOTES]).map((place) => place.project)).toEqual([NOTES, "D:\\blog"]);
  expect(placesFor("project", "", "", [], [])).toEqual([]);
});

test("the marks and F3 go by the open project's matches alone", () => {
  const all = both(results({ "README.md": 2 }), results({ "post.md": 3 }, "D:\\blog"));
  const here = inProject(all, NOTES);
  expect(here?.files.map((file) => file.path)).toEqual(["README.md"]);
  expect(here?.total).toBe(2);
  // One project's results come back as they were.
  const one = results({ "README.md": 2 });
  expect(inProject(one, NOTES)).toBe(one);
});

test("a row's id says which project, file and match it is", () => {
  for (const row of [
    { kind: "project" as const, project: NOTES },
    { kind: "file" as const, project: NOTES, file: "docs/setup.md" },
    { kind: "match" as const, project: NOTES, file: "docs/setup.md", index: 3 },
  ]) {
    expect(rowOf(rowId(row))).toEqual(row);
  }
  expect(rowOf("docs/")).toBe(null);
});

test("the drop-down lists each match under its file, up to the most it lists", () => {
  const found = results({ "README.md": 2, "docs/setup.md": 3 });
  const items = resultItems(found);
  expect(items.length).toBe(5);
  expect(items[2].group).toBe(rowId({ kind: "file", project: NOTES, file: "docs/setup.md" }));
  expect(rowOf(items[2].id)).toEqual({ kind: "match", project: NOTES, file: "docs/setup.md", index: 0 });
  expect(resultItems(found, 3).length).toBe(3);
  expect(resultItems(null)).toEqual([]);
});

test("the files panel's results are the files, each holding its matches, under their projects when there are several", () => {
  const found = results({ "README.md": 1, "docs/setup.md": 2 });
  const tree = resultTree(found, new Map());
  expect(tree.map((item) => item.label)).toEqual(["README.md", "setup.md"]);
  expect(tree[1].children?.length).toBe(2);
  expect(rowCount(found, tree[1].id)).toBe(2);

  const all = both(found, results({ "post.md": 1 }, "D:\\blog"));
  const projects = resultTree(all, new Map([[NOTES, "notes"]]));
  expect(projects.map((item) => item.label)).toEqual(["notes", "D:\\blog"]);
  expect(rowCount(all, projects[0].id)).toBe(3);
});

test("the results tab marks the parts of each line its matches cover, a match across lines on each of them", () => {
  const across: MatchInfo = { ...at(2, 4, 0), endLine: 4, endColumn: 3 };
  const marks = lineMarks([at(1, 0, 3), at(1, 5, 7), across]);
  expect(marks.get(1)).toEqual([
    [0, 3],
    [5, 7],
  ]);
  expect(marks.get(2)).toEqual([[4, Number.MAX_SAFE_INTEGER]]);
  expect(marks.get(3)).toEqual([[0, Number.MAX_SAFE_INTEGER]]);
  expect(marks.get(4)).toEqual([[0, 3]]);
  expect(lineParts("the toolchain is Rust", [[4, 13]])).toEqual([
    { text: "the ", found: false },
    { text: "toolchain", found: true },
    { text: " is Rust", found: false },
  ]);
  expect(lineParts("short", [[2, Number.MAX_SAFE_INTEGER]])).toEqual([
    { text: "sh", found: false },
    { text: "ort", found: true },
  ]);
  expect(lineParts("plain")).toEqual([{ text: "plain", found: false }]);
});

test("a press on a line of the results goes to the match on it, else the nearest", () => {
  const matches = [at(3, 0, 1), at(10, 0, 1), at(20, 0, 1)];
  expect(nearestMatch(matches, 10)).toBe(1);
  expect(nearestMatch(matches, 12)).toBe(1);
  expect(nearestMatch(matches, 18)).toBe(2);
  expect(nearestMatch([], 4)).toBe(-1);
});

test("a match's row has its excerpt, cut at the match", () => {
  const line = at(4, 4, 13, "The toolchain is Rust.");
  const found: SearchResults = { files: [{ project: NOTES, path: "docs/setup.md", matches: [line] }], total: 1, truncated: false, problem: "" };
  const id = rowId({ kind: "match", project: NOTES, file: "docs/setup.md", index: 0 });
  expect(matchOf(found, id)).toEqual({ match: line, folder: "docs" });
  expect(excerptParts(line)).toEqual(["The ", "toolchain", " is Rust."]);
  expect(matchOf(found, rowId({ kind: "file", project: NOTES, file: "docs/setup.md" }))).toBe(undefined);
});

const ORDER = ["README.md", "todo.md", "docs/build.md", "docs/setup.md"];

test("F3 steps through a file's matches, then on to the next file's, around the ends", () => {
  const found = results({ "README.md": 2, "docs/setup.md": 1 });
  expect(stepFrom(found, { file: "README.md", index: 0 }, "README.md", ORDER, 1)).toEqual({ file: "README.md", index: 1 });
  expect(stepFrom(found, { file: "README.md", index: 1 }, "README.md", ORDER, 1)).toEqual({ file: "docs/setup.md", index: 0 });
  expect(stepFrom(found, { file: "docs/setup.md", index: 0 }, "docs/setup.md", ORDER, 1)).toEqual({ file: "README.md", index: 0 });
  // Shift+F3 goes back, to the last match of the file before.
  expect(stepFrom(found, { file: "docs/setup.md", index: 0 }, "docs/setup.md", ORDER, -1)).toEqual({ file: "README.md", index: 1 });
});

test("the first step starts in the file in front, else in the nearest file after it with a match", () => {
  const found = results({ "README.md": 2, "docs/setup.md": 3 });
  expect(stepFrom(found, null, "docs/setup.md", ORDER, 1)).toEqual({ file: "docs/setup.md", index: 0 });
  expect(stepFrom(found, null, "docs/setup.md", ORDER, -1)).toEqual({ file: "docs/setup.md", index: 2 });
  // todo.md has none: on to setup.md, or back to the README.
  expect(stepFrom(found, null, "todo.md", ORDER, 1)).toEqual({ file: "docs/setup.md", index: 0 });
  expect(stepFrom(found, null, "todo.md", ORDER, -1)).toEqual({ file: "README.md", index: 1 });
  // A match that has gone since steps as if there were none.
  expect(stepFrom(found, { file: "todo.md", index: 4 }, "README.md", ORDER, 1)).toEqual({ file: "README.md", index: 0 });
  expect(stepFrom(results({}), null, "README.md", ORDER, 1)).toBe(null);
});

test("the status bar says which match of how many, in how many files", () => {
  const found = results({ "README.md": 2, "docs/setup.md": 10 });
  expect(ordinal(found, { file: "docs/setup.md", index: 1 })).toBe(4);
  expect(searchSummary(found, { file: "docs/setup.md", index: 1 })).toBe("Match 4 of 12 · 2 files");
  expect(searchSummary(found, null)).toBe("12 matches · 2 files");
  expect(searchSummary({ ...found, truncated: true }, null)).toBe("12+ matches · 2 files");
  expect(searchSummary(results({}), null)).toBe("No matches");
  expect(countIn(found, "README.md")).toBe(2);
  expect(countIn(found, "todo.md")).toBe(0);
  expect(countIn(null, "README.md")).toBe(0);
});

test("the outline counts the matches under each heading, none for those above the first", () => {
  const headings = [heading(1, "One", 2), heading(2, "Two", 6), heading(1, "Three", 10)];
  expect(headingCounts([at(0, 0, 1), at(2, 0, 1), at(7, 0, 1), at(8, 0, 1), at(12, 0, 1)], headings)).toEqual([1, 2, 1]);
});

test("Raw marks each line that holds a match once, beside it and on the ruler", () => {
  const marks = matchMarks([at(3, 0, 2), at(3, 5, 7), at(9, 1, 2)], "#f59e0b");
  expect(marks.map((mark) => mark.range.start.line)).toEqual([3, 9]);
  expect(marks[0].gutter?.style).toEqual({ backgroundColor: "#f59e0b" });
  expect(marks[0].overviewRuler?.lane).toBe("center");
});

test("the page reads the query as Raw does: as written, in any case, or as a pattern", () => {
  const plain = pagePattern({ text: "a.b", caseSensitive: false, wholeWord: false, regex: false });
  expect(matchRanges("A.B axb a.b", plain!, false)).toEqual([
    [0, 3],
    [8, 11],
  ]);
  const cased = pagePattern({ text: "Rust", caseSensitive: true, wholeWord: false, regex: false });
  expect(matchRanges("rust Rust", cased!, false)).toEqual([[5, 9]]);
  const words = pagePattern({ text: "ab", caseSensitive: false, wholeWord: true, regex: false });
  expect(matchRanges("xab ab abc", words!, true)).toEqual([[4, 6]]);
  const pattern = pagePattern({ text: "\\d+", caseSensitive: false, wholeWord: false, regex: true });
  expect(matchRanges("v1.95", pattern!, false)).toEqual([
    [1, 2],
    [3, 5],
  ]);
  expect(pagePattern({ text: "(abc", caseSensitive: false, wholeWord: false, regex: true })).toBe(null);
  expect(pagePattern(null)).toBe(null);
});

function highlights(inlines: TextViewInline[]): [string, string][] {
  const found: [string, string][] = [];
  for (const each of inlines) {
    if (each.type === "highlight") found.push([each.content.map((part) => (part.type === "text" ? part.text : "")).join(""), each.color ?? ""]);
    else if ("content" in each && Array.isArray(each.content)) found.push(...highlights(each.content));
  }
  return found;
}

test("the page highlights each match in its text, inside links and strong text too, the current one in its own color", () => {
  const document = TextView.parse("The toolchain, **the toolchain**, [toolchain](setup.md) and `toolchain`.\n", MARKDOWN);
  const pattern = pagePattern({ text: "toolchain", caseSensitive: false, wholeWord: false, regex: false });
  const shown = highlighted(document, pattern, false, 1, { match: "m", current: "c" });
  expect(shown.count).toBe(3);
  const paragraph = shown.document.blocks[0];
  expect(paragraph.type).toBe("paragraph");
  if (paragraph.type !== "paragraph") return;
  expect(highlights(paragraph.content)).toEqual([
    ["toolchain", "m"],
    ["toolchain", "c"],
    ["toolchain", "m"],
  ]);
  // The text around a match stays as it was.
  expect(paragraph.content[0]).toEqual({ type: "text", text: "The " });
  // No query, no change.
  expect(highlighted(document, null, false, -1, { match: "m", current: "c" }).document).toBe(document);
});

test("the match the reader is on is found on the page by its section, counting only what the page shows as text", () => {
  // Line 0 a match in code, line 2 the heading, lines 4 and 5 matches in text.
  const matches = [at(0, 1, 5), at(4, 0, 4), at(5, 0, 4)];
  const headings = [heading(1, "Part", 2)];
  const marks: MarkInfo[] = [{ kind: "code", startLine: 0, startColumn: 1, endLine: 0, endColumn: 5 }];
  expect(onThePage(matches, 0, headings, marks)).toBe(null);
  expect(onThePage(matches, 1, headings, marks)).toEqual({ section: 1, nth: 0 });
  expect(onThePage(matches, 2, headings, marks)).toEqual({ section: 1, nth: 1 });
});

test("the page goes as far into a match's section as its line is into the section's lines", () => {
  const headings = [heading(1, "One", 10), heading(1, "Two", 20)];
  // Sections start at 0, 100 and 300 points; the page ends at 400.
  const tops = [0, 100, 300];
  expect(pageOffset(15, headings, 30, tops, 400)).toBe(200);
  expect(pageOffset(25, headings, 30, tops, 400)).toBe(350);
  expect(pageOffset(5, headings, 30, tops, 400)).toBe(50);
  expect(pageOffset(15, headings, 30, [0], 400)).toBe(undefined);
});
